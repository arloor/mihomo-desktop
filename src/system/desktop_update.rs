use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tempfile::TempPath;
use tokio::io::AsyncWriteExt;

pub use crate::version::{BASE_VERSION, VERSION};

const LATEST: &str = "https://api.github.com/repos/arloor/mihomo-desktop/releases/latest";

#[derive(Debug)]
pub enum UpdateState {
    Idle,
    Checking,
    Current,
    Downloading(String),
    Ready {
        version: String,
        installer: TempPath,
    },
    Failed(String),
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
}

pub async fn check(current: &str, downloading: impl FnOnce(String)) -> Result<UpdateState> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("mihomo-desktop/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .https_only(true)
        .build()?;
    let release: Release = client
        .get(LATEST)
        .send()
        .await
        .context("无法连接 GitHub")?
        .error_for_status()?
        .json()
        .await
        .context("无法读取发布信息")?;
    let tag = &release.tag_name;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).context("发布版本号无效")?;
    if !version.cmp_precedence(&Version::parse(current)?).is_gt() {
        return Ok(UpdateState::Current);
    }
    let name = format!("MihomoDesktop-Setup-{version}-x64.exe");
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .context("此版本尚未发布 Windows x64 安装包")?;
    let digest = asset
        .digest
        .as_deref()
        .and_then(|value| value.strip_prefix("sha256:"))
        .context("此安装包缺少 GitHub SHA-256 校验值")?;
    ensure!(asset.size > 0 && digest.len() == 64, "安装包信息不完整");

    downloading(version.to_string());
    let url = format!("https://github.com/arloor/mihomo-desktop/releases/download/{tag}/{name}");
    let mut response = client
        .get(url)
        .send()
        .await
        .context("安装包下载失败")?
        .error_for_status()?;
    let temp = tempfile::Builder::new()
        .prefix("MihomoDesktop-Setup-")
        .suffix(".exe")
        .tempfile()?;
    let (file, installer) = temp.into_parts();
    let mut file = tokio::fs::File::from_std(file);
    let mut hash = Sha256::new();
    let mut size = 0u64;
    while let Some(chunk) = response.chunk().await? {
        size += chunk.len() as u64;
        ensure!(size <= asset.size, "下载大小超过发布记录");
        hash.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    ensure!(
        size == asset.size && format!("{:x}", hash.finalize()).eq_ignore_ascii_case(digest),
        "安装包校验失败，请重试"
    );
    Ok(UpdateState::Ready {
        version: version.to_string(),
        installer,
    })
}

#[cfg(windows)]
pub fn launch(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    ensure!(
        result as isize > 32,
        "Windows 未启动安装程序（{}）",
        result as isize
    );
    Ok(())
}

#[cfg(not(windows))]
pub fn launch(_: &Path) -> Result<()> {
    anyhow::bail!("此安装包仅适用于 Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "downloads the real installer from GitHub"]
    async fn download_and_verify_released_setup() {
        let state = check("0.0.0", |_| {}).await.unwrap();
        let UpdateState::Ready { installer, .. } = state else {
            panic!("expected installer")
        };
        assert!(installer.is_file());
    }
}
