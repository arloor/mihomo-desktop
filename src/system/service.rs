use anyhow::{Context, Result};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Stopped,
    StartPending,
    StopPending,
    Paused,
    NotInstalled,
    Unknown(u32),
}

impl ServiceState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "运行中",
            Self::Stopped => "已停止",
            Self::StartPending => "正在启动",
            Self::StopPending => "正在停止",
            Self::Paused => "已暂停",
            Self::NotInstalled => "未安装服务",
            Self::Unknown(_) => "未知状态",
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Debug, Clone)]
pub struct ServiceStatusInfo {
    pub service_name: String,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub binary_path: Option<String>,
    pub display_name: Option<String>,
    pub start_type: String,
}

impl Default for ServiceStatusInfo {
    fn default() -> Self {
        Self {
            service_name: "mihomo".to_string(),
            state: ServiceState::Unknown(0),
            pid: None,
            binary_path: None,
            display_name: None,
            start_type: "未知".to_string(),
        }
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST, GetLastError, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW,
        QueryServiceConfigW, QueryServiceStatusEx, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO,
        SERVICE_AUTO_START, SERVICE_CONTROL_STOP, SERVICE_DEMAND_START, SERVICE_DISABLED,
        SERVICE_PAUSED, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START,
        SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOP,
        SERVICE_STOP_PENDING, SERVICE_STOPPED, StartServiceW,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    unsafe fn from_wide_ptr(ptr: *const u16) -> Option<String> {
        if ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
        Some(String::from_utf16_lossy(slice))
    }

    pub fn is_elevated() -> bool {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut ret_len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            );
            CloseHandle(token);
            ok != 0 && elevation.TokenIsElevated != 0
        }
    }

    fn run_elevated_powershell(script: &str) -> Result<()> {
        // Encode script as UTF-16LE Base64 for safe PowerShell -EncodedCommand execution
        let utf16: Vec<u8> = script
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let encoded = base64_encode(&utf16);
        let args = format!("-NoProfile -WindowStyle Hidden -EncodedCommand {encoded}");
        let verb = to_wide("runas");
        let file = to_wide("powershell.exe");
        let params = to_wide(&args);
        let res = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                params.as_ptr(),
                std::ptr::null(),
                SW_HIDE,
            )
        };
        if (res as isize) <= 32 {
            anyhow::bail!("UAC 提权执行被取消或失败 (code={})", res as isize);
        }
        Ok(())
    }

    fn base64_encode(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let triple = (b0 << 16) | (b1 << 8) | b2;
            out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
            out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
            if chunk.len() > 1 {
                out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(TABLE[(triple & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    pub fn query_service(service_name: &str) -> Result<ServiceStatusInfo> {
        let mut info = ServiceStatusInfo {
            service_name: service_name.to_string(),
            ..Default::default()
        };

        unsafe {
            let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
            if scm.is_null() {
                anyhow::bail!("无法打开 Windows 服务管理器 (err={})", GetLastError());
            }

            let name_w = to_wide(service_name);
            let svc = OpenServiceW(scm, name_w.as_ptr(), SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG);
            if svc.is_null() {
                let err = GetLastError();
                CloseServiceHandle(scm);
                if err == ERROR_SERVICE_DOES_NOT_EXIST {
                    info.state = ServiceState::NotInstalled;
                    return Ok(info);
                }
                anyhow::bail!("无法查询服务 '{service_name}' (err={err})");
            }

            // Query process status & PID
            let mut ssp: SERVICE_STATUS_PROCESS = std::mem::zeroed();
            let mut bytes_needed = 0u32;
            if QueryServiceStatusEx(
                svc,
                SC_STATUS_PROCESS_INFO,
                &mut ssp as *mut _ as *mut u8,
                std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut bytes_needed,
            ) != 0
            {
                info.state = match ssp.dwCurrentState {
                    SERVICE_RUNNING => ServiceState::Running,
                    SERVICE_STOPPED => ServiceState::Stopped,
                    SERVICE_START_PENDING => ServiceState::StartPending,
                    SERVICE_STOP_PENDING => ServiceState::StopPending,
                    SERVICE_PAUSED => ServiceState::Paused,
                    other => ServiceState::Unknown(other),
                };
                info.pid = if ssp.dwProcessId > 0 {
                    Some(ssp.dwProcessId)
                } else {
                    None
                };
            }

            // Query binary path & start type
            let mut cfg_needed = 0u32;
            QueryServiceConfigW(svc, std::ptr::null_mut(), 0, &mut cfg_needed);
            if cfg_needed > 0 {
                let mut buf = vec![0u8; cfg_needed as usize];
                let cfg_ptr = buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
                if QueryServiceConfigW(svc, cfg_ptr, cfg_needed, &mut cfg_needed) != 0 {
                    let cfg = &*cfg_ptr;
                    info.binary_path = from_wide_ptr(cfg.lpBinaryPathName)
                        .map(|s| s.trim_matches('"').to_string());
                    info.display_name = from_wide_ptr(cfg.lpDisplayName);
                    info.start_type = match cfg.dwStartType {
                        SERVICE_AUTO_START => "自动".to_string(),
                        SERVICE_DEMAND_START => "手动".to_string(),
                        SERVICE_DISABLED => "已禁用".to_string(),
                        _ => "其他".to_string(),
                    };
                }
            }

            CloseServiceHandle(svc);
            CloseServiceHandle(scm);
        }

        Ok(info)
    }

    fn wait_for_state(service_name: &str, target: ServiceState, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(status) = query_service(service_name) {
                if status.state == target {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let current = query_service(service_name)
            .map(|s| s.state.label())
            .unwrap_or("未知");
        anyhow::bail!(
            "等待服务 '{service_name}' 达到状态 {} 超时 (当前状态: {current})",
            target.label()
        )
    }

    fn try_start_service_direct(service_name: &str) -> std::result::Result<(), u32> {
        unsafe {
            let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
            if scm.is_null() {
                return Err(GetLastError());
            }
            let name_w = to_wide(service_name);
            let svc = OpenServiceW(scm, name_w.as_ptr(), SERVICE_START | SERVICE_QUERY_STATUS);
            if svc.is_null() {
                let err = GetLastError();
                CloseServiceHandle(scm);
                return Err(err);
            }
            let ok = StartServiceW(svc, 0, std::ptr::null());
            let err = if ok == 0 { GetLastError() } else { 0 };
            CloseServiceHandle(svc);
            CloseServiceHandle(scm);
            // ERROR_SERVICE_ALREADY_RUNNING = 1056
            if ok != 0 || err == 1056 {
                Ok(())
            } else {
                Err(err)
            }
        }
    }

    fn try_stop_service_direct(service_name: &str) -> std::result::Result<(), u32> {
        unsafe {
            let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
            if scm.is_null() {
                return Err(GetLastError());
            }
            let name_w = to_wide(service_name);
            let svc = OpenServiceW(scm, name_w.as_ptr(), SERVICE_STOP | SERVICE_QUERY_STATUS);
            if svc.is_null() {
                let err = GetLastError();
                CloseServiceHandle(scm);
                return Err(err);
            }
            let mut status: SERVICE_STATUS = std::mem::zeroed();
            let ok = ControlService(svc, SERVICE_CONTROL_STOP, &mut status);
            let err = if ok == 0 { GetLastError() } else { 0 };
            CloseServiceHandle(svc);
            CloseServiceHandle(scm);
            // ERROR_SERVICE_NOT_ACTIVE = 1062
            if ok != 0 || err == 1062 {
                Ok(())
            } else {
                Err(err)
            }
        }
    }

    pub fn start_service(service_name: &str) -> Result<()> {
        match try_start_service_direct(service_name) {
            Ok(()) => wait_for_state(service_name, ServiceState::Running, Duration::from_secs(15)),
            Err(err) if err == ERROR_ACCESS_DENIED => {
                let ps = format!(
                    "Start-Service -Name '{}'",
                    service_name.replace('\'', "''")
                );
                run_elevated_powershell(&ps)?;
                wait_for_state(service_name, ServiceState::Running, Duration::from_secs(20))
            }
            Err(err) => anyhow::bail!("启动 Windows 服务 '{service_name}' 失败 (Win32 err={err})"),
        }
    }

    pub fn stop_service(service_name: &str) -> Result<()> {
        match try_stop_service_direct(service_name) {
            Ok(()) => wait_for_state(service_name, ServiceState::Stopped, Duration::from_secs(20)),
            Err(err) if err == ERROR_ACCESS_DENIED => {
                let ps = format!(
                    "Stop-Service -Name '{}' -Force",
                    service_name.replace('\'', "''")
                );
                run_elevated_powershell(&ps)?;
                wait_for_state(service_name, ServiceState::Stopped, Duration::from_secs(25))
            }
            Err(err) => anyhow::bail!("停止 Windows 服务 '{service_name}' 失败 (Win32 err={err})"),
        }
    }

    pub fn restart_service(service_name: &str) -> Result<()> {
        match try_stop_service_direct(service_name) {
            Ok(()) => {
                let _ = wait_for_state(service_name, ServiceState::Stopped, Duration::from_secs(20));
            }
            Err(err) if err == ERROR_ACCESS_DENIED => {
                // Execute stop + start in a single UAC prompt instead of prompting twice
                let ps = format!(
                    "Restart-Service -Name '{}' -Force",
                    service_name.replace('\'', "''")
                );
                run_elevated_powershell(&ps)?;
                return wait_for_state(service_name, ServiceState::Running, Duration::from_secs(25));
            }
            Err(err) => {
                anyhow::bail!("停止 Windows 服务 '{service_name}' 失败 (Win32 err={err})");
            }
        }

        match try_start_service_direct(service_name) {
            Ok(()) => wait_for_state(service_name, ServiceState::Running, Duration::from_secs(15)),
            Err(err) if err == ERROR_ACCESS_DENIED => {
                let ps = format!(
                    "Start-Service -Name '{}'",
                    service_name.replace('\'', "''")
                );
                run_elevated_powershell(&ps)?;
                wait_for_state(service_name, ServiceState::Running, Duration::from_secs(20))
            }
            Err(err) => anyhow::bail!("启动 Windows 服务 '{service_name}' 失败 (Win32 err={err})"),
        }
    }

    /// Replace the mihomo core executable managed by the Windows service:
    /// Stop service -> Backup current binary -> Copy new binary -> Start service (if installed).
    pub fn replace_core_and_restart(
        service_name: &str,
        source_exe: &Path,
        target_exe: &Path,
    ) -> Result<()> {
        if !source_exe.exists() {
            anyhow::bail!("源核心文件不存在: {}", source_exe.display());
        }
        let installed = query_service(service_name)
            .map(|s| s.state != ServiceState::NotInstalled)
            .unwrap_or(false);

        if is_elevated() {
            if installed {
                let _ = stop_service(service_name);
                std::thread::sleep(Duration::from_millis(500));
            }
            if target_exe.exists() {
                let backup = target_exe.with_extension("exe.bak");
                let _ = std::fs::copy(target_exe, &backup);
            }
            if let Some(parent) = target_exe.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::copy(source_exe, target_exe).with_context(|| {
                format!(
                    "替换核心文件失败: {} -> {}",
                    source_exe.display(),
                    target_exe.display()
                )
            })?;
            if installed {
                start_service(service_name)?;
            }
            Ok(())
        } else {
            let svc = service_name.replace('\'', "''");
            let src = source_exe.to_string_lossy().replace('\'', "''");
            let dst = target_exe.to_string_lossy().replace('\'', "''");
            let dst_dir = target_exe
                .parent()
                .map(|p| p.to_string_lossy().replace('\'', "''"))
                .unwrap_or_default();
            let bak = target_exe
                .with_extension("exe.bak")
                .to_string_lossy()
                .replace('\'', "''");
            let start_cmd = if installed {
                format!("Start-Service -Name '{svc}'")
            } else {
                String::new()
            };
            let ps = format!(
                "$ErrorActionPreference = 'Stop'; \
                 Stop-Service -Name '{svc}' -Force -ErrorAction SilentlyContinue; \
                 Start-Sleep -Milliseconds 800; \
                 if ('{dst_dir}' -ne '') {{ New-Item -ItemType Directory -Force -Path '{dst_dir}' | Out-Null }}; \
                 if (Test-Path '{dst}') {{ Copy-Item -Force '{dst}' '{bak}' }}; \
                 Copy-Item -Force '{src}' '{dst}'; \
                 {start_cmd}"
            );
            run_elevated_powershell(&ps)?;
            if installed {
                wait_for_state(service_name, ServiceState::Running, Duration::from_secs(30))
            } else {
                std::thread::sleep(Duration::from_millis(1200));
                Ok(())
            }
        }
    }

    /// Install or repair the Mihomo Windows Service using our built-in `--install-service` CLI
    /// (runs directly if already elevated, or elevates in a single UAC prompt).
    pub fn install_service_with_uac(
        service_name: &str,
        core_path: &str,
        work_dir: &str,
        config_path: &str,
    ) -> Result<()> {
        let install_args = crate::system::service_host::ServiceInstallArgs {
            service_name: service_name.to_string(),
            core_path: std::path::PathBuf::from(core_path),
            work_dir: std::path::PathBuf::from(work_dir),
            config_path: std::path::PathBuf::from(config_path),
            bundled_core: None,
            bundled_geo_dir: None,
            start_after_install: true,
        };

        if is_elevated() {
            return crate::system::service_host::install_or_update_service(&install_args);
        }

        let exe = std::env::current_exe().context("获取当前程序路径失败")?;
        let params_str = format!(
            "--install-service --service-name \"{}\" --core \"{}\" --work-dir \"{}\" --config \"{}\"",
            service_name, core_path, work_dir, config_path
        );
        let verb = to_wide("runas");
        let file = to_wide(&exe.to_string_lossy());
        let params = to_wide(&params_str);
        let res = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                params.as_ptr(),
                std::ptr::null(),
                SW_HIDE,
            )
        };
        if (res as isize) <= 32 {
            anyhow::bail!("UAC 提权安装服务被取消或失败 (code={})", res as isize);
        }

        // Wait up to 25s for the service to appear (and start if mihomo.exe exists)
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(25) {
            if let Ok(status) = query_service(service_name) {
                if status.state == ServiceState::Running {
                    return Ok(());
                }
                if status.state == ServiceState::Stopped && !Path::new(core_path).exists() {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        // Return ok if service was at least registered
        let st = query_service(service_name)?;
        if st.state != ServiceState::NotInstalled {
            Ok(())
        } else {
            anyhow::bail!("等待服务安装超时，请检查核心路径或以管理员身份重试")
        }
    }

    /// Uninstall the Mihomo Windows Service (runs directly if elevated, or elevates via UAC).
    pub fn uninstall_service_with_uac(service_name: &str) -> Result<()> {
        if is_elevated() {
            return crate::system::service_host::uninstall_service(service_name);
        }

        let exe = std::env::current_exe().context("获取当前程序路径失败")?;
        let params_str = format!("--uninstall-service --service-name \"{service_name}\"");
        let verb = to_wide("runas");
        let file = to_wide(&exe.to_string_lossy());
        let params = to_wide(&params_str);
        let res = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                params.as_ptr(),
                std::ptr::null(),
                SW_HIDE,
            )
        };
        if (res as isize) <= 32 {
            anyhow::bail!("UAC 提权卸载服务被取消或失败 (code={})", res as isize);
        }
        wait_for_state(service_name, ServiceState::NotInstalled, Duration::from_secs(20))
    }

    /// Write config to a potentially protected path (like ProgramData) with UAC fallback if needed.
    pub fn write_protected_file(target: &Path, content: &str) -> Result<()> {
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(target, content) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied || e.kind() == std::io::ErrorKind::NotFound => {
                let temp = std::env::temp_dir()
                    .join(format!("mihomo-cfg-{}.yaml", uuid::Uuid::new_v4()));
                std::fs::write(&temp, content)?;
                let src = temp.to_string_lossy().replace('\'', "''");
                let dst = target.to_string_lossy().replace('\'', "''");
                let dst_dir = target
                    .parent()
                    .map(|p| p.to_string_lossy().replace('\'', "''"))
                    .unwrap_or_default();
                let ps = format!(
                    "if ('{dst_dir}' -ne '') {{ New-Item -ItemType Directory -Force -Path '{dst_dir}' | Out-Null }}; \
                     Copy-Item -Force '{src}' '{dst}'; \
                     Remove-Item -Force '{src}' -ErrorAction SilentlyContinue"
                );
                run_elevated_powershell(&ps)?;
                std::thread::sleep(Duration::from_millis(800));
                Ok(())
            }
            Err(e) => Err(e).with_context(|| format!("写入文件失败: {}", target.display())),
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::*;
    pub fn is_elevated() -> bool { false }
    pub fn query_service(service_name: &str) -> Result<ServiceStatusInfo> {
        Ok(ServiceStatusInfo {
            service_name: service_name.to_string(),
            state: ServiceState::NotInstalled,
            ..Default::default()
        })
    }
    pub fn start_service(_: &str) -> Result<()> { anyhow::bail!("仅支持 Windows") }
    pub fn stop_service(_: &str) -> Result<()> { anyhow::bail!("仅支持 Windows") }
    pub fn restart_service(_: &str) -> Result<()> { anyhow::bail!("仅支持 Windows") }
    pub fn replace_core_and_restart(_: &str, _: &Path, _: &Path) -> Result<()> {
        anyhow::bail!("仅支持 Windows")
    }
    pub fn install_service_with_uac(_: &str, _: &str, _: &str, _: &str) -> Result<()> {
        anyhow::bail!("仅支持 Windows")
    }
    pub fn uninstall_service_with_uac(_: &str) -> Result<()> {
        anyhow::bail!("仅支持 Windows")
    }
    pub fn write_protected_file(target: &Path, content: &str) -> Result<()> {
        std::fs::write(target, content)?;
        Ok(())
    }
}

pub use win::*;
