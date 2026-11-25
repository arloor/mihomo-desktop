#![allow(dead_code)]

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[derive(Debug, Clone)]
pub struct MihomoClient {
    base_url: String,
    ws_base_url: String,
    secret: String,
    http: reqwest::Client,
}

impl MihomoClient {
    pub fn new(controller_addr: &str, secret: &str) -> Self {
        let clean = controller_addr
            .trim()
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/');
        let addr = if clean.is_empty() {
            "127.0.0.1:9090"
        } else {
            clean
        };

        let mut headers = HeaderMap::new();
        if !secret.is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {secret}")) {
                headers.insert(AUTHORIZATION, val);
            }
        }

        // Critical: `no_proxy()` so local Mihomo controller calls never get routed through the system proxy.
        let http = reqwest::Client::builder()
            .no_proxy()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url: format!("http://{addr}"),
            ws_base_url: format!("ws://{addr}"),
            secret: secret.to_string(),
            http,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn ws_url(&self, path: &str, query: Option<&str>) -> String {
        let mut params = Vec::new();
        if let Some(q) = query {
            if !q.is_empty() {
                params.push(q.to_string());
            }
        }
        if !self.secret.is_empty() {
            params.push(format!("token={}", urlencoding::encode(&self.secret)));
        }
        if params.is_empty() {
            format!("{}{}", self.ws_base_url, path)
        } else {
            format!("{}{}?{}", self.ws_base_url, path, params.join("&"))
        }
    }

    async fn check_response(resp: reqwest::Response) -> Result<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Mihomo API 错误 ({status}): {text}")
        }
    }

    pub async fn get_version(&self) -> Result<VersionInfo> {
        let resp = Self::check_response(self.http.get(self.url("/version")).send().await?).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_configs(&self) -> Result<MihomoConfig> {
        let resp = Self::check_response(self.http.get(self.url("/configs")).send().await?).await?;
        Ok(resp.json().await?)
    }

    pub async fn patch_configs(&self, patch: Value) -> Result<()> {
        let resp = self
            .http
            .patch(self.url("/configs"))
            .json(&patch)
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn reload_configs(&self, config_path: &str, force: bool) -> Result<()> {
        let path = if force {
            "/configs?force=true"
        } else {
            "/configs"
        };
        let resp = self
            .http
            .put(self.url(path))
            .json(&json!({ "path": config_path }))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn update_geo(&self) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/configs/geo"))
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn restart_core(&self) -> Result<()> {
        let resp = self.http.post(self.url("/restart")).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn upgrade_core(&self) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/upgrade"))
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn upgrade_ui(&self) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/upgrade/ui"))
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn flush_fakeip_cache(&self) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/cache/fakeip/flush"))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn flush_dns_cache(&self) -> Result<()> {
        let resp = self.http.post(self.url("/cache/dns/flush")).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn query_dns(&self, name: &str, record_type: &str) -> Result<DnsQueryResponse> {
        let url = format!(
            "{}/dns/query?name={}&type={}",
            self.base_url,
            urlencoding::encode(name.trim()),
            urlencoding::encode(record_type.trim())
        );
        let resp = Self::check_response(self.http.get(url).send().await?).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_proxies(&self) -> Result<ProxiesResponse> {
        let resp = Self::check_response(self.http.get(self.url("/proxies")).send().await?).await?;
        Ok(resp.json().await?)
    }

    pub async fn select_proxy(&self, group: &str, proxy_name: &str) -> Result<()> {
        let path = format!("/proxies/{}", urlencoding::encode(group));
        let resp = self
            .http
            .put(self.url(&path))
            .json(&json!({ "name": proxy_name }))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn unfix_proxy(&self, group: &str) -> Result<()> {
        let path = format!("/proxies/{}", urlencoding::encode(group));
        let resp = self.http.delete(self.url(&path)).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn test_proxy_delay(
        &self,
        proxy_name: &str,
        test_url: &str,
        timeout_ms: u32,
    ) -> Result<u32> {
        let path = format!(
            "/proxies/{}/delay?url={}&timeout={}",
            urlencoding::encode(proxy_name),
            urlencoding::encode(test_url),
            timeout_ms
        );
        let resp = Self::check_response(
            self.http
                .get(self.url(&path))
                .timeout(Duration::from_millis(timeout_ms as u64 + 3000))
                .send()
                .await?,
        )
        .await?;
        let body: DelayResponse = resp.json().await?;
        Ok(body.delay)
    }

    pub async fn test_group_delay(
        &self,
        group: &str,
        test_url: &str,
        timeout_ms: u32,
    ) -> Result<HashMap<String, u32>> {
        let path = format!(
            "/group/{}/delay?url={}&timeout={}",
            urlencoding::encode(group),
            urlencoding::encode(test_url),
            timeout_ms
        );
        let resp = Self::check_response(
            self.http
                .get(self.url(&path))
                .timeout(Duration::from_millis(timeout_ms as u64 + 8000))
                .send()
                .await?,
        )
        .await?;
        Ok(resp.json().await?)
    }

    pub async fn get_proxy_providers(&self) -> Result<ProxyProvidersResponse> {
        let resp =
            Self::check_response(self.http.get(self.url("/providers/proxies")).send().await?)
                .await?;
        Ok(resp.json().await?)
    }

    pub async fn update_proxy_provider(&self, name: &str) -> Result<()> {
        let path = format!("/providers/proxies/{}", urlencoding::encode(name));
        let resp = self
            .http
            .put(self.url(&path))
            .timeout(Duration::from_secs(45))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn healthcheck_proxy_provider(&self, name: &str) -> Result<()> {
        let path = format!("/providers/proxies/{}/healthcheck", urlencoding::encode(name));
        let resp = self
            .http
            .get(self.url(&path))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn get_rule_providers(&self) -> Result<RuleProvidersResponse> {
        let resp =
            Self::check_response(self.http.get(self.url("/providers/rules")).send().await?)
                .await?;
        Ok(resp.json().await?)
    }

    pub async fn update_rule_provider(&self, name: &str) -> Result<()> {
        let path = format!("/providers/rules/{}", urlencoding::encode(name));
        let resp = self
            .http
            .put(self.url(&path))
            .timeout(Duration::from_secs(45))
            .send()
            .await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn get_rules(&self) -> Result<RulesResponse> {
        let resp = Self::check_response(self.http.get(self.url("/rules")).send().await?).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_connections(&self) -> Result<ConnectionsResponse> {
        let resp =
            Self::check_response(self.http.get(self.url("/connections")).send().await?).await?;
        Ok(resp.json().await?)
    }

    pub async fn close_connection(&self, id: &str) -> Result<()> {
        let path = format!("/connections/{}", urlencoding::encode(id));
        let resp = self.http.delete(self.url(&path)).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn close_all_connections(&self) -> Result<()> {
        let resp = self.http.delete(self.url("/connections")).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn stream_traffic<F>(&self, mut on_point: F) -> Result<()>
    where
        F: FnMut(TrafficPoint) -> bool,
    {
        let req = self.ws_url("/traffic", None).into_client_request()?;
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .context("连接 /traffic WebSocket 失败")?;
        while let Some(msg) = ws.next().await {
            let msg = msg?;
            if msg.is_text() {
                if let Ok(point) = serde_json::from_str::<TrafficPoint>(&msg.to_string()) {
                    if !on_point(point) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn stream_memory<F>(&self, mut on_mem: F) -> Result<()>
    where
        F: FnMut(MemoryInfo) -> bool,
    {
        let req = self.ws_url("/memory", None).into_client_request()?;
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .context("连接 /memory WebSocket 失败")?;
        while let Some(msg) = ws.next().await {
            let msg = msg?;
            if msg.is_text() {
                if let Ok(info) = serde_json::from_str::<MemoryInfo>(&msg.to_string()) {
                    if !on_mem(info) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn stream_logs<F>(&self, level: &str, mut on_log: F) -> Result<()>
    where
        F: FnMut(LogEntry) -> bool,
    {
        let query = format!("level={}", urlencoding::encode(level));
        let req = self.ws_url("/logs", Some(&query)).into_client_request()?;
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .context("连接 /logs WebSocket 失败")?;
        while let Some(msg) = ws.next().await {
            let msg = msg?;
            if msg.is_text() {
                if let Ok(entry) = serde_json::from_str::<LogEntry>(&msg.to_string()) {
                    if !on_log(entry) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub meta: bool,
    #[serde(default)]
    pub premium: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MihomoConfig {
    #[serde(default)]
    pub port: u16,
    #[serde(rename = "socks-port", default)]
    pub socks_port: u16,
    #[serde(rename = "mixed-port", default)]
    pub mixed_port: u16,
    #[serde(rename = "allow-lan", default)]
    pub allow_lan: bool,
    #[serde(rename = "bind-address", default)]
    pub bind_address: String,
    #[serde(default)]
    pub mode: String,
    #[serde(rename = "log-level", default)]
    pub log_level: String,
    #[serde(default)]
    pub ipv6: bool,
    #[serde(rename = "tcp-concurrent", default)]
    pub tcp_concurrent: bool,
    #[serde(rename = "unified-delay", default)]
    pub unified_delay: bool,
    #[serde(rename = "find-process-mode", default)]
    pub find_process_mode: String,
    #[serde(default)]
    pub sniffing: bool,
    #[serde(default)]
    pub tun: Option<TunConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub stack: String,
    #[serde(default)]
    pub mtu: u32,
    #[serde(rename = "auto-route", default)]
    pub auto_route: bool,
    #[serde(rename = "strict-route", default)]
    pub strict_route: bool,
    #[serde(rename = "auto-detect-interface", default)]
    pub auto_detect_interface: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DelayResponse {
    #[serde(default)]
    pub delay: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProxiesResponse {
    #[serde(default)]
    pub proxies: HashMap<String, ProxyItem>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProxyItem {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub proxy_type: String,
    #[serde(default)]
    pub now: Option<String>,
    #[serde(default)]
    pub all: Option<Vec<String>>,
    #[serde(default)]
    pub history: Vec<ProxyHistory>,
    #[serde(default)]
    pub udp: bool,
    #[serde(default)]
    pub alive: Option<bool>,
    #[serde(default)]
    pub hidden: bool,
}

impl ProxyItem {
    pub fn latest_delay(&self) -> Option<u32> {
        self.history.last().map(|h| h.delay)
    }

    pub fn is_group(&self) -> bool {
        self.all.is_some()
            && matches!(
                self.proxy_type.as_str(),
                "Selector" | "URLTest" | "Fallback" | "LoadBalance" | "Relay"
            )
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProxyHistory {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub delay: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProxyProvidersResponse {
    #[serde(default)]
    pub providers: HashMap<String, ProxyProvider>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProxyProvider {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub provider_type: String,
    #[serde(rename = "vehicleType", default)]
    pub vehicle_type: String,
    #[serde(default)]
    pub proxies: Vec<ProxyItem>,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    #[serde(rename = "subscriptionInfo", default)]
    pub subscription_info: Option<ProviderSubscriptionInfo>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProviderSubscriptionInfo {
    #[serde(rename = "Upload", default)]
    pub upload: u64,
    #[serde(rename = "Download", default)]
    pub download: u64,
    #[serde(rename = "Total", default)]
    pub total: u64,
    #[serde(rename = "Expire", default)]
    pub expire: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuleProvidersResponse {
    #[serde(default)]
    pub providers: HashMap<String, RuleProvider>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuleProvider {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub provider_type: String,
    #[serde(rename = "vehicleType", default)]
    pub vehicle_type: String,
    #[serde(default)]
    pub behavior: String,
    #[serde(default)]
    pub format: String,
    #[serde(rename = "ruleCount", default)]
    pub rule_count: u64,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RulesResponse {
    #[serde(default)]
    pub rules: Vec<RuleItem>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuleItem {
    #[serde(rename = "type", default)]
    pub rule_type: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub size: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConnectionsResponse {
    #[serde(rename = "downloadTotal", default)]
    pub download_total: u64,
    #[serde(rename = "uploadTotal", default)]
    pub upload_total: u64,
    #[serde(default)]
    pub memory: Option<u64>,
    #[serde(default)]
    pub connections: Option<Vec<ConnectionItem>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConnectionItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub metadata: ConnectionMetadata,
    #[serde(default)]
    pub upload: u64,
    #[serde(default)]
    pub download: u64,
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub chains: Vec<String>,
    #[serde(default)]
    pub rule: String,
    #[serde(rename = "rulePayload", default)]
    pub rule_payload: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConnectionMetadata {
    #[serde(default)]
    pub network: String,
    #[serde(rename = "type", default)]
    pub conn_type: String,
    #[serde(rename = "sourceIP", default)]
    pub source_ip: String,
    #[serde(rename = "sourcePort", default)]
    pub source_port: String,
    #[serde(rename = "destinationIP", default)]
    pub destination_ip: String,
    #[serde(rename = "destinationPort", default)]
    pub destination_port: String,
    #[serde(default)]
    pub host: String,
    #[serde(rename = "sniffHost", default)]
    pub sniff_host: String,
    #[serde(default)]
    pub process: String,
    #[serde(rename = "processPath", default)]
    pub process_path: String,
}

impl ConnectionMetadata {
    pub fn display_host(&self) -> String {
        if !self.host.is_empty() {
            format!("{}:{}", self.host, self.destination_port)
        } else if !self.sniff_host.is_empty() {
            format!("{}:{}", self.sniff_host, self.destination_port)
        } else {
            format!("{}:{}", self.destination_ip, self.destination_port)
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct TrafficPoint {
    #[serde(default)]
    pub up: u64,
    #[serde(default)]
    pub down: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct MemoryInfo {
    #[serde(default)]
    pub inuse: u64,
    #[serde(default)]
    pub oslimit: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LogEntry {
    #[serde(rename = "type", default)]
    pub level: String,
    #[serde(default)]
    pub payload: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DnsQueryResponse {
    #[serde(rename = "Status", default)]
    pub status: i32,
    #[serde(rename = "Answer", default)]
    pub answer: Option<Vec<DnsAnswer>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DnsAnswer {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub record_type: u32,
    #[serde(rename = "TTL", default)]
    pub ttl: u32,
    #[serde(default)]
    pub data: String,
}
