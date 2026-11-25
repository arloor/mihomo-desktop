use anyhow::{Context, Result};

const APP_REG_NAME: &str = "MihomoDesktop";

/// Synchronize Windows Registry auto-start entry strictly with `resident_mode`.
/// Resident mode ON => auto-start enabled (`--minimized`); OFF => auto-start disabled.
pub fn sync_autostart(resident_mode: bool) -> Result<()> {
    set_autostart(resident_mode)
}

#[cfg(windows)]
mod win {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};

    const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn set_autostart(enable: bool) -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(RUN_KEY_PATH, KEY_WRITE)
            .context("打开 Windows 开机自启注册表失败")?;

        if enable {
            let exe = std::env::current_exe().context("获取程序路径失败")?;
            // Launch silently into system tray on Windows sign-in
            let cmd = format!("\"{}\" --minimized", exe.to_string_lossy());
            key.set_value(APP_REG_NAME, &cmd)
                .context("写入开机自启注册表失败")?;
        } else {
            let _ = key.delete_value(APP_REG_NAME);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod win {
    use super::*;
    pub fn set_autostart(_enable: bool) -> Result<()> {
        anyhow::bail!("开机自启仅支持 Windows")
    }
}

pub use win::*;
