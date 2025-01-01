use common::panda_monitor::{Host, State};
use sysinfo::{CpuRefreshKind, Disks, Networks, RefreshKind, System};
use std::{collections::HashSet, ops::Not};

use crate::fetch_ip::fetch_geo_ip;

/// 系统信息收集器
#[derive(Debug)]
pub struct SystemInfoCollector {
    sys: System,
    disks: Disks,
    networks: Networks,
}

impl SystemInfoCollector {
    /// 创建新的系统信息收集器
    pub fn new() -> Self {
        Self {
            sys: System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::everything())),
            disks: Disks::new(),
            networks: Networks::new_with_refreshed_list(),
        }
    }

    /// 刷新系统组件信息
    pub fn refresh(&mut self) {
        self.disks.refresh_list();
        self.sys.refresh_memory();
        self.sys.refresh_cpu_usage();
    }

    /// 获取服务器主机信息
    pub async fn get_host_info(&self) -> Host {
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
        Host {
            os_name: System::name().unwrap_or_default().trim().to_string(),
            distribution_id: System::distribution_id(),
            os_version: System::os_version().unwrap_or_default(),
            cpu,
            cpu_cores: self.sys.cpus().len() as u64,
            kernel_version: System::kernel_version().unwrap_or_default(),
            mem_total: self.sys.total_memory(),
            disk_total,
            swap_total: self.sys.total_swap(),
            arch: System::cpu_arch().unwrap_or_default(),
            boot_time: System::boot_time(),
            ipv4: geo_ip.ipv4,
            ipv6: geo_ip.ipv6,
        }
    }

    /// 获取服务器状态信息
    pub fn get_system_state(&self) -> State {
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

        State {
            cpu_usage: self.sys.global_cpu_usage() as f64,
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