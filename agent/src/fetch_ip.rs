use crate::{
    dto::{IPAPI, IPIP, IPSB},
    utils::http_util::HttpUtil,
};
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use tokio::join;

struct IpServiceConfig {
    v4_url: &'static str,
    v6_url: &'static str,
    fetch_fn: fn(&HttpUtil) -> Pin<Box<dyn Future<Output = Result<GeoIp>> + '_>>,
}

/// 支持的 IP 服务列表
/// 添加新的 IP 获取服务时，需要完成以下步骤：
/// 1. 实现一个新的异步函数（例如 `fetch_new_service`），参考 `fetch_ip_sb` 的实现
/// 2. 在此数组中添加新的配置，例如：
///    IpServiceConfig {
///        v4_url: "新服务的IPv4地址",
///        v6_url: "新服务的IPv6地址",
///        fetch_fn: |http_util| Box::pin(fetch_new_service(http_util)),
///    }
static IP_SERVICES: &[IpServiceConfig] = &[
    IpServiceConfig {
        v4_url: "https://api-ipv4.ip.sb/geoip",
        v6_url: "https://api-ipv6.ip.sb/geoip",
        fetch_fn: |http_util| Box::pin(fetch_ip_sb(http_util)),
    },
    IpServiceConfig {
        v4_url: "https://api.myip.la/en?json",
        v6_url: "https://api.myip.la/en?json",
        fetch_fn: |http_util| Box::pin(fetch_ipip(http_util)),
    },
    IpServiceConfig {
        v4_url: "https://ipapi.co/json",
        v6_url: "https://ipapi.co/json",
        fetch_fn: |http_util| Box::pin(fetch_ipapi(http_util)),
    },
];

#[derive(Default)]
pub struct GeoIp {
    pub ipv4: String,
    pub ipv6: String,
}

/// 获取 IP 地址的主函数
/// 该函数会并发调用所有配置的 IP 获取服务，并返回第一个成功的结果
/// 如果所有服务都失败，则返回默认的 GeoIp 结构体
///
/// 添加新的 IP 获取服务时，无需修改此函数，只需在 `IP_SERVICES` 数组中添加新配置即可
///
/// 返回值：
/// - 成功时返回包含 IPv4 和 IPv6 地址的 GeoIp 结构体
/// - 失败时返回默认的 GeoIp 结构体（空地址）
pub async fn fetch_geo_ip() -> GeoIp {
    let http_util = HttpUtil::new();

    // 创建一个 Future 列表，用于存储所有 IP 服务的获取任务
    let futures: Vec<_> = IP_SERVICES
        .iter()
        .map(|config| fetch_from_service(&http_util, config))
        .collect();

    // 并发执行所有任务
    let results = futures::future::join_all(futures).await;

    // 返回第一个成功的结果
    for result in results {
        if result.is_ok() {
            return result.unwrap();
        }
    }

    GeoIp::default()
}

async fn fetch_from_service(http_util: &HttpUtil, config: &IpServiceConfig) -> Result<GeoIp> {
    (config.fetch_fn)(http_util).await
}

async fn fetch_ip_sb(http_util: &HttpUtil) -> Result<GeoIp> {
    let ipv4 = http_util
        .send_get::<IPSB>(IP_SERVICES[0].v4_url)
        .await
        .unwrap_or_default();

    let ipv6 = http_util
        .send_get::<IPSB>(IP_SERVICES[0].v6_url)
        .await
        .unwrap_or_default();

    if ipv4.ip.is_empty() && ipv6.ip.is_empty() {
        return Err(anyhow::anyhow!(
            "ip.sb failed to obtain the local ip address"
        ));
    }

    Ok(GeoIp {
        ipv4: ipv4.ip,
        ipv6: ipv6.ip,
    })
}

async fn fetch_ipip(http_util: &HttpUtil) -> Result<GeoIp> {
    let ipv4 = http_util.send_get_on_ipv4::<IPIP>(IP_SERVICES[1].v4_url);
    let ipv6 = http_util.send_get_on_ipv6::<IPIP>(IP_SERVICES[1].v6_url);
    let (ipv4, ipv6) = join!(ipv4, ipv6);

    let ipv4 = ipv4.unwrap_or_default();
    let ipv6 = ipv6.unwrap_or_default();

    if ipv4.ip.is_empty() && ipv6.ip.is_empty() {
        return Err(anyhow::anyhow!(
            "ipip.net failed to obtain the local ip address"
        ));
    }

    Ok(GeoIp {
        ipv4: ipv4.ip,
        ipv6: ipv6.ip,
    })
}

async fn fetch_ipapi(http_util: &HttpUtil) -> Result<GeoIp> {
    let ipv4 = http_util
        .send_get_on_ipv4::<IPAPI>(IP_SERVICES[2].v4_url)
        .await
        .unwrap_or_default();

    let ipv6 = http_util
        .send_get_on_ipv6::<IPAPI>(IP_SERVICES[2].v6_url)
        .await
        .unwrap_or_default();

    if ipv4.ip.is_empty() && ipv6.ip.is_empty() {
        return Err(anyhow::anyhow!(
            "ipapi.co failed to obtain the local ip address"
        ));
    }

    Ok(GeoIp {
        ipv4: ipv4.ip,
        ipv6: ipv6.ip,
    })
}
