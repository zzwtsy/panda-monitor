use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

use common::panda_monitor::{
    panda_monitor_server::PandaMonitor, Command, CommandRequest, HostRequest, ServerResponse,
    State, StateRequest, UpdateIpRequest,
};
use futures_util::StreamExt;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

/// 共享状态
#[derive(Debug)]
pub struct SharedState {
    /// 探针状态
    states: Vec<State>,
    /// 探针ID
    server_ids: HashSet<u64>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            states: Vec::new(),
            server_ids: HashSet::new(),
        }
    }
}

// 定义常量
const COMMAND_TIMEOUT_SECONDS: u64 = 30; // 命令处理超时时间
const MAX_SERVER_COUNT: usize = 50; // TODO: 暂时硬编码，最终从 websocket 中获取需要发送的探针 id 计算探针数量

#[derive(Debug)]
pub struct PandaMonitorService {
    command_tx: Sender<Command>,
    shared_states: Arc<Mutex<SharedState>>,
    notify: Arc<Notify>,
}

impl PandaMonitorService {
    pub fn new(command_tx: Sender<Command>) -> Self {
        let service = Self {
            command_tx,
            shared_states: Arc::new(Mutex::new(SharedState::new())),
            notify: Arc::new(Notify::new()),
        };

        // 启动后台状态检查任务
        Self::start_state_check_task(
            service.shared_states.clone(),
            service.command_tx.clone(),
            service.notify.clone(),
        );

        service
    }

    /// 启动状态检查后台任务
    fn start_state_check_task(
        states: Arc<Mutex<SharedState>>,
        command_tx: Sender<Command>,
        notify: Arc<Notify>,
    ) {
        tokio::spawn(async move {
            loop {
                notify.notified().await;
                let mut states_lock = states.lock().await;

                // 如果服务器数量达到上限，跳过处理
                if states_lock.server_ids.len() == MAX_SERVER_COUNT {
                    continue;
                }

                // 序列化状态信息
                let serialized_data = match serde_json::to_string(&states_lock.states) {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::error!("序列化状态信息失败: {}", e);
                        String::new()
                    }
                };

                // 构建命令并发送
                let command = Command {
                    command: 1,
                    data: serialized_data,
                    server_ids: states_lock.server_ids.iter().copied().collect(),
                };

                if let Err(e) = command_tx.send(command) {
                    tracing::error!("转发状态信息失败: {}", e);
                }

                // 清理已处理的状态
                states_lock.states.clear();
                states_lock.server_ids.clear();
            }
        });
    }
}

#[tonic::async_trait]
impl PandaMonitor for PandaMonitorService {
    async fn report_server_host(
        &self,
        request: Request<Streaming<HostRequest>>,
    ) -> Result<Response<ServerResponse>, Status> {
        let mut stream = request.into_inner();
        while let Some(request) = stream.next().await {
            let req = request.map_err(|e| {
                tracing::error!("接收主机信息请求错误: {:?}", e);
                Status::internal("接收请求失败")
            })?;

            let host_info = req.host.ok_or(Status::invalid_argument("缺少主机信息"))?;
            tracing::info!("存储主机信息: {:?}", host_info);
            // TODO: 实现数据库存储逻辑
        }
        Ok(Response::new(ServerResponse { success: true }))
    }

    async fn report_server_state(
        &self,
        request: Request<Streaming<StateRequest>>,
    ) -> Result<Response<ServerResponse>, Status> {
        let mut stream = request.into_inner();
        let shared_states = self.shared_states.clone();

        if let Some(request) = stream.next().await {
            let req = request.map_err(|e| {
                tracing::error!("接收状态请求错误: {:?}", e);
                Status::internal("接收请求失败")
            })?;

            let state = req.state.ok_or(Status::invalid_argument("缺少状态信息"))?;
            let agent_info = req
                .agent_info
                .ok_or(Status::invalid_argument("缺少探针信息"))?;

            let mut states_lock = shared_states.lock().await;
            states_lock.states.push(state);
            states_lock.server_ids.insert(agent_info.server_id);
            self.notify.notify_one();
        }

        Ok(Response::new(ServerResponse { success: true }))
    }

    async fn update_ip(
        &self,
        request: Request<UpdateIpRequest>,
    ) -> Result<Response<ServerResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("收到IP更新请求: {:?}", req);

        let agent_info = req
            .agent_info
            .ok_or(Status::invalid_argument("缺少探针信息"))?;
        let server_id = agent_info.server_id;
        let ip = req.ipv4;

        tracing::info!("更新服务器 {} 的IP地址为 {}", server_id, ip);
        // TODO: 实现IP更新逻辑
        Ok(Response::new(ServerResponse { success: true }))
    }

    type SendCommandStream = ReceiverStream<Result<Command, Status>>;

    async fn send_command(
        &self,
        request: Request<Streaming<CommandRequest>>,
    ) -> Result<Response<Self::SendCommandStream>, Status> {
        tracing::info!("收到命令请求");
        let mut command_rx = self.command_tx.subscribe();
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);
        let response_stream = ReceiverStream::new(rx);

        tokio::spawn(async move {
            let timeout = tokio::time::Duration::from_secs(COMMAND_TIMEOUT_SECONDS);
            let start = tokio::time::Instant::now();

            loop {
                tokio::select! {
                    Ok(command) = command_rx.recv() => {
                        if tx.send(Ok(command)).await.is_err() {
                            tracing::error!("转发WebSocket命令失败");
                            break;
                        }
                    }
                    Some(request) = stream.next() => {
                        if let Err(e) = Self::handle_grpc_command(&tx, request).await {
                            tracing::error!("处理gRPC命令失败: {:?}", e);
                            break;
                        }
                    }
                    _ = tokio::time::sleep_until(start + timeout) => {
                        tracing::warn!("命令处理超时");
                        break;
                    }
                    else => break,
                }
            }
        });

        Ok(Response::new(response_stream))
    }
}

impl PandaMonitorService {
    async fn handle_grpc_command(
        tx: &mpsc::Sender<Result<Command, Status>>,
        request: Result<CommandRequest, Status>,
    ) -> Result<(), Status> {
        let req = request?;
        tracing::info!("收到gRPC命令: {:?}", req);

        let command = Command {
            command: 0,
            data: "ok".into(),
            server_ids: vec![
                req.agent_info
                    .ok_or(Status::invalid_argument("缺少探针信息"))?
                    .server_id,
            ],
        };

        tx.send(Ok(command))
            .await
            .map_err(|_| Status::internal("发送命令失败"))?;
        Ok(())
    }
}
