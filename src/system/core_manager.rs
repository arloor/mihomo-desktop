use crate::config::cores_dir;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::time::Duration;

const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/MetaCubeX/mihomo/releases?per_page=20";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

impl GitHubRelease {
    /// Returns Windows-relevant mihomo core archives sorted with preferred amd64 builds first.
    pub fn windows_assets(&self) -> Vec<&GitHubAsset> {
        let mut list: Vec<&GitHubAsset> = self
            .assets
            .iter()
            .filter(|a| {
                let n = a.name.to_ascii_lowercase();
                n.contains("windows")
                    && (n.ends_with(".zip") || n.ends_with(".gz") || n.ends_with(".exe"))
                    && !n.contains("sha256")
            })
            .collect();

        list.sort_by_key(|a| asset_priority(&a.name));
        list
    }
}

fn asset_priority(name: &str) -> u8 {
    let n = name.to_ascii_lowercase();
    if n.contains("amd64-v2") {
        0
    } else if n.contains("amd64-v1") {
        1
    } else if n.contains("amd64-compatible") {
        2
    } else if n.contains("amd64-v3") {
        3
    } else if n.contains("amd64") {
        4
    } else if n.contains("arm64") {
        5
    } else {
        10
    }
}

#[derive(Debug, Clone)]
pub struct LocalCoreVersion {
    pub tag: String,
    pub exe_path: PathBuf,
    pub size_bytes: u64,
}

pub fn list_local_cores() -> Vec<LocalCoreVersion> {
    let dir = cores_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let exe = path.join("mihomo.exe");
            if exe.exists() {
                let size_bytes = fs::metadata(&exe).map(|m| m.len()).unwrap_or(0);
                let tag = entry.file_name().to_string_lossy().to_string();
                result.push(LocalCoreVersion {
                    tag,
                    exe_path: exe,
                    size_bytes,
                });
            }
        }
    }
    result.sort_by(|a, b| b.tag.cmp(&a.tag));
    result
}

pub fn delete_local_core(tag: &str) -> Result<()> {
    let dir = cores_dir().join(sanitize_tag(tag));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

fn build_http_client(proxy_url: Option<&str>, timeout_secs: u64) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent(concat!("mihomo-desktop/", env!("CARGO_PKG_VERSION")));
    if let Some(p) = proxy_url {
        if let Ok(proxy) = reqwest::Proxy::all(p) {
            builder = builder.proxy(proxy);
        }
    } else {
        builder = builder.no_proxy();
    }
    Ok(builder.build()?)
}

/// Fetch release list from GitHub (`MetaCubeX/mihomo`).
/// If `proxy_url` is supplied, tries via local proxy first and falls back to direct connection.
pub async fn fetch_github_releases(
    include_prerelease: bool,
    proxy_url: Option<&str>,
) -> Result<Vec<GitHubRelease>> {
    let client = build_http_client(proxy_url, 25)?;
    let resp = match client.get(GITHUB_RELEASES_API).send().await {
        Ok(r) => r,
        Err(first_err) if proxy_url.is_some() => {
            let direct = build_http_client(None, 25)?;
            direct.get(GITHUB_RELEASES_API).send().await.with_context(|| {
                format!("拉取 GitHub Release 列表失败 (代理错误: {first_err})")
            })?
        }
        Err(e) => return Err(e).context("连接 GitHub API 失败"),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GitHub API 返回错误 ({status}): {body}");
    }

    let mut releases: Vec<GitHubRelease> = resp.json().await.context("解析 GitHub Release JSON 失败")?;
    if !include_prerelease {
        releases.retain(|r| !r.prerelease);
    }
    Ok(releases)
}

/// Download a GitHub release asset and extract `mihomo.exe` into `%APPDATA%\mihomo-desktop\cores\<tag>-<variant>\mihomo.exe`.
pub async fn download_and_extract_asset(
    tag: &str,
    asset: &GitHubAsset,
    proxy_url: Option<&str>,
) -> Result<LocalCoreVersion> {
    let client = build_http_client(proxy_url, 180)?;
    let resp = match client.get(&asset.browser_download_url).send().await {
        Ok(r) => r,
        Err(first_err) if proxy_url.is_some() => {
            let direct = build_http_client(None, 180)?;
            direct
                .get(&asset.browser_download_url)
                .send()
                .await
                .with_context(|| format!("下载核心失败 (代理错误: {first_err})"))?
        }
        Err(e) => return Err(e).context("下载核心失败"),
    };

    if !resp.status().is_success() {
        anyhow::bail!("下载核心失败，HTTP 状态码: {}", resp.status());
    }

    let bytes = resp.bytes().await.context("读取下载数据流失败")?;

    let folder_name = sanitize_tag(&format!("{tag}-{}", short_variant_name(&asset.name)));
    let target_dir = cores_dir().join(&folder_name);
    fs::create_dir_all(&target_dir)?;
    let target_exe = target_dir.join("mihomo.exe");

    let lower = asset.name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.as_ref()))
            .context("解压 .zip 压缩包失败")?;
        let mut extracted = false;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_ascii_lowercase();
            if name.ends_with(".exe") {
                let mut out = fs::File::create(&target_exe)?;
                std::io::copy(&mut file, &mut out)?;
                extracted = true;
                break;
            }
        }
        if !extracted {
            anyhow::bail!("压缩包中未找到 .exe 可执行文件");
        }
    } else if lower.ends_with(".gz") {
        let mut decoder = GzDecoder::new(Cursor::new(bytes.as_ref()));
        let mut buf = Vec::new();
        decoder.read_to_end(&mut buf)?;
        fs::write(&target_exe, &buf)?;
    } else {
        fs::write(&target_exe, bytes.as_ref())?;
    }

    let size_bytes = fs::metadata(&target_exe).map(|m| m.len()).unwrap_or(0);
    Ok(LocalCoreVersion {
        tag: folder_name,
        exe_path: target_exe,
        size_bytes,
    })
}

pub fn short_variant_name(asset_name: &str) -> String {
    let stripped = asset_name
        .trim_end_matches(".zip")
        .trim_end_matches(".gz")
        .trim_end_matches(".exe");
    if let Some(idx) = stripped.find("windows-") {
        let rest = &stripped[idx..];
        // strip trailing -v1.xx.x if present
        let parts: Vec<&str> = rest.split('-').collect();
        if parts.len() >= 3 && parts[2].starts_with('v') && parts[2].contains('.') {
            format!("{}-{}", parts[0], parts[1])
        } else if parts.len() >= 4 && parts[3].starts_with('v') && parts[3].contains('.') {
            format!("{}-{}-{}", parts[0], parts[1], parts[2])
        } else {
            rest.to_string()
        }
    } else {
        stripped.to_string()
    }
}

fn sanitize_tag(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}
