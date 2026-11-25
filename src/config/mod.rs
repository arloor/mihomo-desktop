use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use yaml_serde::{Mapping, Value};

/// Exact default `profile` sub-fields merged when absent in user profile.
pub const DEFAULT_PROFILE_YAML: &str = r#"store-selected: true
store-fake-ip: true
"#;

/// Exact default `geox-url` sub-fields merged when absent in user profile.
pub const DEFAULT_GEOX_URL_YAML: &str = r#"geoip: https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.dat
geosite: https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geosite.dat
mmdb: https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.metadb
asn: https://github.com/xishang0128/geoip/releases/download/latest/GeoLite2-ASN.mmdb
"#;

/// Exact default `sniffer` block injected when user profile omits `sniffer`.
pub const DEFAULT_SNIFFER_YAML: &str = r#"enable: true
force-dns-mapping: true
parse-pure-ip: true
override-destination: false
sniff:
  HTTP:
    ports: [80, 8080-8880]
    override-destination: true
  TLS:
    ports: [443, 8443]
  QUIC:
    ports: [443, 8443]
skip-domain:
  - "Mijia Cloud"
  - "+.push.apple.com"
  - "+.lan"
  - "+.local"
"#;

/// Exact default DNS configuration required when user profile omits `dns`.
pub const DEFAULT_DNS_YAML: &str = r#"enable: true
default-nameserver:
  - 223.5.5.5
  - 119.29.29.29
  - 223.6.6.6
  - 2400:3200::1
enhanced-mode: fake-ip
fake-ip-filter:
  - "*.lan"
  - "*.local"
  - "*.arpa"
  - ntp.*.com
  - time.*.com
  - +.market.xiaomi.com
  - localhost.ptlogin2.qq.com
  - "*.msftncsi.com"
  - www.msftconnecttest.com
  # 华为手表借网更新天气
  - hw.zuimeitianqi.com
  - weather-drcn.music.hicloud.com
  - hag-drcn.op.dbankcloud.com
  - connectivitycheck.platform.hicloud.com
  - connectivitycheck.cbg-app.huawei.com
fake-ip-filter-mode: blacklist
fake-ip-range: 198.18.0.1/16
respect-rules: true
nameserver-policy:
  "geosite:cn,private,apple":
    - 223.5.5.5
    - 119.29.29.29
direct-nameserver:
  - 223.5.5.5
  - 119.29.29.29
direct-nameserver-follow-policy: true
nameserver:
  - https://8.8.8.8/dns-query
  - https://1.1.1.1/dns-query
ipv6: true
# listen: :53
prefer-h3: false
proxy-server-nameserver:
  - https://1.12.12.12/dns-query
  - https://223.5.5.5/dns-query
  - tls://223.5.5.5
use-hosts: false
use-system-hosts: true
"#;

/// Exact default TUN configuration required when user profile omits `tun`.
pub const DEFAULT_TUN_YAML: &str = r#"auto-detect-interface: true
auto-route: true
device: Mihomo
dns-hijack:
  - any:53
  - tcp://any:53
mtu: 1492
stack: system
strict-route: false
enable: false
inet4-route-exclude-address:
  - 192.168.0.0/16
  - 10.0.0.0/8
  - 172.16.0.0/12
  - 127.0.0.0/8
inet6-route-exclude-address:
  - fc00::/7
  - fe80::/10
"#;

fn indent_yaml_block(block: &str) -> String {
    block
        .lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn default_tun_dns_preview() -> String {
    format!(
        "dns:\n{}\ntun:\n{}",
        indent_yaml_block(DEFAULT_DNS_YAML),
        indent_yaml_block(DEFAULT_TUN_YAML),
    )
}

/// Human-readable preview of all built-in default configuration items merged when omitted by a user profile.
pub fn default_merge_config_preview() -> String {
    format!(
        r#"mixed-port: 7890
allow-lan: false
log-level: info
ipv6: true
mode: rule
unified-delay: true
geo-auto-update: true
geo-update-interval: 24
profile:
{}
geox-url:
{}
sniffer:
{}
{}
rules:
  - MATCH,DIRECT"#,
        indent_yaml_block(DEFAULT_PROFILE_YAML),
        indent_yaml_block(DEFAULT_GEOX_URL_YAML),
        indent_yaml_block(DEFAULT_SNIFFER_YAML),
        default_tun_dns_preview(),
    )
}

/// Minimal runnable `clash.yaml` created on fresh installations when no config exists yet.
pub fn minimal_initial_config_yaml() -> String {
    format!(
        r#"bind-address: "*"
external-controller: 127.0.0.1:9090
secret: ""
{}
proxies: []
proxy-groups:
  - name: PROXY
    type: select
    proxies:
      - DIRECT
"#,
        default_merge_config_preview()
    )
}

pub const DEFAULT_PROXY_BYPASS: &str = "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;<local>";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub service_name: String,
    pub core_binary_path: String,
    pub work_dir: String,
    pub active_config_path: String,
    pub controller_addr: String,
    pub controller_secret: String,
    pub mixed_port: u16,
    pub proxy_bypass: String,
    pub tun_enabled: bool,
    pub resident_mode: bool,
    pub system_proxy_managed: bool,
    pub force_default_tun_dns: bool,
    pub delay_test_url: String,
    pub delay_test_timeout_ms: u32,
    pub dark_mode: bool,
    pub use_proxy_for_github: bool,
    pub include_prerelease: bool,
    pub active_profile_id: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            service_name: "mihomo".to_string(),
            core_binary_path: r"C:\Program Files\Mihomo\mihomo.exe".to_string(),
            work_dir: r"C:\ProgramData\Mihomo".to_string(),
            active_config_path: r"C:\ProgramData\Mihomo\config\clash.yaml".to_string(),
            controller_addr: "127.0.0.1:9090".to_string(),
            controller_secret: String::new(),
            mixed_port: 7890,
            proxy_bypass: DEFAULT_PROXY_BYPASS.to_string(),
            tun_enabled: false,
            resident_mode: false,
            system_proxy_managed: false,
            force_default_tun_dns: false,
            delay_test_url: "http://www.gstatic.com/generate_204".to_string(),
            delay_test_timeout_ms: 5000,
            dark_mode: false,
            use_proxy_for_github: true,
            include_prerelease: false,
            active_profile_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionInfo {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    pub expire: Option<i64>,
}

impl SubscriptionInfo {
    pub fn parse_header(header: &str) -> Option<Self> {
        let mut info = Self::default();
        let mut found = false;
        for part in header.split(';') {
            let mut kv = part.trim().splitn(2, '=');
            let k = kv.next()?.trim().to_ascii_lowercase();
            let v = kv.next()?.trim();
            match k.as_str() {
                "upload" => {
                    if let Ok(n) = v.parse::<u64>() {
                        info.upload = n;
                        found = true;
                    }
                }
                "download" => {
                    if let Ok(n) = v.parse::<u64>() {
                        info.download = n;
                        found = true;
                    }
                }
                "total" => {
                    if let Ok(n) = v.parse::<u64>() {
                        info.total = n;
                        found = true;
                    }
                }
                "expire" => {
                    if let Ok(n) = v.parse::<i64>() {
                        info.expire = Some(n);
                        found = true;
                    }
                }
                _ => {}
            }
        }
        found.then_some(info)
    }

    pub fn used(&self) -> u64 {
        self.upload.saturating_add(self.download)
    }

    pub fn usage_ratio(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used() as f64 / self.total as f64).clamp(0.0, 1.0) as f32
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub url: Option<String>,
    pub file_path: PathBuf,
    pub updated_at: DateTime<Utc>,
    pub subscription_info: Option<SubscriptionInfo>,
    pub node_count: usize,
    pub has_custom_dns: bool,
    pub has_custom_tun: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileStore {
    pub profiles: Vec<Profile>,
}

pub fn app_data_dir() -> PathBuf {
    let base = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("mihomo-desktop")
}

pub fn profiles_dir() -> PathBuf {
    app_data_dir().join("profiles")
}

pub fn cores_dir() -> PathBuf {
    app_data_dir().join("cores")
}

pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

pub fn profiles_index_path() -> PathBuf {
    app_data_dir().join("profiles.json")
}

pub fn ensure_app_dirs() -> Result<()> {
    fs::create_dir_all(app_data_dir())?;
    fs::create_dir_all(profiles_dir())?;
    fs::create_dir_all(cores_dir())?;
    Ok(())
}

pub fn load_settings() -> AppSettings {
    let _ = ensure_app_dirs();
    let path = settings_path();
    let mut settings = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<AppSettings>(&s).ok())
            .unwrap_or_default()
    } else {
        let mut s = AppSettings::default();
        sync_settings_from_yaml(&mut s);
        let _ = save_settings(&s);
        s
    };

    // Always try to sync missing controller info if empty
    if settings.controller_addr.trim().is_empty() {
        settings.controller_addr = "127.0.0.1:9090".to_string();
    }
    settings
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    ensure_app_dirs()?;
    let json = serde_json::to_string_pretty(settings)?;
    fs::write(settings_path(), json)?;
    Ok(())
}

pub fn load_profiles() -> ProfileStore {
    let _ = ensure_app_dirs();
    let path = profiles_index_path();
    if path.exists() {
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(store) = serde_json::from_str::<ProfileStore>(&text) {
                return store;
            }
        }
    }
    ProfileStore::default()
}

pub fn save_profiles(store: &ProfileStore) -> Result<()> {
    ensure_app_dirs()?;
    let json = serde_json::to_string_pretty(store)?;
    fs::write(profiles_index_path(), json)?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct DiscoveredServicePaths {
    pub core_binary_path: Option<String>,
    pub work_dir: Option<String>,
    pub config_path: Option<String>,
}

/// Parse paths from our built-in `--service --core ... --work-dir ... --config ...` SCM binary command line.
pub fn discover_from_service_binary_cmdline(cmdline: &str) -> Option<DiscoveredServicePaths> {
    let tokens = split_windows_args(cmdline);
    if !tokens.iter().any(|t| t == "--service") {
        return None;
    }
    let mut out = DiscoveredServicePaths::default();
    let mut iter = tokens.iter();
    while let Some(tok) = iter.next() {
        match tok.as_str() {
            "--core" => {
                if let Some(v) = iter.next() {
                    out.core_binary_path = Some(v.clone());
                }
            }
            "--work-dir" => {
                if let Some(v) = iter.next() {
                    out.work_dir = Some(v.clone());
                }
            }
            "--config" => {
                if let Some(v) = iter.next() {
                    out.config_path = Some(v.clone());
                }
            }
            _ => {}
        }
    }
    if out.core_binary_path.is_some() || out.work_dir.is_some() || out.config_path.is_some() {
        Some(out)
    } else {
        None
    }
}

fn split_windows_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in s.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

#[derive(Debug, Clone, Default)]
pub struct ExtractedYamlInfo {
    pub controller_addr: Option<String>,
    pub secret: Option<String>,
    pub mixed_port: Option<u16>,
    pub tun_enabled: Option<bool>,
    pub node_count: usize,
    pub has_custom_dns: bool,
    pub has_custom_tun: bool,
}

pub fn inspect_yaml_str(content: &str) -> Result<ExtractedYamlInfo> {
    let value: Value = yaml_serde::from_str(content).context("YAML 解析失败")?;
    let Some(map) = value.as_mapping() else {
        anyhow::bail!("配置文件顶层必须是 YAML Mapping 对象");
    };

    let mut info = ExtractedYamlInfo::default();

    if let Some(ec) = get_str(map, "external-controller") {
        let normalized = normalize_controller_addr(&ec);
        if !normalized.is_empty() {
            info.controller_addr = Some(normalized);
        }
    }
    if let Some(sec) = get_str(map, "secret") {
        info.secret = Some(sec);
    }
    if let Some(p) = get_u16(map, "mixed-port")
        .or_else(|| get_u16(map, "port"))
        .or_else(|| get_u16(map, "socks-port"))
    {
        if p > 0 {
            info.mixed_port = Some(p);
        }
    }
    if let Some(proxies) = map.get(Value::String("proxies".into())).and_then(|v| v.as_sequence()) {
        info.node_count = proxies.len();
    }

    info.has_custom_dns = has_non_empty_mapping(map, "dns");
    info.has_custom_tun = has_non_empty_mapping(map, "tun");

    if let Some(tun_map) = map.get(Value::String("tun".into())).and_then(|v| v.as_mapping()) {
        if let Some(en) = tun_map
            .get(Value::String("enable".into()))
            .and_then(|v| v.as_bool())
        {
            info.tun_enabled = Some(en);
        }
    }

    Ok(info)
}

pub fn sync_settings_from_yaml(settings: &mut AppSettings) {
    let path = Path::new(&settings.active_config_path);
    if !path.exists() {
        return;
    }
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(info) = inspect_yaml_str(&content) {
            if let Some(addr) = info.controller_addr {
                settings.controller_addr = addr;
            }
            if let Some(sec) = info.secret {
                settings.controller_secret = sec;
            }
            if let Some(port) = info.mixed_port {
                settings.mixed_port = port;
            }
            if let Some(tun) = info.tun_enabled {
                settings.tun_enabled = tun;
            }
        }
    }
}

pub fn normalize_controller_addr(raw: &str) -> String {
    let trimmed = raw
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix(':') {
        format!("127.0.0.1:{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("0.0.0.0:") {
        format!("127.0.0.1:{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("[::]:") {
        format!("127.0.0.1:{rest}")
    } else {
        trimmed.to_string()
    }
}

fn get_str(map: &Mapping, key: &str) -> Option<String> {
    map.get(Value::String(key.to_string())).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn get_u16(map: &Mapping, key: &str) -> Option<u16> {
    map.get(Value::String(key.to_string()))
        .and_then(|v| v.as_u64())
        .and_then(|n| u16::try_from(n).ok())
}

fn has_non_empty_mapping(map: &Mapping, key: &str) -> bool {
    map.get(Value::String(key.to_string()))
        .and_then(|v| v.as_mapping())
        .is_some_and(|m| !m.is_empty())
}

fn is_missing_or_null(map: &Mapping, key: &str) -> bool {
    matches!(
        map.get(Value::String(key.to_string())),
        None | Some(Value::Null)
    )
}

fn insert_if_missing(map: &mut Mapping, key: &str, value: Value) {
    if is_missing_or_null(map, key) {
        map.insert(Value::String(key.to_string()), value);
    }
}

fn merge_mapping_defaults(map: &mut Mapping, key: &str, defaults_yaml: &str) -> Result<()> {
    let defaults: Value = yaml_serde::from_str(defaults_yaml)
        .with_context(|| format!("解析内置默认 {key} 配置失败"))?;
    let Some(default_map) = defaults.as_mapping() else {
        return Ok(());
    };

    let key_val = Value::String(key.to_string());
    if !map.get(&key_val).is_some_and(|v| v.is_mapping()) {
        map.insert(key_val.clone(), Value::Mapping(Mapping::new()));
    }

    if let Some(target_map) = map.get_mut(&key_val).and_then(|v| v.as_mapping_mut()) {
        for (k, v) in default_map {
            let missing = match target_map.get(k) {
                None | Some(Value::Null) => true,
                Some(Value::String(s)) => s.trim().is_empty(),
                _ => false,
            };
            if missing {
                target_map.insert(k.clone(), v.clone());
            }
        }
    }
    Ok(())
}

/// Merges the user's profile YAML into a runtime Mihomo configuration:
/// - Injects base defaults (`mixed-port` only when `mixed-port`/`port`/`socks-port` are all absent,
///   `allow-lan`, `log-level`, `ipv6`, `mode`, `unified-delay`, `geo-auto-update`, `geo-update-interval`)
///   when omitted by the user's config.
/// - Sub-field merges `profile` (`store-selected`, `store-fake-ip`) and `geox-url` without overriding user keys.
/// - Injects default `sniffer` block only when `sniffer` is absent/null (respects explicit user `sniffer`).
/// - If `dns` is absent/empty (or `force_default_tun_dns` is true), injects `DEFAULT_DNS_YAML`.
/// - If `tun` is absent/empty (or `force_default_tun_dns` is true), injects `DEFAULT_TUN_YAML`.
/// - Sets `tun.enable` to `tun_enabled`. When `tun_enabled` is true, guarantees `dns.enable = true`.
/// - If `rules` is absent or empty, injects fallback `["MATCH,DIRECT"]`.
/// - Preserves or injects `external-controller` / `secret` so desktop app retains API access.
pub fn merge_runtime_config(
    raw_yaml: &str,
    tun_enabled: bool,
    force_default_tun_dns: bool,
    fallback_controller: &str,
    fallback_secret: &str,
    fallback_mixed_port: u16,
) -> Result<String> {
    let mut root: Value = yaml_serde::from_str(raw_yaml).context("解析用户 YAML 配置失败")?;
    let map = root
        .as_mapping_mut()
        .context("配置文件顶层必须为 YAML 映射")?;

    // Inject mixed-port only when user profile defines none of mixed-port / port / socks-port
    let has_any_proxy_port = get_u16(map, "mixed-port").is_some_and(|p| p > 0)
        || get_u16(map, "port").is_some_and(|p| p > 0)
        || get_u16(map, "socks-port").is_some_and(|p| p > 0);
    if !has_any_proxy_port {
        let port = if fallback_mixed_port > 0 {
            fallback_mixed_port
        } else {
            7890
        };
        map.insert(
            Value::String("mixed-port".to_string()),
            Value::Number(port.into()),
        );
    }

    // Scalar base defaults (never override user-specified values)
    insert_if_missing(map, "allow-lan", Value::Bool(false));
    insert_if_missing(map, "log-level", Value::String("info".to_string()));
    insert_if_missing(map, "ipv6", Value::Bool(true));
    insert_if_missing(map, "mode", Value::String("rule".to_string()));
    insert_if_missing(map, "unified-delay", Value::Bool(true));
    insert_if_missing(map, "geo-auto-update", Value::Bool(true));
    insert_if_missing(map, "geo-update-interval", Value::Number(24u64.into()));

    // Sub-field merge for `profile` and `geox-url`
    merge_mapping_defaults(map, "profile", DEFAULT_PROFILE_YAML)?;
    merge_mapping_defaults(map, "geox-url", DEFAULT_GEOX_URL_YAML)?;

    // Whole-block fallback for `sniffer` (respects explicit user `sniffer`, including `enable: false`)
    if is_missing_or_null(map, "sniffer") {
        let default_sniffer: Value =
            yaml_serde::from_str(DEFAULT_SNIFFER_YAML).context("解析内置默认 sniffer 配置失败")?;
        map.insert(Value::String("sniffer".to_string()), default_sniffer);
    }

    let default_dns: Value = yaml_serde::from_str(DEFAULT_DNS_YAML)
        .context("解析内置默认 DNS 配置失败")?;
    let default_tun: Value = yaml_serde::from_str(DEFAULT_TUN_YAML)
        .context("解析内置默认 TUN 配置失败")?;

    let dns_key = Value::String("dns".to_string());
    let tun_key = Value::String("tun".to_string());

    let missing_dns = !has_non_empty_mapping(map, "dns");
    if force_default_tun_dns || missing_dns {
        map.insert(dns_key.clone(), default_dns);
    }

    let missing_tun = !has_non_empty_mapping(map, "tun");
    if force_default_tun_dns || missing_tun {
        map.insert(tun_key.clone(), default_tun);
    }

    // Set tun.enable to match desired TUN state
    if let Some(tun_map) = map.get_mut(&tun_key).and_then(|v| v.as_mapping_mut()) {
        tun_map.insert(Value::String("enable".to_string()), Value::Bool(tun_enabled));
    }

    // Ensure dns.enable = true when TUN is enabled
    if tun_enabled {
        if let Some(dns_map) = map.get_mut(&dns_key).and_then(|v| v.as_mapping_mut()) {
            dns_map.insert(Value::String("enable".to_string()), Value::Bool(true));
        }
    }

    // Fallback rules only when user profile omits `rules` or has an empty list
    let rules_key = Value::String("rules".to_string());
    let has_rules = map
        .get(&rules_key)
        .and_then(|v| v.as_sequence())
        .is_some_and(|seq| !seq.is_empty());
    if !has_rules {
        map.insert(
            rules_key,
            Value::Sequence(vec![Value::String("MATCH,DIRECT".to_string())]),
        );
    }

    // Ensure external-controller exists so the desktop app never disconnects
    let ec_key = Value::String("external-controller".to_string());
    let has_ec = map
        .get(&ec_key)
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty());
    if !has_ec && !fallback_controller.trim().is_empty() {
        map.insert(ec_key, Value::String(fallback_controller.trim().to_string()));
    }

    let secret_key = Value::String("secret".to_string());
    if !map.contains_key(&secret_key) && !fallback_secret.is_empty() {
        map.insert(secret_key, Value::String(fallback_secret.to_string()));
    }

    let serialized = yaml_serde::to_string(&root).context("序列化合并后的 YAML 失败")?;
    Ok(serialized)
}

/// Import a remote Mihomo/Clash profile from a URL and store its raw YAML locally.
pub async fn fetch_profile_from_url(
    name: Option<String>,
    url: &str,
    proxy_url: Option<&str>,
) -> Result<(Profile, String)> {
    ensure_app_dirs()?;
    let trimmed_url = url.trim();
    if !trimmed_url.starts_with("http://") && !trimmed_url.starts_with("https://") {
        anyhow::bail!("请输入有效的 http:// 或 https:// 订阅地址");
    }

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("clash.meta/mihomo-desktop");

    if let Some(proxy) = proxy_url {
        if let Ok(p) = reqwest::Proxy::all(proxy) {
            builder = builder.proxy(p);
        }
    }
    let client = builder.build()?;

    let resp = match client.get(trimmed_url).send().await {
        Ok(r) => r,
        Err(first_err) if proxy_url.is_some() => {
            // Fallback to direct connection if proxy failed
            let direct = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("clash.meta/mihomo-desktop")
                .no_proxy()
                .build()?;
            direct.get(trimmed_url).send().await.with_context(|| {
                format!("通过代理和直连均无法下载订阅 (代理错误: {first_err})")
            })?
        }
        Err(e) => return Err(e).context("下载订阅配置失败"),
    };

    if !resp.status().is_success() {
        anyhow::bail!("订阅服务器返回错误状态码: {}", resp.status());
    }

    let sub_info = resp
        .headers()
        .get("subscription-userinfo")
        .and_then(|h| h.to_str().ok())
        .and_then(SubscriptionInfo::parse_header);

    let content_disp_name = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|h| h.to_str().ok())
        .and_then(parse_filename_from_disposition);

    let body = resp.text().await.context("读取订阅响应内容失败")?;
    let info = inspect_yaml_str(&body).context("下载的内容不是有效的 Mihomo YAML 配置")?;

    let id = Uuid::new_v4().to_string();
    let file_path = profiles_dir().join(format!("{id}.yaml"));
    fs::write(&file_path, &body).context("保存订阅文件失败")?;

    let resolved_name = name
        .filter(|s| !s.trim().is_empty())
        .or(content_disp_name)
        .unwrap_or_else(|| {
            trimmed_url
                .split('/')
                .rfind(|seg| !seg.is_empty())
                .map(|s| s.split('?').next().unwrap_or(s).to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "远程订阅".to_string())
        });

    let profile = Profile {
        id,
        name: resolved_name,
        url: Some(trimmed_url.to_string()),
        file_path,
        updated_at: Utc::now(),
        subscription_info: sub_info,
        node_count: info.node_count,
        has_custom_dns: info.has_custom_dns,
        has_custom_tun: info.has_custom_tun,
    };

    Ok((profile, body))
}

/// Re-download an existing remote profile in place.
pub async fn refresh_remote_profile(
    profile: &Profile,
    proxy_url: Option<&str>,
) -> Result<Profile> {
    let url = profile
        .url
        .as_deref()
        .context("该配置为本地导入配置，没有远程订阅地址")?;
    let (mut fetched, body) =
        fetch_profile_from_url(Some(profile.name.clone()), url, proxy_url).await?;
    // Remove temp file created by fetch_profile_from_url and overwrite original file_path
    let _ = fs::remove_file(&fetched.file_path);
    fs::write(&profile.file_path, &body).context("写入更新后的订阅文件失败")?;
    fetched.id = profile.id.clone();
    fetched.file_path = profile.file_path.clone();
    Ok(fetched)
}

/// Import a local YAML file into the managed profiles directory.
pub fn import_local_profile(name: Option<String>, source_path: &Path) -> Result<Profile> {
    ensure_app_dirs()?;
    let body = fs::read_to_string(source_path)
        .with_context(|| format!("无法读取本地文件: {}", source_path.display()))?;
    let info = inspect_yaml_str(&body)?;

    let id = Uuid::new_v4().to_string();
    let file_path = profiles_dir().join(format!("{id}.yaml"));
    fs::write(&file_path, &body)?;

    let resolved_name = name
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "本地配置".to_string());

    Ok(Profile {
        id,
        name: resolved_name,
        url: None,
        file_path,
        updated_at: Utc::now(),
        subscription_info: None,
        node_count: info.node_count,
        has_custom_dns: info.has_custom_dns,
        has_custom_tun: info.has_custom_tun,
    })
}

fn parse_filename_from_disposition(header: &str) -> Option<String> {
    for part in header.split(';') {
        let p = part.trim();
        if let Some(rest) = p.strip_prefix("filename*=") {
            if let Some(encoded) = rest.split("''").nth(1) {
                if let Ok(decoded) = urlencoding::decode(encoded) {
                    return Some(decoded.into_owned());
                }
            }
        } else if let Some(rest) = p.strip_prefix("filename=") {
            let unquoted = rest.trim().trim_matches('"');
            if !unquoted.is_empty() {
                return Some(unquoted.to_string());
            }
        }
    }
    None
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut val = bytes as f64;
    let mut idx = 0;
    while val >= 1024.0 && idx < UNITS.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{bytes} B")
    } else {
        format!("{val:.2} {}", UNITS[idx])
    }
}

pub fn format_speed(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_default_base_sniffer_geo_dns_tun_and_rules_when_missing() {
        let minimal = "proxies: []\n";
        let merged =
            merge_runtime_config(minimal, true, false, "127.0.0.1:9090", "", 7890).unwrap();
        let val: Value = yaml_serde::from_str(&merged).unwrap();
        let map = val.as_mapping().unwrap();

        // No redir-port on Windows
        assert!(!map.contains_key(Value::String("redir-port".into())));

        // Base scalar defaults
        assert_eq!(map.get(Value::String("mixed-port".into())).unwrap().as_u64(), Some(7890));
        assert_eq!(map.get(Value::String("allow-lan".into())).unwrap().as_bool(), Some(false));
        assert_eq!(map.get(Value::String("log-level".into())).unwrap().as_str(), Some("info"));
        assert_eq!(map.get(Value::String("ipv6".into())).unwrap().as_bool(), Some(true));
        assert_eq!(map.get(Value::String("mode".into())).unwrap().as_str(), Some("rule"));
        assert_eq!(map.get(Value::String("unified-delay".into())).unwrap().as_bool(), Some(true));
        assert_eq!(map.get(Value::String("geo-auto-update".into())).unwrap().as_bool(), Some(true));
        assert_eq!(
            map.get(Value::String("geo-update-interval".into())).unwrap().as_u64(),
            Some(24)
        );

        // Profile defaults
        let profile = map.get(Value::String("profile".into())).unwrap().as_mapping().unwrap();
        assert_eq!(
            profile.get(Value::String("store-selected".into())).unwrap().as_bool(),
            Some(true)
        );
        assert_eq!(
            profile.get(Value::String("store-fake-ip".into())).unwrap().as_bool(),
            Some(true)
        );

        // GeoX URL defaults
        let geox = map.get(Value::String("geox-url".into())).unwrap().as_mapping().unwrap();
        assert!(geox
            .get(Value::String("geoip".into()))
            .unwrap()
            .as_str()
            .unwrap()
            .contains("geoip.dat"));
        assert!(geox
            .get(Value::String("geosite".into()))
            .unwrap()
            .as_str()
            .unwrap()
            .contains("geosite.dat"));
        assert!(geox
            .get(Value::String("mmdb".into()))
            .unwrap()
            .as_str()
            .unwrap()
            .contains("geoip.metadb"));
        assert!(geox
            .get(Value::String("asn".into()))
            .unwrap()
            .as_str()
            .unwrap()
            .contains("GeoLite2-ASN.mmdb"));

        // Sniffer defaults
        let sniffer = map.get(Value::String("sniffer".into())).unwrap().as_mapping().unwrap();
        assert_eq!(sniffer.get(Value::String("enable".into())).unwrap().as_bool(), Some(true));
        assert_eq!(
            sniffer.get(Value::String("override-destination".into())).unwrap().as_bool(),
            Some(false)
        );
        let sniff_map = sniffer.get(Value::String("sniff".into())).unwrap().as_mapping().unwrap();
        assert!(sniff_map.contains_key(Value::String("HTTP".into())));
        assert!(sniff_map.contains_key(Value::String("TLS".into())));
        assert!(sniff_map.contains_key(Value::String("QUIC".into())));

        // Fallback rules
        let rules = map.get(Value::String("rules".into())).unwrap().as_sequence().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].as_str(), Some("MATCH,DIRECT"));

        let dns = map.get(Value::String("dns".into())).unwrap().as_mapping().unwrap();
        assert_eq!(dns.get(Value::String("enable".into())).unwrap().as_bool(), Some(true));
        assert_eq!(
            dns.get(Value::String("enhanced-mode".into())).unwrap().as_str(),
            Some("fake-ip")
        );
        assert_eq!(
            dns.get(Value::String("fake-ip-range".into())).unwrap().as_str(),
            Some("198.18.0.1/16")
        );
        let filters = dns
            .get(Value::String("fake-ip-filter".into()))
            .unwrap()
            .as_sequence()
            .unwrap();
        assert!(filters.iter().any(|v| v.as_str() == Some("*.lan")));
        assert!(filters.iter().any(|v| v.as_str() == Some("hw.zuimeitianqi.com")));

        let tun = map.get(Value::String("tun".into())).unwrap().as_mapping().unwrap();
        assert_eq!(tun.get(Value::String("enable".into())).unwrap().as_bool(), Some(true));
        assert_eq!(tun.get(Value::String("device".into())).unwrap().as_str(), Some("Mihomo"));
        assert_eq!(tun.get(Value::String("mtu".into())).unwrap().as_u64(), Some(1492));
        assert_eq!(tun.get(Value::String("stack".into())).unwrap().as_str(), Some("system"));
        assert_eq!(tun.get(Value::String("strict-route".into())).unwrap().as_bool(), Some(false));
        assert_eq!(
            map.get(Value::String("external-controller".into())).unwrap().as_str(),
            Some("127.0.0.1:9090")
        );
    }

    #[test]
    fn preserves_user_overrides_and_avoids_port_conflict() {
        let custom = r#"
port: 7890
socks-port: 7891
allow-lan: true
log-level: warning
ipv6: false
mode: global
unified-delay: false
geo-auto-update: false
profile:
  store-fake-ip: false
geox-url:
  geosite: https://example.com/custom-geosite.dat
sniffer:
  enable: false
external-controller: 127.0.0.1:9999
dns:
  enable: false
  nameserver:
    - 1.1.1.1
tun:
  enable: false
  device: CustomTun
  mtu: 1500
  stack: mixed
rules:
  - DOMAIN-SUFFIX,example.com,PROXY
  - MATCH,DIRECT
"#;
        let merged =
            merge_runtime_config(custom, true, false, "127.0.0.1:9090", "", 7890).unwrap();
        let val: Value = yaml_serde::from_str(&merged).unwrap();
        let map = val.as_mapping().unwrap();

        // Must NOT inject mixed-port when user already uses port / socks-port
        assert!(!map.contains_key(Value::String("mixed-port".into())));
        assert_eq!(map.get(Value::String("port".into())).unwrap().as_u64(), Some(7890));
        assert_eq!(map.get(Value::String("allow-lan".into())).unwrap().as_bool(), Some(true));
        assert_eq!(map.get(Value::String("log-level".into())).unwrap().as_str(), Some("warning"));
        assert_eq!(map.get(Value::String("ipv6".into())).unwrap().as_bool(), Some(false));
        assert_eq!(map.get(Value::String("mode".into())).unwrap().as_str(), Some("global"));
        assert_eq!(map.get(Value::String("unified-delay".into())).unwrap().as_bool(), Some(false));
        assert_eq!(
            map.get(Value::String("geo-auto-update".into())).unwrap().as_bool(),
            Some(false)
        );

        // Profile sub-field merge preserves user's store-fake-ip=false while adding store-selected=true
        let profile = map.get(Value::String("profile".into())).unwrap().as_mapping().unwrap();
        assert_eq!(
            profile.get(Value::String("store-fake-ip".into())).unwrap().as_bool(),
            Some(false)
        );
        assert_eq!(
            profile.get(Value::String("store-selected".into())).unwrap().as_bool(),
            Some(true)
        );

        // geox-url preserves custom geosite while filling missing entries
        let geox = map.get(Value::String("geox-url".into())).unwrap().as_mapping().unwrap();
        assert_eq!(
            geox.get(Value::String("geosite".into())).unwrap().as_str(),
            Some("https://example.com/custom-geosite.dat")
        );
        assert!(geox.contains_key(Value::String("mmdb".into())));
        assert!(geox.contains_key(Value::String("asn".into())));

        // Explicit sniffer: { enable: false } is respected untouched
        let sniffer = map.get(Value::String("sniffer".into())).unwrap().as_mapping().unwrap();
        assert_eq!(sniffer.get(Value::String("enable".into())).unwrap().as_bool(), Some(false));
        assert!(!sniffer.contains_key(Value::String("sniff".into())));

        // Existing user rules are completely untouched
        let rules = map.get(Value::String("rules".into())).unwrap().as_sequence().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].as_str(), Some("DOMAIN-SUFFIX,example.com,PROXY"));

        let tun = map.get(Value::String("tun".into())).unwrap().as_mapping().unwrap();
        assert_eq!(tun.get(Value::String("device".into())).unwrap().as_str(), Some("CustomTun"));
        assert_eq!(tun.get(Value::String("mtu".into())).unwrap().as_u64(), Some(1500));
        assert_eq!(tun.get(Value::String("enable".into())).unwrap().as_bool(), Some(true));

        let dns = map.get(Value::String("dns".into())).unwrap().as_mapping().unwrap();
        assert_eq!(dns.get(Value::String("enable".into())).unwrap().as_bool(), Some(true));
        assert_eq!(
            map.get(Value::String("external-controller".into())).unwrap().as_str(),
            Some("127.0.0.1:9999")
        );
    }

    #[test]
    fn parses_subscription_userinfo() {
        let header = "upload=1048576; download=2097152; total=10485760; expire=1800000000";
        let info = SubscriptionInfo::parse_header(header).unwrap();
        assert_eq!(info.upload, 1048576);
        assert_eq!(info.download, 2097152);
        assert_eq!(info.total, 10485760);
        assert_eq!(info.expire, Some(1800000000));
        assert!((info.usage_ratio() - 0.3).abs() < 0.001);
    }

    #[test]
    fn injects_only_missing_section_when_one_exists() {
        let only_dns = "dns:\n  enable: true\n  nameserver:\n    - 223.5.5.5\nrules: []\n";
        let merged =
            merge_runtime_config(only_dns, true, false, "127.0.0.1:9090", "", 7895).unwrap();
        let val: Value = yaml_serde::from_str(&merged).unwrap();
        let map = val.as_mapping().unwrap();

        assert_eq!(map.get(Value::String("mixed-port".into())).unwrap().as_u64(), Some(7895));
        let rules = map.get(Value::String("rules".into())).unwrap().as_sequence().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].as_str(), Some("MATCH,DIRECT"));

        let dns = map.get(Value::String("dns".into())).unwrap().as_mapping().unwrap();
        let ns = dns.get(Value::String("nameserver".into())).unwrap().as_sequence().unwrap();
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0].as_str(), Some("223.5.5.5"));

        let tun = map.get(Value::String("tun".into())).unwrap().as_mapping().unwrap();
        assert_eq!(tun.get(Value::String("device".into())).unwrap().as_str(), Some("Mihomo"));
        assert_eq!(tun.get(Value::String("enable".into())).unwrap().as_bool(), Some(true));
    }

    #[test]
    fn normalizes_controller_addresses() {
        assert_eq!(normalize_controller_addr(":9090"), "127.0.0.1:9090");
        assert_eq!(normalize_controller_addr("0.0.0.0:9090"), "127.0.0.1:9090");
        assert_eq!(normalize_controller_addr("http://127.0.0.1:9090/"), "127.0.0.1:9090");
    }

    #[test]
    fn minimal_initial_config_is_valid_yaml_with_defaults() {
        let yaml = minimal_initial_config_yaml();
        let info = inspect_yaml_str(&yaml).unwrap();
        assert_eq!(info.controller_addr.as_deref(), Some("127.0.0.1:9090"));
        assert_eq!(info.mixed_port, Some(7890));
        assert_eq!(info.tun_enabled, Some(false));
        assert!(info.has_custom_dns);
        assert!(info.has_custom_tun);
    }

    #[test]
    fn discovers_paths_from_builtin_service_cmdline() {
        let cmd = r#""C:\Program Files\Mihomo\mihomo-desktop.exe" --service --service-name mihomo --core "C:\Program Files\Mihomo\mihomo.exe" --work-dir "C:\ProgramData\Mihomo" --config "C:\ProgramData\Mihomo\config\clash.yaml""#;
        let d = discover_from_service_binary_cmdline(cmd).unwrap();
        assert_eq!(d.core_binary_path.as_deref(), Some(r"C:\Program Files\Mihomo\mihomo.exe"));
        assert_eq!(d.work_dir.as_deref(), Some(r"C:\ProgramData\Mihomo"));
        assert_eq!(d.config_path.as_deref(), Some(r"C:\ProgramData\Mihomo\config\clash.yaml"));
    }

    #[test]
    fn default_settings_use_light_mode() {
        let defaults = AppSettings::default();
        assert!(!defaults.dark_mode);
    }
}
