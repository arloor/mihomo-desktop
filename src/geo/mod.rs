use crate::mihomo::MihomoClient;
use anyhow::{Context, Result, bail};
use futures_util::stream::{self, StreamExt};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime};
use yaml_serde::Value;

const MMDB_METADATA_MARKER: &[u8] = b"\xab\xcd\xefMaxMind.com";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GeoRegion {
    /// Uppercase ISO 3166-1 alpha-2 country/region code (e.g. "US", "HK", "JP", "SG", "CN").
    Country(String),
    /// Loopback, LAN, or private address space.
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeGeoInfo {
    pub host: String,
    pub resolved_ip: Option<String>,
    pub region: GeoRegion,
}

/// Pure-Rust MaxMind DB v2 reader supporting both MetaCubeX `geoip.metadb` (`Meta-geoip0`)
/// and standard GeoLite2 `Country.mmdb`.
#[derive(Debug)]
pub struct MmdbReader {
    data: Vec<u8>,
    node_count: usize,
    record_size: usize,
    node_byte_size: usize,
    data_section_start: usize,
    ip_version: u16,
    ipv4_start_node: usize,
}

#[derive(Debug, Clone)]
enum MmdbValue {
    String(String),
    Uint(u64),
    Array(Vec<MmdbValue>),
    Map(HashMap<String, MmdbValue>),
    Other,
}

impl MmdbReader {
    pub fn from_file(path: &Path) -> Result<Self> {
        let data = std::fs::read(path)
            .with_context(|| format!("读取 GeoIP 数据库失败: {}", path.display()))?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let search_start = data.len().saturating_sub(128 * 1024);
        let rel_idx = data[search_start..]
            .windows(MMDB_METADATA_MARKER.len())
            .rposition(|w| w == MMDB_METADATA_MARKER)
            .context("未在文件中找到 MaxMind DB 元数据标记")?;
        let meta_start = search_start + rel_idx + MMDB_METADATA_MARKER.len();

        let (meta_val, _) = decode_value(&data, meta_start, meta_start, 0)?;
        let MmdbValue::Map(meta) = meta_val else {
            bail!("MMDB 元数据不是有效的 Map 结构");
        };

        let node_count = match meta.get("node_count") {
            Some(MmdbValue::Uint(v)) => *v as usize,
            _ => bail!("MMDB 元数据缺少 node_count"),
        };
        let record_size = match meta.get("record_size") {
            Some(MmdbValue::Uint(v)) => *v as usize,
            _ => bail!("MMDB 元数据缺少 record_size"),
        };
        if !matches!(record_size, 24 | 28 | 32) {
            bail!("不支持的 MMDB record_size: {record_size}");
        }
        let ip_version = match meta.get("ip_version") {
            Some(MmdbValue::Uint(v)) => *v as u16,
            _ => 6,
        };

        let node_byte_size = (record_size * 2) / 8;
        let search_tree_size = node_count
            .checked_mul(node_byte_size)
            .context("MMDB 搜索树尺寸溢出")?;
        let data_section_start = search_tree_size + 16;
        if data_section_start > data.len() {
            bail!("MMDB 文件已截断或损坏");
        }

        let mut reader = Self {
            data,
            node_count,
            record_size,
            node_byte_size,
            data_section_start,
            ip_version,
            ipv4_start_node: 0,
        };

        if reader.ip_version == 6 {
            reader.ipv4_start_node = reader.compute_ipv4_start_node();
        }

        Ok(reader)
    }

    fn compute_ipv4_start_node(&self) -> usize {
        let mut node = 0usize;
        for _ in 0..96 {
            if node >= self.node_count {
                break;
            }
            match self.read_record(node, 0) {
                Some(next) => node = next,
                None => break,
            }
        }
        node
    }

    #[inline]
    fn read_record(&self, node: usize, bit: u8) -> Option<usize> {
        let off = node.checked_mul(self.node_byte_size)?;
        let d = &self.data;
        match self.record_size {
            24 => {
                let start = off + if bit == 0 { 0 } else { 3 };
                if start + 3 > d.len() {
                    return None;
                }
                Some(((d[start] as usize) << 16) | ((d[start + 1] as usize) << 8) | (d[start + 2] as usize))
            }
            28 => {
                if off + 7 > d.len() {
                    return None;
                }
                let mid = d[off + 3] as usize;
                if bit == 0 {
                    Some(((mid >> 4) << 24) | ((d[off] as usize) << 16) | ((d[off + 1] as usize) << 8) | (d[off + 2] as usize))
                } else {
                    Some(((mid & 0x0f) << 24) | ((d[off + 4] as usize) << 16) | ((d[off + 5] as usize) << 8) | (d[off + 6] as usize))
                }
            }
            32 => {
                let start = off + if bit == 0 { 0 } else { 4 };
                if start + 4 > d.len() {
                    return None;
                }
                Some(u32::from_be_bytes([d[start], d[start + 1], d[start + 2], d[start + 3]]) as usize)
            }
            _ => None,
        }
    }

    pub fn lookup_region(&self, ip: IpAddr) -> Option<GeoRegion> {
        if is_fake_ip(ip) {
            return None;
        }
        if is_private_ip(ip) {
            return Some(GeoRegion::Private);
        }

        match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                if self.ip_version == 6 {
                    if self.ipv4_start_node > self.node_count {
                        self.decode_region_at_record(self.ipv4_start_node)
                    } else if self.ipv4_start_node == self.node_count {
                        None
                    } else {
                        self.walk_bits(self.ipv4_start_node, &octets)
                    }
                } else {
                    self.walk_bits(0, &octets)
                }
            }
            IpAddr::V6(v6) => {
                if self.ip_version != 6 {
                    return None;
                }
                let octets = v6.octets();
                self.walk_bits(0, &octets)
            }
        }
    }

    fn walk_bits(&self, start_node: usize, bytes: &[u8]) -> Option<GeoRegion> {
        let mut node = start_node;
        for &byte in bytes {
            for shift in (0..8).rev() {
                let bit = (byte >> shift) & 1;
                let rec = self.read_record(node, bit)?;
                if rec == self.node_count {
                    return None;
                }
                if rec > self.node_count {
                    return self.decode_region_at_record(rec);
                }
                node = rec;
            }
        }
        None
    }

    fn decode_region_at_record(&self, rec: usize) -> Option<GeoRegion> {
        let offset = (rec - self.node_count).checked_sub(16)?;
        let pos = self.data_section_start.checked_add(offset)?;
        let (val, _) = decode_value(&self.data, pos, self.data_section_start, 0).ok()?;
        extract_region_from_mmdb_value(&val)
    }
}

fn extract_region_from_mmdb_value(val: &MmdbValue) -> Option<GeoRegion> {
    match val {
        // MetaCubeX Meta-geoip0 single string (e.g. "cn", "jp", "private")
        MmdbValue::String(s) => parse_geo_tag(s),
        // MetaCubeX Meta-geoip0 array of tags (e.g. ["us", "google"] or ["cloudflare", "us"])
        MmdbValue::Array(items) => {
            let mut saw_private = false;
            for item in items {
                if let MmdbValue::String(s) = item {
                    match parse_geo_tag(s) {
                        Some(GeoRegion::Country(cc)) => return Some(GeoRegion::Country(cc)),
                        Some(GeoRegion::Private) => saw_private = true,
                        None => {}
                    }
                }
            }
            saw_private.then_some(GeoRegion::Private)
        }
        // Standard MaxMind GeoLite2 Country.mmdb map
        MmdbValue::Map(map) => {
            for key in ["country", "registered_country", "represented_country"] {
                if let Some(MmdbValue::Map(sub)) = map.get(key) {
                    if let Some(MmdbValue::String(code)) = sub.get("iso_code") {
                        if let Some(region) = parse_geo_tag(code) {
                            return Some(region);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_geo_tag(raw: &str) -> Option<GeoRegion> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("private") || trimmed.eq_ignore_ascii_case("lan") {
        return Some(GeoRegion::Private);
    }
    if trimmed.len() == 2 && trimmed.bytes().all(|b| b.is_ascii_alphabetic()) {
        let upper = trimmed.to_ascii_uppercase();
        if upper != "XX" && upper != "ZZ" {
            return Some(GeoRegion::Country(upper));
        }
    }
    None
}

fn decode_value(buf: &[u8], mut pos: usize, base: usize, depth: u8) -> Result<(MmdbValue, usize)> {
    if depth > 16 {
        bail!("MMDB 指针递归过深");
    }
    let ctrl = *buf.get(pos).context("MMDB 读取越界")?;
    pos += 1;

    let mut typ = ctrl >> 5;
    if typ == 0 {
        let ext = *buf.get(pos).context("MMDB 扩展类型越界")?;
        pos += 1;
        typ = 7 + ext;
    }

    // Pointer type (1)
    if typ == 1 {
        let psize = (ctrl >> 3) & 0x03;
        let pbits = (ctrl & 0x07) as usize;
        let ptr = match psize {
            0 => {
                let b0 = *buf.get(pos).context("MMDB 指针越界")? as usize;
                pos += 1;
                (pbits << 8) | b0
            }
            1 => {
                let b0 = *buf.get(pos).context("MMDB 指针越界")? as usize;
                let b1 = *buf.get(pos + 1).context("MMDB 指针越界")? as usize;
                pos += 2;
                ((pbits << 16) | (b0 << 8) | b1) + 2048
            }
            2 => {
                let b0 = *buf.get(pos).context("MMDB 指针越界")? as usize;
                let b1 = *buf.get(pos + 1).context("MMDB 指针越界")? as usize;
                let b2 = *buf.get(pos + 2).context("MMDB 指针越界")? as usize;
                pos += 3;
                ((pbits << 24) | (b0 << 16) | (b1 << 8) | b2) + 526_336
            }
            _ => {
                if pos + 4 > buf.len() {
                    bail!("MMDB 32位指针越界");
                }
                let p = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]])
                    as usize;
                pos += 4;
                p
            }
        };
        let target = base.checked_add(ptr).context("MMDB 指针偏移溢出")?;
        let (val, _) = decode_value(buf, target, base, depth + 1)?;
        return Ok((val, pos));
    }

    let mut size = (ctrl & 0x1f) as usize;
    if size == 29 {
        let b0 = *buf.get(pos).context("MMDB 长度越界")? as usize;
        pos += 1;
        size = 29 + b0;
    } else if size == 30 {
        if pos + 2 > buf.len() {
            bail!("MMDB 长度越界");
        }
        size = 285 + u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        pos += 2;
    } else if size == 31 {
        if pos + 3 > buf.len() {
            bail!("MMDB 长度越界");
        }
        size = 65_821
            + (((buf[pos] as usize) << 16) | ((buf[pos + 1] as usize) << 8) | (buf[pos + 2] as usize));
        pos += 3;
    }

    match typ {
        2 => {
            if pos + size > buf.len() {
                bail!("MMDB 字符串越界");
            }
            let s = String::from_utf8_lossy(&buf[pos..pos + size]).into_owned();
            Ok((MmdbValue::String(s), pos + size))
        }
        5 | 6 | 9 => {
            if pos + size > buf.len() {
                bail!("MMDB 无符号整数越界");
            }
            let mut n = 0u64;
            for &b in &buf[pos..pos + size] {
                n = (n << 8) | (b as u64);
            }
            Ok((MmdbValue::Uint(n), pos + size))
        }
        7 => {
            let mut map = HashMap::with_capacity(size.min(64));
            for _ in 0..size {
                let (k, next) = decode_value(buf, pos, base, depth + 1)?;
                pos = next;
                let (v, next) = decode_value(buf, pos, base, depth + 1)?;
                pos = next;
                if let MmdbValue::String(ks) = k {
                    map.insert(ks, v);
                }
            }
            Ok((MmdbValue::Map(map), pos))
        }
        11 => {
            let mut arr = Vec::with_capacity(size.min(64));
            for _ in 0..size {
                let (v, next) = decode_value(buf, pos, base, depth + 1)?;
                pos = next;
                arr.push(v);
            }
            Ok((MmdbValue::Array(arr), pos))
        }
        14 => Ok((MmdbValue::Other, pos)),
        // Other primitive types (double, bytes, int32, uint128, float, container, end marker)
        3 | 4 | 8 | 10 | 15 => {
            if pos + size > buf.len() {
                bail!("MMDB 负载越界");
            }
            Ok((MmdbValue::Other, pos + size))
        }
        _ => Ok((MmdbValue::Other, pos)),
    }
}

pub fn is_fake_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 198 && (o[1] == 18 || o[1] == 19)
        }
        IpAddr::V6(_) => false,
    }
}

pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // CGNAT 100.64.0.0/10
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (seg0 & 0xfe00) == 0xfc00 // Unique local fc00::/7
                || (seg0 & 0xffc0) == 0xfe80 // Link-local fe80::/10
        }
    }
}

/// Locate the best available MMDB database (`geoip.metadb` or `Country.mmdb`).
pub fn find_geoip_database(work_dir: &Path, core_binary_path: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let names = ["geoip.metadb", "Country.mmdb", "GeoIP.mmdb"];

    for name in names {
        candidates.push(work_dir.join(name));
    }
    if let Some(core_dir) = core_binary_path.parent() {
        for name in names {
            candidates.push(core_dir.join("geo").join(name));
            candidates.push(core_dir.join(name));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for name in names {
                candidates.push(exe_dir.join("geo").join(name));
                candidates.push(exe_dir.join(name));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for rel in ["installer/geo", "dist/stage/geo"] {
            for name in names {
                candidates.push(cwd.join(rel).join(name));
            }
        }
    }

    candidates
        .into_iter()
        .find(|p| p.metadata().map(|m| m.is_file() && m.len() > 1024).unwrap_or(false))
}

struct CachedMmdb {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    reader: Arc<MmdbReader>,
}

static MMDB_CACHE: OnceLock<RwLock<Option<CachedMmdb>>> = OnceLock::new();

pub fn load_cached_mmdb(work_dir: &Path, core_binary_path: &Path) -> Option<Arc<MmdbReader>> {
    let path = find_geoip_database(work_dir, core_binary_path)?;
    let meta = std::fs::metadata(&path).ok()?;
    let modified = meta.modified().ok();
    let len = meta.len();

    let lock = MMDB_CACHE.get_or_init(|| RwLock::new(None));
    if let Ok(guard) = lock.read() {
        if let Some(cached) = guard.as_ref() {
            if cached.path == path && cached.modified == modified && cached.len == len {
                return Some(Arc::clone(&cached.reader));
            }
        }
    }

    let reader = Arc::new(MmdbReader::from_file(&path).ok()?);
    if let Ok(mut guard) = lock.write() {
        *guard = Some(CachedMmdb {
            path,
            modified,
            len,
            reader: Arc::clone(&reader),
        });
    }
    Some(reader)
}

/// Extract `proxy_name -> server_host` from the active Mihomo YAML config and any referenced
/// local proxy-provider files.
pub fn extract_proxy_hosts_from_config(
    config_path: &Path,
    work_dir: &Path,
) -> HashMap<String, String> {
    let mut hosts = HashMap::new();
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return hosts;
    };
    extract_proxy_hosts_from_yaml(&content, Some(work_dir), &mut hosts);
    hosts
}

pub fn extract_proxy_hosts_from_yaml(
    yaml_text: &str,
    work_dir: Option<&Path>,
    out: &mut HashMap<String, String>,
) {
    let Ok(root) = yaml_serde::from_str::<Value>(yaml_text) else {
        return;
    };
    let Some(map) = root.as_mapping() else {
        return;
    };

    // 1. Direct `proxies` list
    if let Some(seq) = map
        .get(Value::String("proxies".into()))
        .and_then(|v| v.as_sequence())
    {
        extract_proxies_sequence(seq, out);
    }

    // 2. Local `proxy-providers` files
    if let (Some(wd), Some(providers)) = (
        work_dir,
        map.get(Value::String("proxy-providers".into()))
            .and_then(|v| v.as_mapping()),
    ) {
        for (k, v) in providers {
            let Some(prov_name) = k.as_str() else {
                continue;
            };
            let rel_path = v
                .as_mapping()
                .and_then(|m| m.get(Value::String("path".into())))
                .and_then(|p| p.as_str());
            let provider_file = match rel_path {
                Some(p) if !p.trim().is_empty() => {
                    let pb = PathBuf::from(p.trim());
                    if pb.is_absolute() {
                        pb
                    } else {
                        wd.join(pb)
                    }
                }
                _ => wd.join("proxies").join(prov_name),
            };
            if let Ok(prov_content) = std::fs::read_to_string(&provider_file) {
                extract_provider_content(&prov_content, out);
            }
        }
    }
}

fn extract_proxies_sequence(seq: &[Value], out: &mut HashMap<String, String>) {
    for item in seq {
        let Some(m) = item.as_mapping() else {
            continue;
        };
        let Some(name) = m
            .get(Value::String("name".into()))
            .and_then(yaml_scalar_to_string)
        else {
            continue;
        };

        let server = m
            .get(Value::String("server".into()))
            .and_then(yaml_scalar_to_string)
            .or_else(|| {
                // WireGuard configs can specify server inside `peers[0].server`
                m.get(Value::String("peers".into()))
                    .and_then(|v| v.as_sequence())
                    .and_then(|peers| peers.first())
                    .and_then(|p| p.as_mapping())
                    .and_then(|pm| pm.get(Value::String("server".into())))
                    .and_then(yaml_scalar_to_string)
            });

        if let Some(raw_host) = server {
            if let Some(clean) = normalize_host(&raw_host) {
                out.insert(name, clean);
            }
        }
    }
}

fn extract_provider_content(content: &str, out: &mut HashMap<String, String>) {
    // First try standard Clash/Mihomo provider YAML (`proxies: [...]`)
    if let Ok(val) = yaml_serde::from_str::<Value>(content) {
        if let Some(seq) = val
            .as_mapping()
            .and_then(|m| m.get(Value::String("proxies".into())))
            .and_then(|v| v.as_sequence())
        {
            extract_proxies_sequence(seq, out);
            return;
        }
    }

    // Fallback: raw URI lines (e.g. ss://, vless://, trojan://, hysteria2://)
    for line in content.lines() {
        if let Some((name, host)) = parse_proxy_uri_host(line.trim()) {
            out.entry(name).or_insert(host);
        }
    }
}

fn parse_proxy_uri_host(uri: &str) -> Option<(String, String)> {
    let (_scheme, rest) = uri.split_once("://")?;
    let (before_frag, raw_name) = rest.rsplit_once('#')?;
    let name = urlencoding::decode(raw_name).ok()?.into_owned();
    if name.trim().is_empty() {
        return None;
    }
    let after_auth = before_frag.rsplit_once('@').map(|(_, r)| r).unwrap_or(before_frag);
    let authority = after_auth
        .split(['/', '?'])
        .next()
        .unwrap_or(after_auth)
        .trim();
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next()?
    } else {
        authority.rsplit_once(':').map(|(h, _)| h).unwrap_or(authority)
    };
    Some((name, normalize_host(host)?))
}

fn yaml_scalar_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn normalize_host(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Resolve a server hostname to a real (non-FakeIP) IP address.
/// Uses Mihomo's `/dns/query` first (which bypasses TUN FakeIP), falling back to system DNS.
pub async fn resolve_host_ip(host: &str, client: Option<&MihomoClient>) -> Option<IpAddr> {
    let clean = normalize_host(host)?;
    if clean.eq_ignore_ascii_case("localhost") {
        return Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    if let Ok(ip) = clean.parse::<IpAddr>() {
        return Some(ip);
    }

    // 1. Prefer Mihomo /dns/query (A then AAAA) so TUN FakeIP does not poison the lookup
    if let Some(c) = client {
        for rtype in ["A", "AAAA"] {
            if let Ok(Ok(resp)) =
                tokio::time::timeout(Duration::from_secs(3), c.query_dns(&clean, rtype)).await
            {
                if let Some(answers) = resp.answer {
                    for ans in answers {
                        if let Ok(ip) = ans.data.trim().parse::<IpAddr>() {
                            if !is_fake_ip(ip) && !ip.is_unspecified() {
                                return Some(ip);
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Fallback to Tokio/OS DNS resolver (filtering FakeIP 198.18.0.0/15)
    if let Ok(Ok(addrs)) =
        tokio::time::timeout(Duration::from_secs(3), tokio::net::lookup_host((clean.as_str(), 0)))
            .await
    {
        let mut fallback_v6: Option<IpAddr> = None;
        for addr in addrs {
            let ip = addr.ip();
            if is_fake_ip(ip) || ip.is_unspecified() {
                continue;
            }
            if ip.is_ipv4() {
                return Some(ip);
            }
            if fallback_v6.is_none() {
                fallback_v6 = Some(ip);
            }
        }
        if fallback_v6.is_some() {
            return fallback_v6;
        }
    }

    // IPv6 loopback edge case
    if clean == "::1" {
        return Some(IpAddr::V6(Ipv6Addr::LOCALHOST));
    }

    None
}

/// Resolve GeoIP region info for all proxy nodes in `active_config_path` concurrently.
/// Reuses `existing_host_cache` for already-resolved hosts to avoid redundant DNS queries.
pub async fn resolve_all_proxy_geos(
    config_path: PathBuf,
    work_dir: PathBuf,
    core_binary_path: PathBuf,
    client: Option<MihomoClient>,
    existing_host_cache: HashMap<String, (Option<String>, GeoRegion)>,
) -> (
    HashMap<String, NodeGeoInfo>,
    HashMap<String, (Option<String>, GeoRegion)>,
) {
    let mmdb = load_cached_mmdb(&work_dir, &core_binary_path);
    let node_hosts = tokio::task::spawn_blocking(move || {
        extract_proxy_hosts_from_config(&config_path, &work_dir)
    })
    .await
    .unwrap_or_default();

    if node_hosts.is_empty() {
        return (HashMap::new(), existing_host_cache);
    }

    // Deduplicate unique server hosts that are not yet cached
    let mut pending_hosts: Vec<String> = Vec::new();
    for host in node_hosts.values() {
        let key = host.to_ascii_lowercase();
        if !existing_host_cache.contains_key(&key) && !pending_hosts.contains(&key) {
            pending_hosts.push(key);
        }
    }

    let mut host_cache = existing_host_cache;

    if !pending_hosts.is_empty() {
        let resolved_list: Vec<(String, Option<String>, Option<GeoRegion>)> =
            stream::iter(pending_hosts)
                .map(|host| {
                    let client = client.clone();
                    let mmdb = mmdb.clone();
                    async move {
                        let ip = resolve_host_ip(&host, client.as_ref()).await;
                        let region = match (ip, mmdb.as_ref()) {
                            (Some(addr), Some(db)) => db.lookup_region(addr),
                            (Some(addr), None) if is_private_ip(addr) => Some(GeoRegion::Private),
                            _ => None,
                        };
                        (host, ip.map(|a| a.to_string()), region)
                    }
                })
                .buffer_unordered(12)
                .collect()
                .await;

        for (host, ip_str, region) in resolved_list {
            if let Some(reg) = region {
                host_cache.insert(host, (ip_str, reg));
            }
        }
    }

    let mut node_geos = HashMap::with_capacity(node_hosts.len());
    for (node_name, host) in node_hosts {
        let key = host.to_ascii_lowercase();
        if let Some((resolved_ip, region)) = host_cache.get(&key) {
            node_geos.insert(
                node_name,
                NodeGeoInfo {
                    host,
                    resolved_ip: resolved_ip.clone(),
                    region: region.clone(),
                },
            );
        }
    }

    (node_geos, host_cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_fake_ip_and_detects_private_ip() {
        assert!(is_fake_ip("198.18.0.1".parse().unwrap()));
        assert!(is_fake_ip("198.19.255.254".parse().unwrap()));
        assert!(!is_fake_ip("8.8.8.8".parse().unwrap()));

        assert!(is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(is_private_ip("192.168.1.10".parse().unwrap()));
        assert!(is_private_ip("10.0.0.1".parse().unwrap()));
        assert!(is_private_ip("::1".parse().unwrap()));
        assert!(is_private_ip("fd12:3456::1".parse().unwrap()));
        assert!(!is_private_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn extracts_hosts_from_yaml_including_ipv6_and_wireguard_peers() {
        let yaml = r#"
proxies:
  - name: us
    type: http
    server: us.example.com
    port: 443
  - name: us6
    type: http
    server: "[2001:4860:4860::8888]"
    port: 443
  - name: mac-wg
    type: wireguard
    peers:
      - server: 192.168.50.2
        port: 51820
"#;
        let mut out = HashMap::new();
        extract_proxy_hosts_from_yaml(yaml, None, &mut out);
        assert_eq!(out.get("us").map(String::as_str), Some("us.example.com"));
        assert_eq!(out.get("us6").map(String::as_str), Some("2001:4860:4860::8888"));
        assert_eq!(out.get("mac-wg").map(String::as_str), Some("192.168.50.2"));
    }

    #[test]
    fn reads_bundled_geoip_metadb_accurately() {
        let db_path = Path::new("installer/geo/geoip.metadb");
        if !db_path.exists() {
            return;
        }
        let reader = MmdbReader::from_file(db_path).expect("should parse bundled geoip.metadb");
        assert_eq!(
            reader.lookup_region("8.8.8.8".parse().unwrap()),
            Some(GeoRegion::Country("US".to_string()))
        );
        assert_eq!(
            reader.lookup_region("223.5.5.5".parse().unwrap()),
            Some(GeoRegion::Country("CN".to_string()))
        );
        assert_eq!(
            reader.lookup_region("210.140.92.180".parse().unwrap()),
            Some(GeoRegion::Country("JP".to_string()))
        );
        assert_eq!(
            reader.lookup_region("2001:4860:4860::8888".parse().unwrap()),
            Some(GeoRegion::Country("US".to_string()))
        );
        assert_eq!(
            reader.lookup_region("127.0.0.1".parse().unwrap()),
            Some(GeoRegion::Private)
        );
        assert_eq!(reader.lookup_region("198.18.0.5".parse().unwrap()), None);
    }
}
