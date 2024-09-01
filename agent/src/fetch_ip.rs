use tokio::join;

use crate::{
    dto::{IPAPI, IPIP, IPSB},
    utils::http_util::HttpUtil,
};

static IP_SB_V6_URL: &'static str = "https://api-ipv6.ip.sb/geoip";
static IP_SB_V4_URL: &'static str = "https://api-ipv4.ip.sb/geoip";
static IPIP_URL: &'static str = "https://api.myip.la/en?json";
static IPAPI_URL: &'static str = "https://ipapi.co/json";

#[derive(Default)]
pub struct GeoIp {
    pub country_code: String,
    pub ipv4: String,
    pub ipv6: String,
}

pub async fn fetch_geo_ip() -> GeoIp {
    let http_util = HttpUtil::new();
    let (ip_sb, ipip, ipapi) = join!(
        fetch_ip_sb(&http_util),
        fetch_ipip(&http_util),
        fetch_ipapi(&http_util)
    );
    if ipip.is_ok() {
        return ipip.unwrap();
    }
    if ipapi.is_ok() {
        return ipapi.unwrap();
    }
    if ip_sb.is_ok() {
        return ip_sb.unwrap();
    }
    return GeoIp::default();
}

async fn fetch_ip_sb(http_util: &HttpUtil) -> anyhow::Result<GeoIp> {
    let ipv4 = http_util
        .send_get::<IPSB>(&IP_SB_V4_URL)
        .await
        .unwrap_or_else(|_| IPSB::default());

    let ipv6 = http_util
        .send_get::<IPSB>(&IP_SB_V6_URL)
        .await
        .unwrap_or_else(|_| IPSB::default());

    if ipv4.ip == "" && ipv6.ip == "" {
        eprintln!("ip.sb failed to obtain the local ip address");
        return Err(anyhow::anyhow!(
            "ip.sb failed to obtain the local ip address"
        ));
    }

    Ok(GeoIp {
        country_code: if ipv4.country_code == "" {
            ipv6.country_code
        } else {
            ipv4.country_code
        },
        ipv4: ipv4.ip,
        ipv6: ipv6.ip,
    })
}

async fn fetch_ipip(http_util: &HttpUtil) -> anyhow::Result<GeoIp> {
    let ipv4 = http_util.send_get_on_ipv4::<IPIP>(&IPIP_URL);

    let ipv6 = http_util.send_get_on_ipv6::<IPIP>(&IPIP_URL);

    let (ipv4, ipv6) = join!(ipv4, ipv6);

    let ipv4 = ipv4.unwrap_or_else(|err| {
        eprintln!("ipip.net failed to obtain the local ipv4 address: {}", err);
        IPIP::default()
    });

    let ipv6 = ipv6.unwrap_or_else(|err| {
        eprintln!("ipip.net failed to obtain the local ipv6 address: {}", err);
        IPIP::default()
    });

    if ipv4.ip == "" && ipv6.ip == "" {
        eprintln!("ipip.net failed to obtain the local ip address");
        return Err(anyhow::anyhow!(
            "ipip.net failed to obtain the local ip address"
        ));
    }

    Ok(GeoIp {
        country_code: if ipv4.location.country_code == "" {
            ipv6.location.country_code
        } else {
            ipv4.location.country_code
        },
        ipv4: ipv4.ip,
        ipv6: ipv6.ip,
    })
}

async fn fetch_ipapi(http_util: &HttpUtil) -> anyhow::Result<GeoIp> {
    let ipv4 = http_util
        .send_get_on_ipv4::<IPAPI>(&IPAPI_URL)
        .await
        .unwrap_or_else(|_| IPAPI::default());

    let ipv6 = http_util
        .send_get_on_ipv6::<IPAPI>(&IPAPI_URL)
        .await
        .unwrap_or_else(|_| IPAPI::default());
    if ipv4.ip == "" && ipv6.ip == "" {
        eprintln!("ipapi.co failed to obtain the local ip address");
        return Err(anyhow::anyhow!(
            "ipapi.co failed to obtain the local ip address"
        ));
    }
    return Ok(GeoIp {
        country_code: if ipv4.country_code == "" {
            ipv6.country_code
        } else {
            ipv4.country_code
        },
        ipv4: ipv4.ip,
        ipv6: ipv6.ip,
    });
}
