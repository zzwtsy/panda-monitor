use std::{net::IpAddr, str::FromStr, time::Duration};

use reqwest::{Client, ClientBuilder};
use serde::de::DeserializeOwned;

pub struct HttpUtil {
    client: Client,
    ipv4_client: Client,
    ipv6_client: Client,
}

impl HttpUtil {
    pub fn new() -> Self {
        let ipv4_addr = IpAddr::from_str("0.0.0.0").unwrap();
        let ipv6_addr = IpAddr::from_str("::").unwrap();
        let http_util = HttpUtil {
            client: ClientBuilder::new()
                .connect_timeout(Duration::from_millis(500))
                .build()
                .expect("Init Http Client Failed"),
            ipv4_client: ClientBuilder::new()
                .connect_timeout(Duration::from_millis(500))
                .local_address(ipv4_addr)
                .build()
                .expect("Init only send ipv4 Http Client Failed"),
            ipv6_client: ClientBuilder::new()
                .connect_timeout(Duration::from_millis(500))
                .local_address(ipv6_addr)
                .build()
                .expect("Init only send ipv6 Http Client Failed"),
        };
        http_util
    }
    /// 发送 get 请求
    pub async fn send_get<T>(&self, url: &str) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
    {
        let resp = self.client.get(url).send().await?;
        let json = resp.json::<T>().await?;
        Ok(json)
    }

    /// 发送 get 请求，仅请求 ipv4 地址
    pub async fn send_get_on_ipv4<T>(&self, url: &str) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
    {
        let resp = self.ipv4_client.get(url).send().await?;
        let json = resp.json::<T>().await?;
        Ok(json)
    }

    /// 发送 get 请求，仅请求 ipv6 地址
    pub async fn send_get_on_ipv6<T>(&self, url: &str) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
    {
        let resp = self.ipv6_client.get(url).send().await?;
        let json = resp.json::<T>().await?;
        Ok(json)
    }
}