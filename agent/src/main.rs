use clap::Parser;
use command::Command;
use server_monitor_info_upload::ServerMonitorAgent;

mod command;
mod dto;
mod fetch_ip;
mod server_monitor_info_upload;
mod utils;

#[tokio::main]
async fn main() {
    let command = Command::parse();
    // TODO: 待进一步完善
    ServerMonitorAgent::new(command.url, command.port, 1)
        .await
        .report_server_monitor()
        .await;
}
