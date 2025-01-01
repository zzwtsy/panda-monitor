use clap::Parser;
use command::Command;
use monitor::ServerMonitorAgent;

mod command;
mod dto;
mod fetch_ip;
mod monitor;
mod utils;
mod system_info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = Command::parse();
    command.validate()?;
    
    match ServerMonitorAgent::new(command).await {
        Ok(mut agent) => {
            match agent.send_command().await {
                Ok(_) => println!("命令执行成功"),
                Err(e) => eprintln!("命令执行失败: {}", e)
            }
        },
        Err(e) => eprintln!("创建代理实例失败: {}", e)
    }
    
    Ok(())
}
