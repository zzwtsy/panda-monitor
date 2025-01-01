use common::panda_monitor::Command;
use salvo::websocket::{Message, WebSocket, WebSocketUpgrade};
use salvo::{async_trait, Depot, FlowCtrl, Handler, Request, Response};
use tokio::sync::broadcast::Sender;

#[derive(Debug)]
pub struct WsHandler {
    command_tx: Sender<Command>,
}

impl WsHandler {
    pub fn new(command_tx: Sender<Command>) -> Self {
        Self { command_tx }
    }
}

#[async_trait]
impl Handler for WsHandler {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        // if let Err(e) = self.verify_token(req).await {
        //     tracing::error!("Token验证失败: {}", e);
        //     res.status_code(StatusCode::UNAUTHORIZED);
        //     return;
        // }

        tracing::info!("WebSocket连接建立");
        let command_tx = self.command_tx.clone();
        WebSocketUpgrade::new()
            .upgrade(req, res, |ws| async move {
                handle_socket(ws, command_tx).await;
            })
            .await
            .unwrap_or_else(|e| {
                tracing::error!("WebSocket升级失败: {}", e);
            })
    }
}

impl WsHandler {
    async fn verify_token(&self, req: &mut Request) -> anyhow::Result<()> {
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| anyhow::anyhow!("缺少授权token"))?;

        // 实现JWT token验证
        let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET未配置");
        let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        let _: jsonwebtoken::TokenData<serde_json::Value> = jsonwebtoken::decode(
            token,
            &jsonwebtoken::DecodingKey::from_secret(secret.as_ref()),
            &validation,
        )
        .map_err(|e| anyhow::anyhow!("Token验证失败: {}", e))?;

        Ok(())
    }
}

async fn handle_socket(mut socket: WebSocket, command_tx: Sender<Command>) {
    while let Some(msg) = socket.recv().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                tracing::error!("{}", err);
                break;
            }
        };
        if msg.is_close() {
            tracing::info!("WebSocket closed connection");
            let _ = socket.close().await;
            let result = command_tx.send(Command {
                command: 0,
                data: "stop_report_state".into(),
                server_ids: vec![1, 2, 3],
            });
            match result {
                Ok(ok) => {
                    tracing::info!("Message sent successfully：{}", ok);
                }
                Err(e) => tracing::error!("Failed to send message: {}", e),
            }
            break;
        }
        let text = match msg.to_str() {
            Ok(text) => text,
            Err(err) => {
                tracing::error!("{}", err);
                break;
            }
        };
        tracing::info!("Received message: {}", text);
        match text {
            "start" => {
                let result = command_tx.send(Command {
                    command: 0,
                    data: "report_state".into(),
                    server_ids: vec![1, 2, 3],
                });
                match result {
                    Ok(ok) => {
                        tracing::info!("Message sent successfully：{}", ok);
                    }
                    Err(e) => tracing::error!("Failed to send message: {}", e),
                }
                let mut rx = command_tx.subscribe();
                    while let Ok(res) = rx.recv().await {
                    // tracing::info!("收到命令: {:?}", res);
                    if let Err(e) = socket.send(Message::text(res.data)).await {
                        tracing::error!("发送消息失败: {}", e);
                    }
                }
            }

            "stop" => {
                let result = command_tx.send(Command {
                    command: 0,
                    data: "stop_report_state".into(),
                    server_ids: vec![1, 2, 3],
                });
                match result {
                    Ok(ok) => {
                        tracing::info!("Message sent successfully：{}", ok);
                    }
                    Err(e) => tracing::error!("Failed to send message: {}", e),
                }
            }

            _ => {}
        }
    }
}
