use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Command {
    /// 服务器信息上报的目标地址 (URL)
    /// 指定服务器的 URL 地址，用于将数据上报到该地址。
    #[arg(short, long)]
    pub url: String,
    /// 服务器信息上报的目标端口
    /// 指定服务器的端口号，用于将数据上报到该端口。
    #[arg(short, long)]
    pub port: String,
    /// 加密上报数据的密钥
    /// 用于加密在上报过程中发送到服务器的数据，以确保数据的安全性。
    #[arg(short, long)]
    pub key: String,
    /// 主机信息上报的时间间隔（秒）
    /// 指定主机信息的上报间隔时间，单位为秒。默认为 0，表示仅在启动时上报一次。
    /// 如果需要周期性上报，可以设置为大于 0 的值。
    #[arg(short = 'o', long, default_value_t = 0)]
    pub host_report_interval: u64,
    /// 主机状态信息上报的时间间隔（秒）
    /// 指定主机状态信息的上报间隔时间，单位为秒。默认为 1 秒，表示每秒循环上报一次。
    #[arg(short, long, default_value_t = 1)]
    pub state_report_interval: u64,
}