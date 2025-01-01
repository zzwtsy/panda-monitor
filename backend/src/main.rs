mod rpc_service;
mod ws_handler;

use common::panda_monitor::panda_monitor_server::PandaMonitorServer;
use common::panda_monitor::Command;
use rpc_service::PandaMonitorService;
use salvo::prelude::*;
use tokio::sync::broadcast;
use tonic::transport::Server as TonicServer;
use ws_handler::WsHandler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 创建命令通道
    let (command_tx, _) = broadcast::channel::<Command>(128);

    // 初始化 RPC 服务器
    tracing::info!("Starting RPC server...");
    let rpc_addr = "0.0.0.0:50051".parse()?;
    let rpc_service = PandaMonitorServer::new(PandaMonitorService::new(command_tx.clone()));
    let rpc_server = TonicServer::builder()
        .add_service(rpc_service)
        .serve(rpc_addr);

    // 创建路由
    let router = Router::new()
        .push(Router::with_path("/ws").goal(WsHandler::new(command_tx)));
    tracing::info!("Starting HTTP server...");
    let acceptor = TcpListener::new("0.0.0.0:8000").bind().await;
    // 启动 HTTP 服务器
    let http_server = Server::new(acceptor).serve(router);

    // 并发运行两个服务器
    let _ = tokio::join!(rpc_server, http_server);

    Ok(())
}
