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

        // 启动超时检查任务
        let states = service.shared_states.clone();
        let command_tx = service.command_tx.clone();
        let notify = service.notify.clone();
        tokio::spawn(async move {
            loop {
                notify.notified().await;
                let mut states_lock = states.lock().await;
                if states_lock.server_ids.len() == 50 {
                    continue;
                }
                let command = Command {
                    command: 1,
                    data: serde_json::to_string(&states_lock.states).unwrap_or_else(|e| {
                        tracing::error!("序列化状态信息失败: {}", e);
                        String::new()
                    }),
                    server_ids: states_lock.server_ids.iter().copied().collect(),
                };
                if let Err(e) = command_tx.send(command) {
                    tracing::error!("转发状态信息失败: {}", e);
                }
                states_lock.states.clear();
                states_lock.server_ids.clear();
            }
        });

        service
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
            match request {
                Ok(req) => {
                    tracing::info!("收到主机信息请求: {:?}", req);
                    // 处理主机信息，存储到数据库
                    let host_info = req
                        .host
                        .ok_or_else(|| Status::invalid_argument("缺少主机信息"))?;
                    // TODO: 实现数据库存储逻辑
                    tracing::info!("存储主机信息: {:?}", host_info);
                }
                Err(e) => {
                    tracing::error!("接收主机信息请求错误: {:?}", e);
                    break;
                }
            }
        }
        Ok(Response::new(ServerResponse { success: true }))
    }

    async fn report_server_state(
        &self,
        request: Request<Streaming<StateRequest>>,
    ) -> Result<Response<ServerResponse>, Status> {
        let mut stream = request.into_inner();
        let shared_states = self.shared_states.clone();
        while let Some(request) = stream.next().await {
            match request {
                Ok(req) => {
                    let mut states_lock = shared_states.lock().await;

                    let state = req
                        .state
                        .ok_or_else(|| Status::invalid_argument("缺少状态信息"))?;
                    let agent_info = req
                        .agent_info
                        .ok_or_else(|| Status::invalid_argument("缺少探针信息"))?;

                    states_lock.states.push(state);
                    states_lock.server_ids.insert(agent_info.server_id);
                    self.notify.notify_one();
                    break;
                }
                Err(e) => {
                    tracing::error!("接收状态请求错误: {:?}", e);
                    break;
                }
            }
        }
        Ok(Response::new(ServerResponse { success: true }))
    }

    async fn update_ip(
        &self,
        request: Request<UpdateIpRequest>,
    ) -> Result<Response<ServerResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("收到IP更新请求: {:?}", req);
        // 更新IP地址
        let agent_info = req
            .agent_info
            .ok_or_else(|| Status::invalid_argument("缺少探针信息"))?;
        let server_id = agent_info.server_id;
        let ip = req.ipv4;
        // TODO: 实现IP更新逻辑
        tracing::info!("更新服务器 {} 的IP地址为 {}", server_id, ip);
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
            let timeout = tokio::time::Duration::from_secs(30); // 设置超时时间
            let start = tokio::time::Instant::now();

            loop {
                tokio::select! {
                    Ok(command) = command_rx.recv() => {
                        if let Err(e) = tx.send(Ok(command)).await {
                            tracing::error!("转发WebSocket命令失败: {}", e);
                            break;
                        }
                    }
                    Some(request) = stream.next() => {
                        match request {
                            Ok(req) => {
                                tracing::info!("收到gRPC命令: {:?}", req);
                                let command = Command {
                                    command: 0, // 使用请求中的实际值
                                    data: "ok".into(),
                                    server_ids: vec![req.agent_info.unwrap().server_id],
                                };
                                if let Err(e) = tx.send(Ok(command)).await {
                                    tracing::error!("转发gRPC命令失败: {}", e);
                                    break;
                                }
                            },
                            Err(e) => {
                                tracing::error!("接收gRPC命令错误: {:?}", e);
                                break;
                            }
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
