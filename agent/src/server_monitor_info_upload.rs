use std::{
    collections::HashSet,
    ops::Not,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{command::Command, fetch_ip::fetch_geo_ip};
use sysinfo::{CpuRefreshKind, Disks, Networks, RefreshKind, System};
use tokio::time;
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig},
    Request,
};

use common::panda_monitor::{
    panda_monitor_client::PandaMonitorClient, ServerHost, ServerHostRequest, ServerState,
    ServerStateRequest, UpdateIpRequest,
};

const VERSION: &'static str = include_str!(concat!(env!("OUT_DIR"), "/VERSION"));

pub struct ServerMonitorAgent {
    client: PandaMonitorClient<Channel>,
    server_id: u64,
    sys: System,
    disks: Disks,
    networks: Networks,
    command: Command,
}

impl ServerMonitorAgent {
    pub async fn new(command: Command) -> Self {
        let url = format!("https://{}:{}", command.url, command.port);

        let cert = std::fs::read_to_string(command.ssl_cert_path.clone())
            .expect(format!("read ssl cert failed: {}", command.ssl_cert_path).as_str());

        let tls = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(&cert))
            .domain_name(command.url.clone());

        let channel = Channel::from_shared(url)
            .expect("create grpc channel failed: Incalid url")
            .tls_config(tls)
            .expect("create grpc channel failed: Invalid tls config")
            .timeout(Duration::from_secs(5))
            .concurrency_limit(256)
            .connect()
            .await
            .expect("grpc server connect failed");

        let client = PandaMonitorClient::new(channel);

        let disks = Disks::new();
        let sys =
            System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::everything()));
        let networks = Networks::new_with_refreshed_list();
        Self {
            client,
            server_id: command.state_report_interval,
            sys,
            disks,
            networks,
            command,
        }
    }

    /// 启动上报服务器信息
    pub async fn report_server_monitor(&mut self) {
        // 仅在启动时上报一次
        self.report_server_host().await;
        // 上传一次 ip 信息
        if self.command.host_report_interval == 0 {
            self.update_ip().await;
        }
        // 循环上报
        loop {
            self.report_server_state().await;
            // 上传 ip 信息
            let ip_report_interval = self.command.ip_report_interval;
            if ip_report_interval != 0 && self.get_upload_time() % (ip_report_interval * 3600) == 0
            {
                self.update_ip().await;
            }
            // 等待上报间隔
            time::sleep(time::Duration::from_secs(
                self.command.state_report_interval,
            ))
            .await
        }
    }

    /// 上报服务器信息
    async fn report_server_host(&mut self) {
        self.refresh_system_components();
        let server_host = self.get_server_host().await;
        let request = Request::new(ServerHostRequest {
            server_host: Some(server_host),
            upload_time: self.get_upload_time(),
            agent_version: VERSION.to_string(),
            server_id: self.server_id,
        });
        match self.client.report_server_host(request).await {
            Ok(msg) => {
                if msg.get_ref().success.not() {
                    eprintln!("report_server_host failed")
                }
            }
            Err(error) => {
                eprintln!("report_server_host failed: {}", error.message())
            }
        };
    }

    /// 上报服务器状态
    async fn report_server_state(&mut self) {
        self.refresh_system_components();
        self.networks.refresh_list();

        let server_state = self.get_server_state();

        let request: Request<ServerStateRequest> = Request::new(ServerStateRequest {
            server_state: Some(server_state),
            upload_time: self.get_upload_time(),
            agent_version: VERSION.to_string(),
            server_id: self.server_id,
        });
        match self.client.report_server_state(request).await {
            Ok(msg) => {
                if msg.get_ref().success.not() {
                    eprintln!("report_server_state failed")
                }
            }
            Err(error) => {
                eprintln!("report_server_state failed: {}", error.message())
            }
        };
    }

    /// 更新 ip 信息
    async fn update_ip(&mut self) {
        let geo_ip = fetch_geo_ip().await;
        if geo_ip.ipv4.is_empty() && geo_ip.ipv6.is_empty() {
            return;
        }
        let request = UpdateIpRequest {
            ipv4: geo_ip.ipv4,
            ipv6: geo_ip.ipv6,
            country_code: geo_ip.country_code,
            server_id: self.server_id,
        };
        match self.client.update_ip(request).await {
            Ok(msg) => {
                if msg.get_ref().success.not() {
                    eprintln!("update_ip failed")
                }
            }
            Err(error) => {
                eprintln!("update_ip failed: {}", error.message())
            }
        }
    }

    /// 刷新系统组件
    fn refresh_system_components(&mut self) {
        self.disks.refresh_list();
        self.sys.refresh_memory();
        self.sys.refresh_cpu_usage();
    }

    /// 获取当前时间戳（秒）
    fn get_upload_time(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_secs())
    }

    /// 获取服务器信息
    async fn get_server_host(&self) -> ServerHost {
        let disk_total = self
            .disks
            .list()
            .iter()
            .filter(|disk| disk.file_system().eq_ignore_ascii_case("overlay").not())
            .map(|disk| disk.total_space())
            .sum::<u64>();
        let cpu = self
            .sys
            .cpus()
            .iter()
            .map(|cpu| cpu.brand())
            .collect::<HashSet<&str>>()
            .into_iter()
            .map(|cpu_brand| cpu_brand.to_string())
            .collect::<Vec<String>>();
        let geo_ip = fetch_geo_ip().await;
        ServerHost {
            os_name: System::name().unwrap_or_default().trim().to_string(),
            distribution_id: System::distribution_id(),
            os_version: System::os_version().unwrap_or_default(),
            cpu,
            cpu_cores: self.sys.cpus().len() as u32,
            kernel_version: System::kernel_version().unwrap_or_default(),
            mem_total: self.sys.total_memory(),
            disk_total,
            swap_total: self.sys.total_swap(),
            arch: System::cpu_arch().unwrap_or_default(),
            boot_time: System::boot_time(),
            ipv4: geo_ip.ipv4,
            ipv6: geo_ip.ipv6,
            country_code: geo_ip.country_code,
        }
    }

    /// 获取服务器状态
    fn get_server_state(&self) -> ServerState {
        let disk_used = self
            .disks
            .list()
            .iter()
            .filter(|disk| disk.file_system().eq_ignore_ascii_case("overlay").not())
            .map(|disk| disk.total_space() - disk.available_space())
            .sum::<u64>();
        let net_in_transfer = self
            .networks
            .list()
            .iter()
            .map(|(_, net)| net.total_received())
            .sum::<u64>();
        let net_out_transfer = self
            .networks
            .list()
            .iter()
            .map(|(_, net)| net.total_transmitted())
            .sum::<u64>();
        let net_in_speed = self
            .networks
            .list()
            .iter()
            .map(|(_, net)| net.received())
            .sum::<u64>();
        let net_out_speed = self
            .networks
            .list()
            .iter()
            .map(|(_, net)| net.transmitted())
            .sum::<u64>();
        ServerState {
            cpu_usage: self.sys.global_cpu_usage(),
            mem_used: self.sys.used_memory(),
            swap_used: self.sys.used_swap(),
            disk_used,
            net_in_transfer,
            net_out_transfer,
            net_in_speed,
            net_out_speed,
            load1: System::load_average().one,
            load5: System::load_average().five,
            load15: System::load_average().fifteen,
        }
    }
}
