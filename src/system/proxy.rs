use anyhow::{Context, Result};

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub server: String,
    pub bypass: String,
}

/// Three explicit takeover states for Windows System Proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemProxyTakeoverState {
    /// 未接管：应用未设置为接管系统代理
    Unmanaged,
    /// 接管并且生效：应用已接管，系统注册表已指向本机 Mihomo 代理端口且服务运行中
    ManagedEffective,
    /// 设置为接管但未生效：应用设置为接管，但系统代理未启用/被外部修改，或 Mihomo 服务未运行
    ManagedIneffective,
}

impl SystemProxyTakeoverState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unmanaged => "未接管",
            Self::ManagedEffective => "接管并且生效",
            Self::ManagedIneffective => "设置为接管但未生效",
        }
    }
}

/// Check whether Windows `ProxyServer` string points to `expected` (e.g. `127.0.0.1:7890`).
pub fn server_matches_expected(actual: &str, expected: &str) -> bool {
    let actual_norm = actual.trim().to_ascii_lowercase();
    let expected_norm = expected.trim().to_ascii_lowercase();
    if actual_norm.is_empty() || expected_norm.is_empty() {
        return false;
    }
    if actual_norm == expected_norm
        || actual_norm == format!("http://{expected_norm}")
        || actual_norm == format!("https://{expected_norm}")
    {
        return true;
    }
    // Handle per-protocol format such as "http=127.0.0.1:7890;https=127.0.0.1:7890"
    let mut saw_http = false;
    for part in actual_norm.split(';') {
        let p = part.trim();
        if let Some(val) = p.strip_prefix("http=") {
            if val == expected_norm {
                saw_http = true;
            } else {
                return false;
            }
        } else if let Some(val) = p.strip_prefix("https=") {
            if val != expected_norm {
                return false;
            }
        }
    }
    saw_http
}

/// Resolve the 3-state system proxy takeover status along with an optional diagnostic reason.
pub fn resolve_takeover_state(
    managed: bool,
    sys_proxy: &SystemProxyStatus,
    mixed_port: u16,
    service_running: bool,
) -> (SystemProxyTakeoverState, Option<&'static str>) {
    if !managed {
        return (SystemProxyTakeoverState::Unmanaged, None);
    }
    let expected = format!("127.0.0.1:{mixed_port}");
    if !sys_proxy.enabled {
        return (
            SystemProxyTakeoverState::ManagedIneffective,
            Some("Windows 系统代理当前已被关闭"),
        );
    }
    if !server_matches_expected(&sys_proxy.server, &expected) {
        return (
            SystemProxyTakeoverState::ManagedIneffective,
            Some("系统代理地址已被外部修改"),
        );
    }
    if !service_running {
        return (
            SystemProxyTakeoverState::ManagedIneffective,
            Some("Mihomo 服务未运行"),
        );
    }
    (SystemProxyTakeoverState::ManagedEffective, None)
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows_sys::Win32::Networking::WinInet::{
        INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED, InternetSetOptionW,
    };
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    const INTERNET_SETTINGS_PATH: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    pub fn get_system_proxy() -> Result<SystemProxyStatus> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(INTERNET_SETTINGS_PATH, KEY_READ)
            .context("打开 Windows Internet Settings 注册表失败")?;

        let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
        let server: String = key.get_value("ProxyServer").unwrap_or_default();
        let bypass: String = key.get_value("ProxyOverride").unwrap_or_default();

        Ok(SystemProxyStatus {
            enabled: enabled != 0,
            server,
            bypass,
        })
    }

    pub fn set_system_proxy(enable: bool, server: &str, bypass: &str) -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(INTERNET_SETTINGS_PATH, KEY_WRITE)
            .context("打开 Windows Internet Settings 注册表写入权限失败")?;

        let flag: u32 = if enable { 1 } else { 0 };
        key.set_value("ProxyEnable", &flag)
            .context("设置 ProxyEnable 失败")?;

        if enable && !server.trim().is_empty() {
            key.set_value("ProxyServer", &server.trim())
                .context("设置 ProxyServer 失败")?;
        }
        if !bypass.trim().is_empty() {
            key.set_value("ProxyOverride", &bypass.trim())
                .context("设置 ProxyOverride 失败")?;
        }

        // Notify Windows & browsers immediately so changes take effect without restart
        unsafe {
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_SETTINGS_CHANGED,
                std::ptr::null_mut(),
                0,
            );
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_REFRESH,
                std::ptr::null_mut(),
                0,
            );
        }

        Ok(())
    }
}

#[cfg(not(windows))]
mod win {
    use super::*;
    pub fn get_system_proxy() -> Result<SystemProxyStatus> {
        Ok(SystemProxyStatus::default())
    }
    pub fn set_system_proxy(_enable: bool, _server: &str, _bypass: &str) -> Result<()> {
        anyhow::bail!("系统代理设置仅支持 Windows")
    }
}

pub use win::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_three_system_proxy_states() {
        let ok_reg = SystemProxyStatus {
            enabled: true,
            server: "127.0.0.1:7890".into(),
            bypass: String::new(),
        };
        let off_reg = SystemProxyStatus {
            enabled: false,
            server: "127.0.0.1:7890".into(),
            bypass: String::new(),
        };
        let other_reg = SystemProxyStatus {
            enabled: true,
            server: "127.0.0.1:10809".into(),
            bypass: String::new(),
        };

        assert_eq!(
            resolve_takeover_state(false, &off_reg, 7890, true).0,
            SystemProxyTakeoverState::Unmanaged
        );
        assert_eq!(
            resolve_takeover_state(true, &ok_reg, 7890, true).0,
            SystemProxyTakeoverState::ManagedEffective
        );
        assert_eq!(
            resolve_takeover_state(true, &off_reg, 7890, true).0,
            SystemProxyTakeoverState::ManagedIneffective
        );
        assert_eq!(
            resolve_takeover_state(true, &other_reg, 7890, true).0,
            SystemProxyTakeoverState::ManagedIneffective
        );
        assert_eq!(
            resolve_takeover_state(true, &ok_reg, 7890, false).0,
            SystemProxyTakeoverState::ManagedIneffective
        );
    }
}
