use crate::fetch_ip::fetch_geo_ip;
use crate::{command::Command, system_info::SystemInfoCollector};
use common::panda_monitor::{
    panda_monitor_client::PandaMonitorClient, AgentInfo, CommandRequest, Host, HostRequest, State,
    StateRequest, UpdateIpRequest,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time;
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::tokio_stream::StreamExt;
use tonic::transport::Channel;
use tonic::Status;

// 常量定义
const VERSION: &'static str = include_str!(concat!(env!("OUT_DIR"), "/VERSION"));
const GRPC_TIMEOUT_SECS: u64 = 10; // gRPC请求超时时间
const RETRY_ATTEMPTS: u32 = 3; // 操作重试次数
const RETRY_DELAY_SECS: u64 = 2; // 重试间隔时间

/// 服务器监控代理
#[derive(Debug)]
pub struct ServerMonitorAgent {
    client: PandaMonitorClient<Channel>, // gRPC客户端
    server_id: u64,                      // 服务器ID
    system_info: SystemInfoCollector,    // 系统信息收集器
    report_state: bool,                  // 是否上报状态
}

impl ServerMonitorAgent {
    /// 创建新的监控代理实例
    pub async fn new(command: Command) -> anyhow::Result<Self> {
        let url = format!("grpc://{}:{}", command.url, command.port);

        // 添加连接重试机制
        let mut attempts = 0;
        let channel = loop {
            match Channel::from_shared(url.clone())?
                .timeout(Duration::from_secs(GRPC_TIMEOUT_SECS))
                .connect_timeout(Duration::from_secs(GRPC_TIMEOUT_SECS))
                .concurrency_limit(256)
                .connect()
                .await
            {
                Ok(channel) => break channel,
                Err(e) => {
                    attempts += 1;
                    if attempts >= RETRY_ATTEMPTS {
                        return Err(anyhow::anyhow!(
                            "连接服务器失败，已重试 {} 次: {}",
                            RETRY_ATTEMPTS,
                            e
                        ));
                    }
                    println!(
                        "连接失败，正在重试 ({}/{}): {}",
                        attempts, RETRY_ATTEMPTS, e
                    );
                    time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                }
            }
        };

        Ok(Self {
            client: PandaMonitorClient::new(channel),
            server_id: command.agent_id,
            system_info: SystemInfoCollector::new(),
            report_state: false,
        })
    }

    /// 发送命令并处理响应
    pub async fn send_command(&mut self) -> anyhow::Result<()> {
        let mut attempts = 0;

        while attempts < RETRY_ATTEMPTS {
            match self.try_send_command().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    attempts += 1;
                    if attempts == RETRY_ATTEMPTS {
                        return Err(anyhow::anyhow!(
                            "发送命令失败，已重试 {} 次: {}",
                            RETRY_ATTEMPTS,
                            e
                        )
                        .into());
                    }
                    println!(
                        "发送命令失败，正在重试 ({}/{}): {}",
                        attempts, RETRY_ATTEMPTS, e
                    );
                    time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                }
            }
        }

        Ok(())
    }

    /// 尝试发送单个命令
    async fn try_send_command(&mut self) -> anyhow::Result<()> {
        let mut client = self.client.clone();
        let (tx, rx) = mpsc::channel(128);

        let command_request = self.create_command_request();
        tx.send(command_request)
            .await
            .map_err(|e| anyhow::anyhow!("发送命令请求失败: {}", e))?;

        let mut stream = client
            .send_command(ReceiverStream::new(rx))
            .await
            .map_err(|e| anyhow::anyhow!("创建命令流失败: {}", e))?
            .into_inner();

        while let Some(result) = stream.next().await {
            self.parse_command(result).await?;
        }

        Ok(())
    }

    /// 解析并处理收到的命令
    async fn parse_command(
        &mut self,
        command: Result<common::panda_monitor::Command, Status>,
    ) -> anyhow::Result<()> {
        let command = command?;

        // 验证服务器 ID
        if !command.server_ids.contains(&self.server_id) {
            return Ok(()); // ID不匹配时忽略命令
        }

        match command.data.as_str() {
            "stop_report_state" => {
                self.shutdown().await?;
            }
            "report_state" => {
                self.report_state = true;
                self.start_reporting_state().await?;
            }
            "report_host" => {
                self.refresh_system_components();
                self.create_host_request().await;
            }
            "report_ip" => {
                self.create_update_ip_request().await;
            }
            _ => println!("未知命令: {}", command.data),
        }

        Ok(())
    }

    /// 开始定期上报状态
    async fn start_reporting_state(&mut self) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        
        while self.report_state {
            let start = tokio::time::Instant::now();
            if let Err(e) = self.report_server_state().await {
                eprintln!("状态上报失败: {}", e);
            }
           
            // 计算剩余时间
            let elapsed = start.elapsed();
            if elapsed < Duration::from_secs(1) {
                interval.tick().await;
            } else {
                // 如果超时，立即开始下一轮
                interval.reset();
            }
        }
        Ok(())
    }

    /// 上报服务器状态
    async fn report_server_state(&mut self) -> anyhow::Result<()> {
        // 上报前检查连接状态
        // if let Err(e) = self.check_connection().await {
        //     eprintln!("连接检查失败: {}", e);
        //     return Err(e);
        // }

        let mut attempts = 0;

        while attempts < RETRY_ATTEMPTS {
            match self.try_report_state().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    attempts += 1;
                    if attempts == RETRY_ATTEMPTS {
                        return Err(anyhow::anyhow!(
                            "状态上报失败，已重试 {} 次: {}",
                            RETRY_ATTEMPTS,
                            e
                        )
                        .into());
                    }
                    println!(
                        "状态上报失败，正在重试 ({}/{}): {}",
                        attempts, RETRY_ATTEMPTS, e
                    );
                    time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                }
            }
        }

        Ok(())
    }

    /// 尝试上报单次状态
    async fn try_report_state(&mut self) -> anyhow::Result<()> {
        self.refresh_system_components();

        let (tx, rx) = mpsc::channel(128);
        let request = self.create_state_request().await;

        tx.send(request)
            .await
            .map_err(|e| anyhow::anyhow!("发送状态请求失败: {}", e))?;
        let start = tokio::time::Instant::now();
        let _ = self.client.report_server_state(ReceiverStream::new(rx)).await;
        println!("rpc client 上报耗时: {:?}", start.elapsed());
        // let _response = time::timeout(
        //     Duration::from_secs(GRPC_TIMEOUT_SECS),
        //     self.client.report_server_state(ReceiverStream::new(rx)),
        // )
        // .await;

        // if !response.get_ref().success {
        //     return Err(anyhow::anyhow!("服务器返回状态上报失败"));
        // }

        Ok(())
    }

    /// 创建命令请求
    fn create_command_request(&mut self) -> CommandRequest {
        CommandRequest {
            agent_info: Some(AgentInfo {
                agent_version: VERSION.to_string(),
                server_id: self.server_id,
            }),
        }
    }

    /// 创建状态请求
    async fn create_state_request(&self) -> StateRequest {
        StateRequest {
            agent_info: Some(AgentInfo {
                agent_version: VERSION.to_string(),
                server_id: self.server_id,
            }),
            state: Some(self.get_server_state()),
            upload_time: self.get_upload_time(),
        }
    }

    /// 创建更新IP请求
    async fn create_update_ip_request(&self) -> UpdateIpRequest {
        let geo_ip = fetch_geo_ip().await;
        UpdateIpRequest {
            ipv4: geo_ip.ipv4,
            ipv6: geo_ip.ipv6,
            agent_info: Some(AgentInfo {
                agent_version: VERSION.to_string(),
                server_id: self.server_id,
            }),
            upload_time: self.get_upload_time(),
        }
    }

    /// 创建Host请求
    async fn create_host_request(&self) -> HostRequest {
        HostRequest {
            host: Some(self.get_server_host().await),
            agent_info: Some(AgentInfo {
                agent_version: VERSION.to_string(),
                server_id: self.server_id,
            }),
            upload_time: self.get_upload_time(),
        }
    }

    /// 刷新系统组件信息
    fn refresh_system_components(&mut self) {
        self.system_info.refresh();
    }

    /// 获取服务器信息
    async fn get_server_host(&self) -> Host {
        self.system_info.get_host_info().await
    }

    /// 获取服务器状态
    fn get_server_state(&self) -> State {
        self.system_info.get_system_state()
    }

    /// 获取当前时间戳（秒）
    fn get_upload_time(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs())
    }

    /// 优雅关闭探针
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        if self.report_state {
            self.report_state = false;
            println!("正在停止状态上报...");

            // 发送最后一次状态报告
            self.report_server_state().await?;
        }
        println!("探针已关闭");
        Ok(())
    }
}
