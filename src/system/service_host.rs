use crate::config;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Bundled GeoIP / GeoSite / MMDB / ASN database filenames shipped with the installer.
pub const BUNDLED_GEO_FILES: [&str; 4] = [
    "GeoIP.dat",
    "GeoSite.dat",
    "geoip.metadb",
    "ASN.mmdb",
];

#[derive(Debug, Clone)]
pub struct ServiceHostArgs {
    pub service_name: String,
    pub core_path: PathBuf,
    pub work_dir: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ServiceInstallArgs {
    pub service_name: String,
    pub core_path: PathBuf,
    pub work_dir: PathBuf,
    pub config_path: PathBuf,
    pub bundled_core: Option<PathBuf>,
    pub bundled_geo_dir: Option<PathBuf>,
    pub start_after_install: bool,
}

#[derive(Debug, Clone)]
pub enum CliMode {
    Gui,
    RunService(ServiceHostArgs),
    InstallService(ServiceInstallArgs),
    UninstallService { service_name: String },
}

/// Parse CLI arguments before initializing GPUI so `--service`, `--install-service`,
/// and `--uninstall-service` execute cleanly headlessly.
pub fn parse_cli_args() -> CliMode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut service_name = "mihomo".to_string();
    let mut core_path = PathBuf::from(r"C:\Program Files\Mihomo\mihomo.exe");
    let mut work_dir = PathBuf::from(r"C:\ProgramData\Mihomo");
    let mut config_path = PathBuf::from(r"C:\ProgramData\Mihomo\config\clash.yaml");
    let mut bundled_core: Option<PathBuf> = None;
    let mut bundled_geo_dir: Option<PathBuf> = None;
    let mut start_after_install = true;

    let mut mode_run_service = false;
    let mut mode_install = false;
    let mut mode_uninstall = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--service" => mode_run_service = true,
            "--install-service" => mode_install = true,
            "--uninstall-service" => mode_uninstall = true,
            "--no-start" => start_after_install = false,
            "--service-name" => {
                if let Some(v) = args.get(i + 1) {
                    service_name = v.clone();
                    i += 1;
                }
            }
            "--core" => {
                if let Some(v) = args.get(i + 1) {
                    core_path = PathBuf::from(v);
                    i += 1;
                }
            }
            "--work-dir" => {
                if let Some(v) = args.get(i + 1) {
                    work_dir = PathBuf::from(v);
                    i += 1;
                }
            }
            "--config" => {
                if let Some(v) = args.get(i + 1) {
                    config_path = PathBuf::from(v);
                    i += 1;
                }
            }
            "--bundled-core" => {
                if let Some(v) = args.get(i + 1) {
                    bundled_core = Some(PathBuf::from(v));
                    i += 1;
                }
            }
            "--bundled-geo-dir" => {
                if let Some(v) = args.get(i + 1) {
                    bundled_geo_dir = Some(PathBuf::from(v));
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if mode_run_service {
        CliMode::RunService(ServiceHostArgs {
            service_name,
            core_path,
            work_dir,
            config_path,
        })
    } else if mode_install {
        CliMode::InstallService(ServiceInstallArgs {
            service_name,
            core_path,
            work_dir,
            config_path,
            bundled_core,
            bundled_geo_dir,
            start_after_install,
        })
    } else if mode_uninstall {
        CliMode::UninstallService { service_name }
    } else {
        CliMode::Gui
    }
}

/// Locate a bundled `mihomo.exe` candidate next to the current executable or in local cache.
pub fn find_bundled_core_candidate(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for rel in ["mihomo.exe", r"resources\mihomo.exe", r"cores\mihomo.exe"] {
                let candidate = dir.join(rel);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    // Fallback to latest downloaded core in %APPDATA%\mihomo-desktop\cores
    crate::system::core_manager::list_local_cores()
        .into_iter()
        .next()
        .map(|c| c.exe_path)
}

/// Locate a directory containing bundled GEO / MMDB / ASN files next to the executable.
pub fn find_bundled_geo_dir(explicit: Option<&Path>) -> Option<PathBuf> {
    let has_any_geo = |dir: &Path| {
        dir.is_dir()
            && BUNDLED_GEO_FILES
                .iter()
                .any(|name| dir.join(name).is_file())
    };
    if let Some(p) = explicit {
        if has_any_geo(p) {
            return Some(p.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for rel in ["geo", r"resources\geo", "."] {
                let candidate = if rel == "." {
                    dir.to_path_buf()
                } else {
                    dir.join(rel)
                };
                if has_any_geo(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Copy bundled GEO / MMDB / ASN files from `source_dir` into `target_dir`.
/// When `overwrite_existing` is `false`, existing non-empty files in `target_dir` are preserved.
pub fn deploy_bundled_geo_files(
    source_dir: &Path,
    target_dir: &Path,
    overwrite_existing: bool,
) -> Result<usize> {
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("创建 GEO 目标目录失败: {}", target_dir.display()))?;
    let mut copied = 0usize;
    for name in BUNDLED_GEO_FILES {
        let src = source_dir.join(name);
        if !src.is_file() {
            continue;
        }
        let dst = target_dir.join(name);
        let same_file = std::fs::canonicalize(&src).ok() == std::fs::canonicalize(&dst).ok();
        if same_file {
            continue;
        }
        let dst_non_empty = dst
            .metadata()
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false);
        if !overwrite_existing && dst_non_empty {
            continue;
        }
        std::fs::copy(&src, &dst).with_context(|| {
            format!(
                "部署内置 GEO 数据失败: {} -> {}",
                src.display(),
                dst.display()
            )
        })?;
        copied += 1;
    }
    Ok(copied)
}

#[cfg(windows)]
mod win_service {
    use super::*;
    use std::ffi::OsString;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::{OnceLock, mpsc};
    use std::time::{Duration, Instant};
    use windows_service::service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
        ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const MAX_LOG_SIZE_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB

    static SERVICE_ARGS: OnceLock<ServiceHostArgs> = OnceLock::new();

    struct JobObjectGuard(HANDLE);

    impl JobObjectGuard {
        fn create_kill_on_close() -> Option<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    CloseHandle(handle);
                    return None;
                }
                Some(Self(handle))
            }
        }

        fn assign_child(&self, child: &Child) {
            unsafe {
                let _ = AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE);
            }
        }
    }

    impl Drop for JobObjectGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    windows_service::define_windows_service!(ffi_service_main, my_service_main);

    pub fn run_windows_service(args: ServiceHostArgs) -> Result<()> {
        let name = args.service_name.clone();
        let _ = SERVICE_ARGS.set(args);
        service_dispatcher::start(&name, ffi_service_main)
            .with_context(|| format!("启动 Windows Service Dispatcher 失败 (service={name})"))
    }

    fn my_service_main(_arguments: Vec<OsString>) {
        if let Some(args) = SERVICE_ARGS.get().cloned() {
            if let Err(err) = service_loop(args.clone()) {
                append_service_log(&args.work_dir, &format!("Service fatal error: {err:#}"));
            }
        }
    }

    fn rotate_log_if_needed(log_path: &Path) {
        if let Ok(meta) = fs::metadata(log_path) {
            if meta.len() >= MAX_LOG_SIZE_BYTES {
                let backup = log_path.with_extension("log.1");
                let _ = fs::remove_file(&backup);
                let _ = fs::rename(log_path, &backup);
            }
        }
    }

    fn append_service_log(work_dir: &Path, msg: &str) {
        let logs_dir = work_dir.join("logs");
        let _ = fs::create_dir_all(&logs_dir);
        let log_path = logs_dir.join("mihomo-service.log");
        rotate_log_if_needed(&log_path);
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(f, "[{ts}] [mihomo-service] {msg}");
        }
    }

    fn spawn_mihomo_child(args: &ServiceHostArgs, job: Option<&JobObjectGuard>) -> Result<Child> {
        let _ = fs::create_dir_all(&args.work_dir);
        let logs_dir = args.work_dir.join("logs");
        let _ = fs::create_dir_all(&logs_dir);
        let log_path = logs_dir.join("mihomo-core.log");
        rotate_log_if_needed(&log_path);

        let stdout_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();
        let stderr_file = stdout_file.as_ref().and_then(|f| f.try_clone().ok());

        let mut cmd = Command::new(&args.core_path);
        cmd.arg("-d")
            .arg(&args.work_dir)
            .arg("-f")
            .arg(&args.config_path)
            .current_dir(&args.work_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null());

        if let Some(f) = stdout_file {
            cmd.stdout(Stdio::from(f));
        } else {
            cmd.stdout(Stdio::null());
        }
        if let Some(f) = stderr_file {
            cmd.stderr(Stdio::from(f));
        } else {
            cmd.stderr(Stdio::null());
        }

        let child = cmd.spawn().with_context(|| {
            format!(
                "启动核心进程失败: {} -d {} -f {}",
                args.core_path.display(),
                args.work_dir.display(),
                args.config_path.display()
            )
        })?;

        if let Some(j) = job {
            j.assign_child(&child);
        }
        Ok(child)
    }

    fn service_loop(args: ServiceHostArgs) -> Result<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                ServiceControl::Stop | ServiceControl::Shutdown => {
                    let _ = shutdown_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let status_handle =
            service_control_handler::register(&args.service_name, event_handler)?;

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 1,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })?;

        // Ensure work directory, bundled GEO/MMDB/ASN files & minimal config exist before starting mihomo
        if let Some(geo_dir) = find_bundled_geo_dir(None) {
            match deploy_bundled_geo_files(&geo_dir, &args.work_dir, false) {
                Ok(n) if n > 0 => append_service_log(
                    &args.work_dir,
                    &format!("Restored {n} missing bundled GEO/MMDB/ASN file(s) from {}", geo_dir.display()),
                ),
                Err(e) => append_service_log(
                    &args.work_dir,
                    &format!("Warning: failed to deploy bundled GEO files: {e:#}"),
                ),
                _ => {}
            }
        }
        if !args.config_path.exists() {
            if let Some(parent) = args.config_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&args.config_path, config::minimal_initial_config_yaml());
        }

        let job = JobObjectGuard::create_kill_on_close();
        append_service_log(
            &args.work_dir,
            &format!(
                "Service starting: core={}, work_dir={}, config={}",
                args.core_path.display(),
                args.work_dir.display(),
                args.config_path.display()
            ),
        );

        let mut child = match spawn_mihomo_child(&args, job.as_ref()) {
            Ok(c) => {
                append_service_log(&args.work_dir, &format!("mihomo spawned (PID={})", c.id()));
                Some(c)
            }
            Err(e) => {
                append_service_log(&args.work_dir, &format!("Initial spawn failed: {e:#}"));
                None
            }
        };

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        // Supervisor loop: watch child health & listen for SCM stop/shutdown
        loop {
            match shutdown_rx.recv_timeout(Duration::from_millis(500)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    append_service_log(&args.work_dir, "Stop/Shutdown signal received");
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }

            let needs_restart = match child.as_mut() {
                Some(c) => match c.try_wait() {
                    Ok(Some(status)) => {
                        append_service_log(
                            &args.work_dir,
                            &format!("mihomo exited unexpectedly with {status}; restarting in 5s"),
                        );
                        child = None;
                        true
                    }
                    Ok(None) => false,
                    Err(e) => {
                        append_service_log(&args.work_dir, &format!("try_wait error: {e}"));
                        false
                    }
                },
                None => true,
            };

            if needs_restart {
                // Sleep up to 5s, waking immediately if SCM sends stop
                if shutdown_rx.recv_timeout(Duration::from_secs(5)).is_ok() {
                    break;
                }
                match spawn_mihomo_child(&args, job.as_ref()) {
                    Ok(c) => {
                        append_service_log(
                            &args.work_dir,
                            &format!("mihomo restarted (PID={})", c.id()),
                        );
                        child = Some(c);
                    }
                    Err(e) => {
                        append_service_log(&args.work_dir, &format!("Restart failed: {e:#}"));
                    }
                }
            }
        }

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StopPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 1,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })?;

        if let Some(mut c) = child {
            let _ = c.kill();
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                if matches!(c.try_wait(), Ok(Some(_))) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        // Dropping `job` ensures any grandchild processes are terminated too
        drop(job);
        append_service_log(&args.work_dir, "Service stopped cleanly");

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }

    /// Install or upgrade the Mihomo Windows Service (must run elevated).
    /// Also sets Service DACL + ProgramData directory ACL so the normal desktop app
    /// can start/stop/restart the service and update configs with ZERO UAC prompts.
    pub fn install_or_update_service(args: &ServiceInstallArgs) -> Result<()> {
        let core_dir = args
            .core_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Mihomo"));
        fs::create_dir_all(&core_dir)
            .with_context(|| format!("创建核心目录失败: {}", core_dir.display()))?;
        fs::create_dir_all(&args.work_dir)
            .with_context(|| format!("创建工作目录失败: {}", args.work_dir.display()))?;
        fs::create_dir_all(args.work_dir.join("logs"))?;
        if let Some(cfg_parent) = args.config_path.parent() {
            fs::create_dir_all(cfg_parent)
                .with_context(|| format!("创建配置目录失败: {}", cfg_parent.display()))?;
        }

        // Stop existing service before updating binaries/config
        let _ = crate::system::service::stop_service(&args.service_name);
        std::thread::sleep(Duration::from_millis(500));

        // Copy current desktop binary into stable install directory for service hosting
        let current_exe = std::env::current_exe().context("获取当前程序路径失败")?;
        let host_exe = core_dir.join("mihomo-desktop.exe");
        let same_path = fs::canonicalize(&current_exe).ok() == fs::canonicalize(&host_exe).ok();
        if !same_path {
            fs::copy(&current_exe, &host_exe).with_context(|| {
                format!(
                    "复制服务托管程序失败: {} -> {}",
                    current_exe.display(),
                    host_exe.display()
                )
            })?;
        }

        // Ensure mihomo.exe exists (copy bundled core if missing or if explicitly provided)
        if !args.core_path.exists() {
            if let Some(bundled) = find_bundled_core_candidate(args.bundled_core.as_deref()) {
                let same_core =
                    fs::canonicalize(&bundled).ok() == fs::canonicalize(&args.core_path).ok();
                if !same_core {
                    fs::copy(&bundled, &args.core_path).with_context(|| {
                        format!(
                            "部署内置核心失败: {} -> {}",
                            bundled.display(),
                            args.core_path.display()
                        )
                    })?;
                }
            }
        } else if let Some(explicit) = args.bundled_core.as_deref() {
            if explicit.exists()
                && fs::canonicalize(explicit).ok() != fs::canonicalize(&args.core_path).ok()
            {
                let _ = fs::copy(explicit, &args.core_path);
            }
        }

        // Deploy bundled GEO / MMDB / ASN databases:
        // 1) Mirror into `<core_dir>\geo` so future service restarts / repairs keep an offline copy.
        // 2) Copy missing (or empty) files into `work_dir` without overwriting existing user/auto-updated databases.
        if let Some(geo_src) = find_bundled_geo_dir(args.bundled_geo_dir.as_deref()) {
            let installed_geo_dir = core_dir.join("geo");
            let _ = deploy_bundled_geo_files(&geo_src, &installed_geo_dir, true);
            deploy_bundled_geo_files(&geo_src, &args.work_dir, false)?;
        }

        // Write minimal runnable clash.yaml if not already present (preserves existing configs!)
        if !args.config_path.exists() {
            fs::write(&args.config_path, config::minimal_initial_config_yaml())
                .with_context(|| format!("写入初始配置失败: {}", args.config_path.display()))?;
        }

        // Register or update Windows Service via SCM
        let manager = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
        )
        .context("打开 Windows 服务管理器失败（请以管理员权限运行）")?;

        let launch_args = vec![
            OsString::from("--service"),
            OsString::from("--service-name"),
            OsString::from(&args.service_name),
            OsString::from("--core"),
            args.core_path.as_os_str().to_os_string(),
            OsString::from("--work-dir"),
            args.work_dir.as_os_str().to_os_string(),
            OsString::from("--config"),
            args.config_path.as_os_str().to_os_string(),
        ];

        let service_info = ServiceInfo {
            name: OsString::from(&args.service_name),
            display_name: OsString::from("Mihomo Proxy Service"),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: host_exe,
            launch_arguments: launch_args,
            dependencies: vec![],
            account_name: None, // LocalSystem
            account_password: None,
        };

        let service = match manager.open_service(
            &args.service_name,
            ServiceAccess::CHANGE_CONFIG | ServiceAccess::START | ServiceAccess::QUERY_STATUS,
        ) {
            Ok(existing) => {
                existing
                    .change_config(&service_info)
                    .context("更新现有 Windows 服务配置失败")?;
                existing
            }
            Err(_) => manager
                .create_service(
                    &service_info,
                    ServiceAccess::CHANGE_CONFIG
                        | ServiceAccess::START
                        | ServiceAccess::QUERY_STATUS,
                )
                .context("创建 Windows 服务失败")?,
        };

        let _ = service.set_description("Mihomo proxy service managed by Mihomo Desktop");
        let _ = service.update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86400)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(5),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(5),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(10),
                },
            ]),
        });

        // Grant Authenticated Users (AU) start/stop/pause/query permission on this service
        // so the non-admin desktop app can control lifecycle with ZERO UAC prompts!
        let sddl = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)(A;;CCLCSWRPWPLOCRRC;;;AU)";
        let _ = Command::new("sc.exe")
            .args(["sdset", &args.service_name, sddl])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        // Grant Built-in Users (SID *S-1-5-32-545) Modify access on work_dir (e.g. C:\ProgramData\Mihomo)
        // so the non-admin desktop app can write merged clash.yaml & hot-reload with ZERO UAC prompts!
        let _ = Command::new("icacls.exe")
            .arg(&args.work_dir)
            .args(["/grant", "*S-1-5-32-545:(OI)(CI)M", "/T", "/Q"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        if args.start_after_install && args.core_path.exists() {
            crate::system::service::start_service(&args.service_name)?;
        }

        Ok(())
    }

    /// Stop and remove the Mihomo Windows Service (must run elevated).
    pub fn uninstall_service(service_name: &str) -> Result<()> {
        let _ = crate::system::service::stop_service(service_name);
        std::thread::sleep(Duration::from_millis(500));

        let manager =
            ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
                .context("打开 Windows 服务管理器失败")?;
        let service = match manager.open_service(service_name, ServiceAccess::DELETE) {
            Ok(s) => s,
            Err(_) => return Ok(()), // Already removed
        };
        service.delete().context("删除 Windows 服务失败")?;
        Ok(())
    }
}

#[cfg(not(windows))]
mod win_service {
    use super::*;
    pub fn run_windows_service(_: ServiceHostArgs) -> Result<()> {
        anyhow::bail!("Windows Service 模式仅支持 Windows")
    }
    pub fn install_or_update_service(_: &ServiceInstallArgs) -> Result<()> {
        anyhow::bail!("Windows Service 安装仅支持 Windows")
    }
    pub fn uninstall_service(_: &str) -> Result<()> {
        anyhow::bail!("Windows Service 卸载仅支持 Windows")
    }
}

pub use win_service::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploys_missing_geo_files_without_overwriting_existing() {
        let base = std::env::temp_dir().join(format!("mihomo-geo-test-{}", uuid::Uuid::new_v4()));
        let src = base.join("src");
        let dst = base.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();

        for name in BUNDLED_GEO_FILES {
            std::fs::write(src.join(name), format!("bundled-{name}")).unwrap();
        }
        // Pre-populate one non-empty file and one 0-byte broken file in target
        std::fs::write(dst.join("GeoSite.dat"), "user-updated-geosite").unwrap();
        std::fs::write(dst.join("ASN.mmdb"), "").unwrap();

        let copied = deploy_bundled_geo_files(&src, &dst, false).unwrap();
        assert_eq!(copied, 3);
        assert_eq!(
            std::fs::read_to_string(dst.join("GeoSite.dat")).unwrap(),
            "user-updated-geosite"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("ASN.mmdb")).unwrap(),
            "bundled-ASN.mmdb"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("GeoIP.dat")).unwrap(),
            "bundled-GeoIP.dat"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("geoip.metadb")).unwrap(),
            "bundled-geoip.metadb"
        );

        // With overwrite_existing = true, all 4 files are updated
        let overwritten = deploy_bundled_geo_files(&src, &dst, true).unwrap();
        assert_eq!(overwritten, 4);
        assert_eq!(
            std::fs::read_to_string(dst.join("GeoSite.dat")).unwrap(),
            "bundled-GeoSite.dat"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
