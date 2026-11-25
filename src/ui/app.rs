use crate::config::{
    self, AppSettings, Profile, ProfileStore, format_bytes, format_speed,
};
use crate::mihomo::{
    ConnectionsResponse, DnsQueryResponse, MihomoClient, MihomoConfig, ProxyItem, ProxyProvider,
    RuleItem, RuleProvider, VersionInfo,
};
use crate::runtime::{LiveTelemetry, spawn_telemetry_workers, tokio_rt};
use crate::system::{
    autostart,
    core_manager::{self, GitHubAsset, GitHubRelease, LocalCoreVersion},
    proxy::{self, SystemProxyStatus, SystemProxyTakeoverState},
    service::{self, ServiceStatusInfo},
    tray::{self, TrayCommand, TrayController, TrayState},
};
use crate::ui::{AppPage, ConnSort, ProvidersTab, ProxySort, ToolsTab, pill_badge};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _, Theme, ThemeMode, TitleBar,
    WindowExt as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[derive(Debug)]
pub enum UiEvent {
    SnapshotRefreshed {
        service: ServiceStatusInfo,
        sys_proxy: SystemProxyStatus,
        api_online: bool,
        api_error: Option<String>,
        version: Option<VersionInfo>,
        config: Option<MihomoConfig>,
        proxies: Option<HashMap<String, ProxyItem>>,
        proxy_providers: Option<HashMap<String, ProxyProvider>>,
        rule_providers: Option<HashMap<String, RuleProvider>>,
        rules: Option<Vec<RuleItem>>,
        connections: Option<ConnectionsResponse>,
    },
    ConnectionsRefreshed(ConnectionsResponse),
    DelayTested {
        proxy_name: String,
        delay: Result<u32, String>,
    },
    GroupDelayTested {
        group: String,
        delays: Result<HashMap<String, u32>, String>,
    },
    ProfileImported(Result<Profile, String>),
    ProfileUpdated(Result<Profile, String>),
    ReleasesFetched(Result<Vec<GitHubRelease>, String>),
    CoreDownloaded(Result<LocalCoreVersion, String>),
    DnsQueried(Result<DnsQueryResponse, String>),
    ActionFinished {
        label: String,
        result: Result<String, String>,
        refresh_after: bool,
    },
}

/// Stable notification ID for non-error status toasts so rapid toggles
/// (e.g. system proxy / proxy mode) replace the previous toast cleanly.
struct StatusNoticeId;

pub struct MihomoApp {
    pub page: AppPage,
    pub settings: AppSettings,
    pub profiles: ProfileStore,
    pub service_status: ServiceStatusInfo,
    pub system_proxy: SystemProxyStatus,

    // Mihomo live API snapshot
    pub api_online: bool,
    pub api_error: Option<String>,
    pub version: Option<VersionInfo>,
    pub mihomo_config: Option<MihomoConfig>,
    pub proxies: HashMap<String, ProxyItem>,
    pub proxy_delays: HashMap<String, u32>,
    pub proxy_providers: HashMap<String, ProxyProvider>,
    pub rule_providers: HashMap<String, RuleProvider>,
    pub rules: Vec<RuleItem>,
    pub connections: ConnectionsResponse,

    // Live WebSocket telemetry
    pub telemetry: Arc<Mutex<LiveTelemetry>>,

    // Sub-view state
    pub selected_group: Option<String>,
    pub proxy_sort: ProxySort,
    pub providers_tab: ProvidersTab,
    pub conn_sort: ConnSort,
    pub connections_paused: bool,
    pub tools_tab: ToolsTab,
    pub dns_record_type: String,
    pub dns_result: Option<DnsQueryResponse>,
    pub preview_yaml: Option<(String, String)>,

    // Cores & GitHub releases
    pub github_releases: Vec<GitHubRelease>,
    pub local_cores: Vec<LocalCoreVersion>,
    pub expanded_release_tag: Option<String>,

    // Status feedback
    pub busy_action: Option<String>,
    pending_notices: Vec<(bool, String)>,

    // Text input states
    pub profile_name_input: Entity<InputState>,
    pub profile_url_input: Entity<InputState>,
    pub local_import_input: Entity<InputState>,
    pub proxy_search_input: Entity<InputState>,
    pub rule_search_input: Entity<InputState>,
    pub conn_search_input: Entity<InputState>,
    pub log_search_input: Entity<InputState>,
    pub dns_domain_input: Entity<InputState>,

    // Settings inputs
    pub setting_service_name: Entity<InputState>,
    pub setting_core_path: Entity<InputState>,
    pub setting_work_dir: Entity<InputState>,
    pub setting_config_path: Entity<InputState>,
    pub setting_controller_addr: Entity<InputState>,
    pub setting_controller_secret: Entity<InputState>,
    pub setting_mixed_port: Entity<InputState>,
    pub setting_proxy_bypass: Entity<InputState>,
    pub setting_test_url: Entity<InputState>,

    // Channel for background Tokio tasks -> UI entity
    pub tx: mpsc::Sender<UiEvent>,
    rx: Arc<Mutex<mpsc::Receiver<UiEvent>>>,

    // Resident mode & System Tray integration
    tray: TrayController,
    tray_rx: Arc<Mutex<mpsc::Receiver<TrayCommand>>>,
    resident_flag: Arc<AtomicBool>,
    force_quit_flag: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
}

impl MihomoApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut settings = config::load_settings();
        let mut profiles = config::load_profiles();

        // Sync initial theme from settings
        Theme::change(
            if settings.dark_mode {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );

        // Sync Windows auto-start registry strictly with resident_mode on startup.
        let _ = autostart::sync_autostart(settings.resident_mode);

        // If user has no managed profiles yet and existing service config exists, import it automatically
        if profiles.profiles.is_empty() && Path::new(&settings.active_config_path).exists() {
            if let Ok(p) = config::import_local_profile(
                Some("本机服务当前配置".to_string()),
                Path::new(&settings.active_config_path),
            ) {
                settings.active_profile_id = Some(p.id.clone());
                profiles.profiles.push(p);
                let _ = config::save_profiles(&profiles);
                let _ = config::save_settings(&settings);
            }
        }

        let service_status = service::query_service(&settings.service_name).unwrap_or_default();
        let system_proxy = proxy::get_system_proxy().unwrap_or_default();
        let local_cores = core_manager::list_local_cores();

        let (initial_proxy_state, initial_proxy_reason) = proxy::resolve_takeover_state(
            settings.system_proxy_managed,
            &system_proxy,
            settings.mixed_port,
            service_status.state.is_running(),
        );

        let (tray_tx, tray_rx) = mpsc::channel();
        let tray_rx = Arc::new(Mutex::new(tray_rx));
        let initial_hwnd = tray::find_current_process_main_hwnd().unwrap_or(0);
        if initial_hwnd != 0 {
            tray::apply_window_icon(initial_hwnd);
        }
        let main_hwnd = Arc::new(AtomicIsize::new(initial_hwnd));
        let resident_flag = Arc::new(AtomicBool::new(settings.resident_mode));
        let force_quit_flag = Arc::new(AtomicBool::new(false));

        let tray = TrayController::start(
            TrayState {
                visible: settings.resident_mode,
                proxy_state: initial_proxy_state,
                proxy_reason: initial_proxy_reason.map(str::to_string),
                proxy_server: format!("127.0.0.1:{}", settings.mixed_port),
                tun_enabled: settings.tun_enabled,
                mode: "rule".to_string(),
                service_running: service_status.state.is_running(),
                main_hwnd: initial_hwnd,
            },
            tray_tx,
        );

        // Intercept window close: in resident mode, hide to system tray instead of exiting.
        {
            let resident_close = Arc::clone(&resident_flag);
            let force_quit_close = Arc::clone(&force_quit_flag);
            let hwnd_close = Arc::clone(&main_hwnd);
            let tray_close = tray.clone();
            window.on_window_should_close(cx, move |_window, _cx| {
                if resident_close.load(Ordering::SeqCst)
                    && !force_quit_close.load(Ordering::SeqCst)
                {
                    let mut h = hwnd_close.load(Ordering::SeqCst);
                    if h == 0 {
                        if let Some(found) = tray::find_current_process_main_hwnd() {
                            h = found;
                            hwnd_close.store(found, Ordering::SeqCst);
                            tray_close.update(|s| s.main_hwnd = found);
                        }
                    }
                    tray::hide_main_window(h);
                    false
                } else {
                    tray_close.shutdown();
                    true
                }
            });
        }

        let telemetry = Arc::new(Mutex::new(LiveTelemetry::new(
            settings.controller_addr.clone(),
            settings.controller_secret.clone(),
        )));
        spawn_telemetry_workers(Arc::clone(&telemetry));

        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));

        let profile_name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("配置名称（可选，留空自动识别）")
        });
        let profile_url_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("https://example.com/clash.yaml")
        });
        let local_import_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"输入本地 YAML 文件路径，例如 C:\ProgramData\Mihomo\config\clash.yaml")
        });
        let proxy_search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索节点名称..."));
        let rule_search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索规则类型 / 域名 / 目标策略..."));
        let conn_search_input = cx.new(|cx| InputState::new(window, cx).placeholder("搜索域名、IP、进程、规则或链路..."));
        let log_search_input = cx.new(|cx| InputState::new(window, cx).placeholder("筛选日志关键字..."));
        let dns_domain_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入要查询的域名，例如 www.google.com")
                .default_value("www.google.com")
        });

        let setting_service_name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("mihomo")
                .default_value(settings.service_name.clone())
        });
        let setting_core_path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"C:\Program Files\Mihomo\mihomo.exe")
                .default_value(settings.core_binary_path.clone())
        });
        let setting_work_dir = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"C:\ProgramData\Mihomo")
                .default_value(settings.work_dir.clone())
        });
        let setting_config_path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"C:\ProgramData\Mihomo\config\clash.yaml")
                .default_value(settings.active_config_path.clone())
        });
        let setting_controller_addr = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("127.0.0.1:9090")
                .default_value(settings.controller_addr.clone())
        });
        let setting_controller_secret = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("若无密钥可留空")
                .default_value(settings.controller_secret.clone())
        });
        let setting_mixed_port = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("7890")
                .default_value(settings.mixed_port.to_string())
        });
        let setting_proxy_bypass = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("localhost;127.*;<local>")
                .default_value(settings.proxy_bypass.clone())
        });
        let setting_test_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://www.gstatic.com/generate_204")
                .default_value(settings.delay_test_url.clone())
        });

        let app = Self {
            page: AppPage::Dashboard,
            settings,
            profiles,
            service_status,
            system_proxy,
            api_online: false,
            api_error: None,
            version: None,
            mihomo_config: None,
            proxies: HashMap::new(),
            proxy_delays: HashMap::new(),
            proxy_providers: HashMap::new(),
            rule_providers: HashMap::new(),
            rules: Vec::new(),
            connections: ConnectionsResponse::default(),
            telemetry,
            selected_group: None,
            proxy_sort: ProxySort::Default,
            providers_tab: ProvidersTab::ProxyProviders,
            conn_sort: ConnSort::DownloadDesc,
            connections_paused: false,
            tools_tab: ToolsTab::Logs,
            dns_record_type: "A".to_string(),
            dns_result: None,
            preview_yaml: None,
            github_releases: Vec::new(),
            local_cores,
            expanded_release_tag: None,
            busy_action: None,
            pending_notices: Vec::new(),
            profile_name_input,
            profile_url_input,
            local_import_input,
            proxy_search_input,
            rule_search_input,
            conn_search_input,
            log_search_input,
            dns_domain_input,
            setting_service_name,
            setting_core_path,
            setting_work_dir,
            setting_config_path,
            setting_controller_addr,
            setting_controller_secret,
            setting_mixed_port,
            setting_proxy_bypass,
            setting_test_url,
            tx,
            rx,
            tray,
            tray_rx,
            resident_flag,
            force_quit_flag,
            main_hwnd,
        };

        app.trigger_full_refresh();
        app.start_ui_loops(window, cx);
        app
    }

    fn start_ui_loops(&self, window: &mut Window, cx: &mut Context<Self>) {
        // Fast UI drain loop (200ms) for incoming channel events, tray commands, and live WebSocket telemetry redraws
        cx.spawn_in(window, async move |this, cx| {
            let mut tick: u64 = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                tick = tick.wrapping_add(1);

                let ok = this.update_in(cx, |app, window, cx| {
                    let mut changed = false;

                    // Ensure main HWND is cached once the Win32 window exists
                    if app.main_hwnd.load(Ordering::SeqCst) == 0 {
                        if let Some(found) = tray::find_current_process_main_hwnd() {
                            app.main_hwnd.store(found, Ordering::SeqCst);
                            tray::apply_window_icon(found);
                            app.tray.update(|s| s.main_hwnd = found);
                        }
                    }

                    // Drain tray commands
                    let tray_cmds: Vec<TrayCommand> = {
                        let rx = app.tray_rx.lock().unwrap();
                        let mut buf = Vec::new();
                        while let Ok(cmd) = rx.try_recv() {
                            buf.push(cmd);
                        }
                        buf
                    };
                    for cmd in tray_cmds {
                        app.handle_tray_command(cmd, window, cx);
                        changed = true;
                    }

                    // Drain all pending background results
                    let events: Vec<UiEvent> = {
                        let rx = app.rx.lock().unwrap();
                        let mut buf = Vec::new();
                        while let Ok(ev) = rx.try_recv() {
                            buf.push(ev);
                        }
                        buf
                    };
                    for ev in events {
                        app.handle_event(ev);
                        changed = true;
                    }

                    // Every 1s (5 ticks): redraw telemetry pages and refresh connections if active
                    if tick % 5 == 0 {
                        if matches!(app.page, AppPage::Dashboard | AppPage::Tools) {
                            changed = true;
                        }
                        if app.page == AppPage::Connections && !app.connections_paused && app.api_online {
                            app.trigger_connections_refresh();
                        }
                    }

                    // Every 5s (25 ticks): refresh Windows service status + core snapshot + tray proxy status
                    if tick % 25 == 0 {
                        app.trigger_full_refresh();
                    }

                    if !app.pending_notices.is_empty() {
                        app.flush_pending_notices(window, cx);
                    }

                    if changed {
                        cx.notify();
                    }
                });

                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_tray_command(&mut self, cmd: TrayCommand, window: &mut Window, cx: &mut Context<Self>) {
        match cmd {
            TrayCommand::ShowWindow => {
                let h = self.main_hwnd.load(Ordering::SeqCst);
                tray::show_and_focus_main_window(h);
                window.activate_window();
                self.trigger_full_refresh();
            }
            TrayCommand::EnableSystemProxy => {
                self.toggle_system_proxy(true);
            }
            TrayCommand::DisableSystemProxy => {
                self.toggle_system_proxy(false);
            }
            TrayCommand::ReapplySystemProxy => {
                self.reapply_system_proxy();
            }
            TrayCommand::ToggleTun => {
                self.toggle_tun_mode(!self.settings.tun_enabled);
            }
            TrayCommand::SetMode(mode) => {
                self.set_proxy_mode(&mode);
            }
            TrayCommand::RestartService => {
                self.restart_windows_service();
            }
            TrayCommand::Quit => {
                self.force_quit_flag.store(true, Ordering::SeqCst);
                self.tray.shutdown();
                cx.quit();
            }
        }
    }

    pub fn system_proxy_takeover_state(&self) -> (SystemProxyTakeoverState, Option<&'static str>) {
        proxy::resolve_takeover_state(
            self.settings.system_proxy_managed,
            &self.system_proxy,
            self.settings.mixed_port,
            self.service_status.state.is_running(),
        )
    }

    pub fn sync_tray_state(&self) {
        let (proxy_state, proxy_reason) = self.system_proxy_takeover_state();
        let mode = self
            .mihomo_config
            .as_ref()
            .map(|c| c.mode.clone())
            .unwrap_or_else(|| "rule".to_string());
        let visible = self.settings.resident_mode;
        let server = format!("127.0.0.1:{}", self.settings.mixed_port);
        let tun_enabled = self.settings.tun_enabled;
        let service_running = self.service_status.state.is_running();
        let hwnd = self.main_hwnd.load(Ordering::SeqCst);

        self.tray.update(|s| {
            s.visible = visible;
            s.proxy_state = proxy_state;
            s.proxy_reason = proxy_reason.map(str::to_string);
            s.proxy_server = server;
            s.tun_enabled = tun_enabled;
            s.mode = mode;
            s.service_running = service_running;
            if hwnd != 0 {
                s.main_hwnd = hwnd;
            }
        });
    }

    fn handle_event(&mut self, ev: UiEvent) {
        match ev {
            UiEvent::SnapshotRefreshed {
                service,
                sys_proxy,
                api_online,
                api_error,
                version,
                config,
                proxies,
                proxy_providers,
                rule_providers,
                rules,
                connections,
            } => {
                self.service_status = service;
                self.system_proxy = sys_proxy;
                self.api_online = api_online;
                self.api_error = api_error;
                if version.is_some() {
                    self.version = version;
                }
                if let Some(cfg) = config {
                    if let Some(tun) = &cfg.tun {
                        self.settings.tun_enabled = tun.enable;
                    }
                    if cfg.mixed_port > 0 {
                        self.settings.mixed_port = cfg.mixed_port;
                    } else if cfg.port > 0 {
                        self.settings.mixed_port = cfg.port;
                    }
                    self.mihomo_config = Some(cfg);
                }
                if let Some(p) = proxies {
                    for (name, item) in &p {
                        if let Some(d) = item.latest_delay() {
                            self.proxy_delays.entry(name.clone()).or_insert(d);
                        }
                    }
                    self.proxies = p;
                    let groups = self.sorted_proxy_groups();
                    if self
                        .selected_group
                        .as_ref()
                        .is_none_or(|g| !groups.contains(g))
                    {
                        self.selected_group = groups.first().cloned();
                    }
                }
                if let Some(pp) = proxy_providers {
                    self.proxy_providers = pp;
                }
                if let Some(rp) = rule_providers {
                    self.rule_providers = rp;
                }
                if let Some(r) = rules {
                    self.rules = r;
                }
                if let Some(c) = connections {
                    if !self.connections_paused {
                        self.connections = c;
                    }
                }
                self.sync_tray_state();
            }
            UiEvent::ConnectionsRefreshed(c) => {
                if !self.connections_paused {
                    self.connections = c;
                }
            }
            UiEvent::DelayTested { proxy_name, delay } => match delay {
                Ok(ms) => {
                    self.proxy_delays.insert(proxy_name, ms);
                }
                Err(_) => {
                    self.proxy_delays.insert(proxy_name, 0);
                }
            },
            UiEvent::GroupDelayTested { group, delays } => {
                self.busy_action = None;
                match delays {
                    Ok(map) => {
                        let count = map.len();
                        for (k, v) in map {
                            self.proxy_delays.insert(k, v);
                        }
                        self.set_notice(false, format!("策略组「{group}」测速完成（{count} 个节点响应）"));
                    }
                    Err(e) => {
                        self.set_notice(true, format!("策略组「{group}」测速失败: {e}"));
                    }
                }
            }
            UiEvent::ProfileImported(res) => {
                self.busy_action = None;
                match res {
                    Ok(profile) => {
                        let name = profile.name.clone();
                        let id = profile.id.clone();
                        self.profiles.profiles.push(profile);
                        let _ = config::save_profiles(&self.profiles);
                        self.set_notice(false, format!("已成功导入配置「{name}」"));
                        if self.settings.active_profile_id.is_none() {
                            self.activate_profile(&id);
                        }
                    }
                    Err(e) => self.set_notice(true, format!("导入配置失败: {e}")),
                }
            }
            UiEvent::ProfileUpdated(res) => {
                self.busy_action = None;
                match res {
                    Ok(updated) => {
                        let name = updated.name.clone();
                        let id = updated.id.clone();
                        if let Some(slot) = self
                            .profiles
                            .profiles
                            .iter_mut()
                            .find(|p| p.id == updated.id)
                        {
                            *slot = updated;
                        }
                        let _ = config::save_profiles(&self.profiles);
                        if self.settings.active_profile_id.as_deref() == Some(id.as_str()) {
                            self.activate_profile(&id);
                        } else {
                            self.set_notice(false, format!("配置「{name}」已更新"));
                        }
                    }
                    Err(e) => self.set_notice(true, format!("更新配置失败: {e}")),
                }
            }
            UiEvent::ReleasesFetched(res) => {
                self.busy_action = None;
                match res {
                    Ok(list) => {
                        let count = list.len();
                        self.expanded_release_tag = list.first().map(|r| r.tag_name.clone());
                        self.github_releases = list;
                        self.set_notice(false, format!("已从 GitHub 拉取 {count} 个 Mihomo Release"));
                    }
                    Err(e) => self.set_notice(true, format!("拉取 Release 失败: {e}")),
                }
            }
            UiEvent::CoreDownloaded(res) => {
                self.busy_action = None;
                match res {
                    Ok(local) => {
                        let tag = local.tag.clone();
                        self.local_cores = core_manager::list_local_cores();
                        self.set_notice(false, format!("核心「{tag}」已下载解压完毕，可点击切换生效"));
                    }
                    Err(e) => self.set_notice(true, format!("下载核心失败: {e}")),
                }
            }
            UiEvent::DnsQueried(res) => {
                self.busy_action = None;
                match res {
                    Ok(resp) => {
                        self.dns_result = Some(resp);
                    }
                    Err(e) => self.set_notice(true, format!("DNS 查询失败: {e}")),
                }
            }
            UiEvent::ActionFinished {
                label,
                result,
                refresh_after,
            } => {
                self.busy_action = None;
                match result {
                    Ok(msg) => self.set_notice(false, if msg.is_empty() { format!("{label} 成功") } else { msg }),
                    Err(e) => self.set_notice(true, format!("{label} 失败: {e}")),
                }
                if refresh_after {
                    self.trigger_full_refresh();
                }
            }
        }
    }

    pub fn set_notice(&mut self, is_error: bool, msg: impl Into<String>) {
        self.pending_notices.push((is_error, msg.into()));
    }

    fn flush_pending_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_notices.is_empty() {
            return;
        }
        let notices = std::mem::take(&mut self.pending_notices);
        window.defer(cx, move |window, cx| {
            for (is_error, msg) in notices {
                let note = if is_error {
                    Notification::error(msg)
                        .placement(Anchor::BottomRight)
                        .autohide(false)
                } else {
                    Notification::success(msg)
                        .id::<StatusNoticeId>()
                        .placement(Anchor::BottomRight)
                        .autohide(true)
                };
                window.push_notification(note, cx);
            }
        });
    }

    pub fn local_http_proxy_url(&self) -> Option<String> {
        if self.api_online && self.settings.mixed_port > 0 {
            Some(format!("http://127.0.0.1:{}", self.settings.mixed_port))
        } else {
            None
        }
    }

    pub fn client(&self) -> MihomoClient {
        MihomoClient::new(&self.settings.controller_addr, &self.settings.controller_secret)
    }

    pub fn sorted_proxy_groups(&self) -> Vec<String> {
        // Respect GLOBAL ordering if present, otherwise alphabetical
        let mut order: Vec<String> = Vec::new();
        if let Some(global) = self.proxies.get("GLOBAL") {
            if let Some(all) = &global.all {
                for name in all {
                    if let Some(item) = self.proxies.get(name) {
                        if item.is_group() && !item.hidden {
                            order.push(name.clone());
                        }
                    }
                }
            }
        }
        let mut rest: Vec<String> = self
            .proxies
            .values()
            .filter(|p| p.is_group() && !p.hidden && p.name != "GLOBAL" && !order.contains(&p.name))
            .map(|p| p.name.clone())
            .collect();
        rest.sort();
        order.extend(rest);
        if let Some(global) = self.proxies.get("GLOBAL") {
            if global.is_group() && !order.contains(&global.name) {
                order.push(global.name.clone());
            }
        }
        order
    }

    // ==================== Background Command Dispatchers ====================

    pub fn trigger_full_refresh(&self) {
        let tx = self.tx.clone();
        let svc_name = self.settings.service_name.clone();
        let client = self.client();

        tokio_rt().spawn(async move {
            let service = tokio::task::spawn_blocking(move || {
                service::query_service(&svc_name).unwrap_or_default()
            })
            .await
            .unwrap_or_default();

            let sys_proxy = proxy::get_system_proxy().unwrap_or_default();

            let version_res = client.get_version().await;
            let (api_online, api_error, version) = match version_res {
                Ok(v) => (true, None, Some(v)),
                Err(e) => (false, Some(e.to_string()), None),
            };

            let (config, proxies, proxy_providers, rule_providers, rules, connections) =
                if api_online {
                    let (cfg, prx, pp, rp, rls, conns) = tokio::join!(
                        client.get_configs(),
                        client.get_proxies(),
                        client.get_proxy_providers(),
                        client.get_rule_providers(),
                        client.get_rules(),
                        client.get_connections(),
                    );
                    (
                        cfg.ok(),
                        prx.ok().map(|x| x.proxies),
                        pp.ok().map(|x| x.providers),
                        rp.ok().map(|x| x.providers),
                        rls.ok().map(|x| x.rules),
                        conns.ok(),
                    )
                } else {
                    (None, None, None, None, None, None)
                };

            let _ = tx.send(UiEvent::SnapshotRefreshed {
                service,
                sys_proxy,
                api_online,
                api_error,
                version,
                config,
                proxies,
                proxy_providers,
                rule_providers,
                rules,
                connections,
            });
        });
    }

    pub fn trigger_connections_refresh(&self) {
        let tx = self.tx.clone();
        let client = self.client();
        tokio_rt().spawn(async move {
            if let Ok(conns) = client.get_connections().await {
                let _ = tx.send(UiEvent::ConnectionsRefreshed(conns));
            }
        });
    }

    // ---- Windows Service Lifecycle ----

    pub fn start_windows_service(&mut self) {
        self.busy_action = Some(format!("正在启动 Windows 服务「{}」...", self.settings.service_name));
        let tx = self.tx.clone();
        let name = self.settings.service_name.clone();
        tokio_rt().spawn(async move {
            let res = tokio::task::spawn_blocking(move || service::start_service(&name))
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
            let _ = tx.send(UiEvent::ActionFinished {
                label: "启动 Mihomo 服务".to_string(),
                result: res.map(|_| "Mihomo Windows 服务已启动".to_string()).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn stop_windows_service(&mut self) {
        self.busy_action = Some(format!("正在停止 Windows 服务「{}」...", self.settings.service_name));
        let tx = self.tx.clone();
        let name = self.settings.service_name.clone();
        tokio_rt().spawn(async move {
            let res = tokio::task::spawn_blocking(move || service::stop_service(&name))
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
            let _ = tx.send(UiEvent::ActionFinished {
                label: "停止 Mihomo 服务".to_string(),
                result: res.map(|_| "Mihomo Windows 服务已停止".to_string()).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn restart_windows_service(&mut self) {
        self.busy_action = Some(format!("正在重启 Windows 服务「{}」...", self.settings.service_name));
        let tx = self.tx.clone();
        let name = self.settings.service_name.clone();
        tokio_rt().spawn(async move {
            let res = tokio::task::spawn_blocking(move || service::restart_service(&name))
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
            let _ = tx.send(UiEvent::ActionFinished {
                label: "重启 Mihomo 服务".to_string(),
                result: res.map(|_| "Mihomo Windows 服务已重启".to_string()).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn install_windows_service(&mut self) {
        self.busy_action = Some(format!(
            "正在安装/修复 Windows 服务「{}」（配置免 UAC 权限并初始化目录）...",
            self.settings.service_name
        ));
        let tx = self.tx.clone();
        let svc_name = self.settings.service_name.clone();
        let core_path = self.settings.core_binary_path.clone();
        let work_dir = self.settings.work_dir.clone();
        let config_path = self.settings.active_config_path.clone();
        tokio_rt().spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                service::install_service_with_uac(&svc_name, &core_path, &work_dir, &config_path)
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
            let _ = tx.send(UiEvent::ActionFinished {
                label: "安装/修复 Mihomo Windows 服务".to_string(),
                result: res
                    .map(|_| "Windows 服务已安装并配置免 UAC 启停权限".to_string())
                    .map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn uninstall_windows_service(&mut self) {
        self.busy_action = Some(format!("正在卸载 Windows 服务「{}」...", self.settings.service_name));
        let tx = self.tx.clone();
        let svc_name = self.settings.service_name.clone();
        tokio_rt().spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                service::uninstall_service_with_uac(&svc_name)
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
            let _ = tx.send(UiEvent::ActionFinished {
                label: "卸载 Mihomo Windows 服务".to_string(),
                result: res
                    .map(|_| "Windows 服务已卸载".to_string())
                    .map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    // ---- System Proxy, Resident Mode & Auto-Start ----

    pub fn toggle_system_proxy(&mut self, enable: bool) {
        self.settings.system_proxy_managed = enable;
        let _ = config::save_settings(&self.settings);

        let server = format!("127.0.0.1:{}", self.settings.mixed_port);
        match proxy::set_system_proxy(enable, &server, &self.settings.proxy_bypass) {
            Ok(()) => {
                self.system_proxy = proxy::get_system_proxy().unwrap_or_default();
                let (state, reason) = self.system_proxy_takeover_state();
                match state {
                    SystemProxyTakeoverState::ManagedEffective => {
                        self.set_notice(false, format!("系统代理已接管并生效 ({server})"));
                    }
                    SystemProxyTakeoverState::ManagedIneffective => {
                        self.set_notice(
                            true,
                            format!(
                                "已设置为接管系统代理，但当前未生效（{}）",
                                reason.unwrap_or("请检查服务状态")
                            ),
                        );
                    }
                    SystemProxyTakeoverState::Unmanaged => {
                        self.set_notice(false, "已取消系统代理接管");
                    }
                }
            }
            Err(e) => self.set_notice(true, format!("切换系统代理失败: {e}")),
        }
        self.sync_tray_state();
    }

    pub fn reapply_system_proxy(&mut self) {
        self.settings.system_proxy_managed = true;
        let _ = config::save_settings(&self.settings);
        let server = format!("127.0.0.1:{}", self.settings.mixed_port);
        match proxy::set_system_proxy(true, &server, &self.settings.proxy_bypass) {
            Ok(()) => {
                self.system_proxy = proxy::get_system_proxy().unwrap_or_default();
                let (state, reason) = self.system_proxy_takeover_state();
                if state == SystemProxyTakeoverState::ManagedEffective {
                    self.set_notice(false, format!("系统代理已重新应用并生效 ({server})"));
                } else {
                    self.set_notice(
                        true,
                        format!(
                            "已重新写入系统代理，但仍未生效（{}）",
                            reason.unwrap_or("请检查 Mihomo 服务是否运行")
                        ),
                    );
                }
            }
            Err(e) => self.set_notice(true, format!("重新应用系统代理失败: {e}")),
        }
        self.sync_tray_state();
    }

    pub fn toggle_resident_mode(&mut self, enable: bool) {
        self.settings.resident_mode = enable;
        self.resident_flag.store(enable, Ordering::SeqCst);
        let _ = config::save_settings(&self.settings);

        match autostart::sync_autostart(enable) {
            Ok(()) => {
                if enable {
                    self.set_notice(
                        false,
                        "已开启常驻模式：托盘常驻、关闭窗口最小化到托盘，并已启用开机自启",
                    );
                } else {
                    self.set_notice(
                        false,
                        "已关闭常驻模式：关闭窗口将直接退出，并已关闭开机自启",
                    );
                }
            }
            Err(e) => {
                self.set_notice(true, format!("切换常驻模式时同步开机自启失败: {e}"));
            }
        }
        self.sync_tray_state();
    }

    // ---- TUN Mode Toggle (with Requirement 7 default TUN/DNS injection!) ----

    pub fn toggle_tun_mode(&mut self, enable: bool) {
        self.settings.tun_enabled = enable;
        let _ = config::save_settings(&self.settings);

        self.busy_action = Some(if enable {
            "正在开启 TUN 模式并应用默认 TUN/DNS 配置...".to_string()
        } else {
            "正在关闭 TUN 模式...".to_string()
        });

        let active_cfg_path = self.settings.active_config_path.clone();
        let force_default = self.settings.force_default_tun_dns;
        let fallback_ec = self.settings.controller_addr.clone();
        let fallback_secret = self.settings.controller_secret.clone();
        let fallback_mixed_port = self.settings.mixed_port;
        let active_profile_file = self
            .settings
            .active_profile_id
            .as_ref()
            .and_then(|id| self.profiles.profiles.iter().find(|p| &p.id == id))
            .map(|p| p.file_path.clone());
        let client = self.client();
        let tx = self.tx.clone();

        tokio_rt().spawn(async move {
            let res: anyhow::Result<String> = async {
                // Read raw profile if available, otherwise read current active service config
                let source_path = active_profile_file
                    .filter(|p| p.exists())
                    .unwrap_or_else(|| std::path::PathBuf::from(&active_cfg_path));

                if source_path.exists() {
                    let raw = std::fs::read_to_string(&source_path)?;
                    let merged = config::merge_runtime_config(
                        &raw,
                        enable,
                        force_default,
                        &fallback_ec,
                        &fallback_secret,
                        fallback_mixed_port,
                    )?;
                    let target = std::path::PathBuf::from(&active_cfg_path);
                    tokio::task::spawn_blocking(move || {
                        service::write_protected_file(&target, &merged)
                    })
                    .await??;

                    // Hot-reload full merged config (ensures default DNS + TUN take effect immediately!)
                    if let Err(reload_err) = client.reload_configs(&active_cfg_path, true).await {
                        // Fallback to live PATCH /configs if reload fails
                        client
                            .patch_configs(json!({ "tun": { "enable": enable } }))
                            .await
                            .map_err(|patch_err| {
                                anyhow::anyhow!("配置已写入但热重载失败 ({reload_err}); PATCH 失败 ({patch_err})")
                            })?;
                    }
                } else {
                    client
                        .patch_configs(json!({ "tun": { "enable": enable } }))
                        .await?;
                }

                Ok(if enable {
                    "TUN 模式已开启（缺失的 TUN/DNS 已自动补全默认配置）".to_string()
                } else {
                    "TUN 模式已关闭".to_string()
                })
            }
            .await;

            let _ = tx.send(UiEvent::ActionFinished {
                label: "切换 TUN 模式".to_string(),
                result: res.map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    // ---- Proxy Mode & Live Config Patch ----

    pub fn set_proxy_mode(&mut self, mode: &str) {
        let mode_str = mode.to_string();
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.patch_configs(json!({ "mode": mode_str })).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: format!("切换模式为 {mode_str}"),
                result: res.map(|_| format!("已切换为 {mode_str} 模式")).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn patch_runtime_config(&mut self, label: &str, patch: serde_json::Value) {
        let client = self.client();
        let tx = self.tx.clone();
        let lbl = label.to_string();
        tokio_rt().spawn(async move {
            let res = client.patch_configs(patch).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: lbl.clone(),
                result: res.map(|_| format!("{lbl} 已更新")).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    // ---- Proxy Switching & Latency Testing ----

    pub fn select_proxy(&mut self, group: String, proxy_name: String) {
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.select_proxy(&group, &proxy_name).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: format!("切换节点 {group} -> {proxy_name}"),
                result: res
                    .map(|_| format!("已将「{group}」切换至「{proxy_name}」"))
                    .map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn unfix_proxy_group(&mut self, group: String) {
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.unfix_proxy(&group).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: format!("恢复「{group}」自动选择"),
                result: res
                    .map(|_| format!("已取消「{group}」固定节点"))
                    .map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn test_single_proxy_delay(&self, proxy_name: String) {
        let client = self.client();
        let tx = self.tx.clone();
        let test_url = self.settings.delay_test_url.clone();
        let timeout = self.settings.delay_test_timeout_ms;
        tokio_rt().spawn(async move {
            let res = client.test_proxy_delay(&proxy_name, &test_url, timeout).await;
            let _ = tx.send(UiEvent::DelayTested {
                proxy_name,
                delay: res.map_err(|e| e.to_string()),
            });
        });
    }

    pub fn test_group_delay(&mut self, group: String) {
        self.busy_action = Some(format!("正在对策略组「{group}」批量测速..."));
        let client = self.client();
        let tx = self.tx.clone();
        let test_url = self.settings.delay_test_url.clone();
        let timeout = self.settings.delay_test_timeout_ms;
        tokio_rt().spawn(async move {
            let res = client.test_group_delay(&group, &test_url, timeout).await;
            let _ = tx.send(UiEvent::GroupDelayTested {
                group,
                delays: res.map_err(|e| e.to_string()),
            });
        });
    }

    // ---- Profiles Import / Update / Activate / Preview ----

    pub fn import_profile_from_url(&mut self, cx: &App) {
        let url = self.profile_url_input.read(cx).value().to_string();
        let name = self.profile_name_input.read(cx).value().to_string();
        if url.trim().is_empty() {
            self.set_notice(true, "请输入配置订阅 URL");
            return;
        }
        self.busy_action = Some("正在从 URL 下载并校验 Mihomo 配置...".to_string());
        let tx = self.tx.clone();
        let proxy_url = self.local_http_proxy_url();
        tokio_rt().spawn(async move {
            let name_opt = if name.trim().is_empty() {
                None
            } else {
                Some(name)
            };
            let res =
                config::fetch_profile_from_url(name_opt, &url, proxy_url.as_deref()).await;
            let _ = tx.send(UiEvent::ProfileImported(
                res.map(|(p, _)| p).map_err(|e| e.to_string()),
            ));
        });
    }

    pub fn import_profile_from_local_path(&mut self, cx: &App) {
        let path_str = self.local_import_input.read(cx).value().to_string();
        let name = self.profile_name_input.read(cx).value().to_string();
        let trimmed = path_str.trim().trim_matches('"');
        if trimmed.is_empty() {
            self.set_notice(true, "请输入本地 YAML 文件路径");
            return;
        }
        let name_opt = if name.trim().is_empty() {
            None
        } else {
            Some(name)
        };
        match config::import_local_profile(name_opt, Path::new(trimmed)) {
            Ok(p) => {
                let p_name = p.name.clone();
                self.profiles.profiles.push(p);
                let _ = config::save_profiles(&self.profiles);
                self.set_notice(false, format!("已导入本地配置「{p_name}」"));
            }
            Err(e) => self.set_notice(true, format!("导入本地配置失败: {e}")),
        }
    }

    pub fn sync_current_service_config_as_profile(&mut self) {
        let src = self.settings.active_config_path.clone();
        match config::import_local_profile(Some("同步自当前服务配置".to_string()), Path::new(&src))
        {
            Ok(p) => {
                let id = p.id.clone();
                self.profiles.profiles.push(p);
                self.settings.active_profile_id = Some(id);
                let _ = config::save_profiles(&self.profiles);
                let _ = config::save_settings(&self.settings);
                self.set_notice(false, "已将当前服务配置同步至配置列表");
            }
            Err(e) => self.set_notice(true, format!("同步失败: {e}")),
        }
    }

    pub fn update_profile(&mut self, id: &str) {
        let Some(profile) = self.profiles.profiles.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        self.busy_action = Some(format!("正在更新远程订阅「{}」...", profile.name));
        let tx = self.tx.clone();
        let proxy_url = self.local_http_proxy_url();
        tokio_rt().spawn(async move {
            let res = config::refresh_remote_profile(&profile, proxy_url.as_deref()).await;
            let _ = tx.send(UiEvent::ProfileUpdated(res.map_err(|e| e.to_string())));
        });
    }

    pub fn activate_profile(&mut self, id: &str) {
        let Some(profile) = self.profiles.profiles.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        self.settings.active_profile_id = Some(profile.id.clone());
        let _ = config::save_settings(&self.settings);

        self.busy_action = Some(format!("正在合并并应用配置「{}」...", profile.name));
        let active_cfg_path = self.settings.active_config_path.clone();
        let tun_enabled = self.settings.tun_enabled;
        let force_default = self.settings.force_default_tun_dns;
        let fallback_ec = self.settings.controller_addr.clone();
        let fallback_secret = self.settings.controller_secret.clone();
        let fallback_mixed_port = self.settings.mixed_port;
        let client = self.client();
        let tx = self.tx.clone();

        tokio_rt().spawn(async move {
            let res: anyhow::Result<String> = async {
                let raw = std::fs::read_to_string(&profile.file_path)?;
                let merged = config::merge_runtime_config(
                    &raw,
                    tun_enabled,
                    force_default,
                    &fallback_ec,
                    &fallback_secret,
                    fallback_mixed_port,
                )?;
                let target = std::path::PathBuf::from(&active_cfg_path);
                tokio::task::spawn_blocking(move || {
                    service::write_protected_file(&target, &merged)
                })
                .await??;

                client.reload_configs(&active_cfg_path, true).await?;
                Ok(format!(
                    "已启用配置「{}」并热重载生效",
                    profile.name
                ))
            }
            .await;

            let _ = tx.send(UiEvent::ActionFinished {
                label: "启用配置".to_string(),
                result: res.map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn preview_merged_profile(&mut self, id: &str) {
        let Some(profile) = self.profiles.profiles.iter().find(|p| p.id == id) else {
            return;
        };
        match std::fs::read_to_string(&profile.file_path).and_then(|raw| {
            config::merge_runtime_config(
                &raw,
                self.settings.tun_enabled,
                self.settings.force_default_tun_dns,
                &self.settings.controller_addr,
                &self.settings.controller_secret,
                self.settings.mixed_port,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
        }) {
            Ok(merged) => {
                // Keep preview concise (first 250 lines) so rendering is instant
                let lines: Vec<&str> = merged.lines().take(250).collect();
                let mut text = lines.join("\n");
                if merged.lines().count() > 250 {
                    text.push_str("\n\n... (仅展示前 250 行)");
                }
                self.preview_yaml = Some((format!("合并后运行时配置预览: {}", profile.name), text));
            }
            Err(e) => self.set_notice(true, format!("生成配置预览失败: {e}")),
        }
    }

    pub fn delete_profile(&mut self, id: &str) {
        if let Some(idx) = self.profiles.profiles.iter().position(|p| p.id == id) {
            let removed = self.profiles.profiles.remove(idx);
            let _ = std::fs::remove_file(&removed.file_path);
            if self.settings.active_profile_id.as_deref() == Some(id) {
                self.settings.active_profile_id = None;
                let _ = config::save_settings(&self.settings);
            }
            let _ = config::save_profiles(&self.profiles);
            self.set_notice(false, format!("已删除配置「{}」", removed.name));
        }
    }

    // ---- External Providers (Proxy & Rule Providers) ----

    pub fn update_proxy_provider(&mut self, name: String) {
        self.busy_action = Some(format!("正在更新代理集合「{name}」..."));
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.update_proxy_provider(&name).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: format!("更新代理集合「{name}」"),
                result: res.map(|_| format!("代理集合「{name}」已更新")).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn healthcheck_proxy_provider(&mut self, name: String) {
        self.busy_action = Some(format!("正在对代理集合「{name}」执行健康检查..."));
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.healthcheck_proxy_provider(&name).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: format!("健康检查「{name}」"),
                result: res.map(|_| format!("代理集合「{name}」健康检查完成")).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn update_all_proxy_providers(&mut self) {
        let names: Vec<String> = self
            .proxy_providers
            .values()
            .filter(|p| p.vehicle_type != "Compatible")
            .map(|p| p.name.clone())
            .collect();
        if names.is_empty() {
            self.set_notice(false, "没有可更新的外部代理集合");
            return;
        }
        self.busy_action = Some(format!("正在更新全部 {} 个代理集合...", names.len()));
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let mut errs = Vec::new();
            for n in &names {
                if let Err(e) = client.update_proxy_provider(n).await {
                    errs.push(format!("{n}: {e}"));
                }
            }
            let result = if errs.is_empty() {
                Ok(format!("已更新全部 {} 个代理集合", names.len()))
            } else {
                Err(errs.join("; "))
            };
            let _ = tx.send(UiEvent::ActionFinished {
                label: "更新全部代理集合".to_string(),
                result,
                refresh_after: true,
            });
        });
    }

    pub fn update_rule_provider(&mut self, name: String) {
        self.busy_action = Some(format!("正在更新规则集合「{name}」..."));
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.update_rule_provider(&name).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: format!("更新规则集合「{name}」"),
                result: res.map(|_| format!("规则集合「{name}」已更新")).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn update_all_rule_providers(&mut self) {
        let names: Vec<String> = self.rule_providers.keys().cloned().collect();
        if names.is_empty() {
            self.set_notice(false, "没有可更新的规则集合");
            return;
        }
        self.busy_action = Some(format!("正在更新全部 {} 个规则集合...", names.len()));
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let mut errs = Vec::new();
            for n in &names {
                if let Err(e) = client.update_rule_provider(n).await {
                    errs.push(format!("{n}: {e}"));
                }
            }
            let result = if errs.is_empty() {
                Ok(format!("已更新全部 {} 个规则集合", names.len()))
            } else {
                Err(errs.join("; "))
            };
            let _ = tx.send(UiEvent::ActionFinished {
                label: "更新全部规则集合".to_string(),
                result,
                refresh_after: true,
            });
        });
    }

    // ---- Connections ----

    pub fn close_connection(&mut self, id: String) {
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.close_connection(&id).await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: "断开连接".to_string(),
                result: res.map(|_| "已断开指定连接".to_string()).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn close_all_connections(&mut self) {
        let client = self.client();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.close_all_connections().await;
            let _ = tx.send(UiEvent::ActionFinished {
                label: "断开全部连接".to_string(),
                result: res.map(|_| "已断开所有活跃连接".to_string()).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    // ---- GitHub Core Management ----

    pub fn fetch_github_releases(&mut self) {
        self.busy_action = Some("正在从 GitHub 拉取 MetaCubeX/mihomo Release 列表...".to_string());
        let include_pre = self.settings.include_prerelease;
        let proxy_url = if self.settings.use_proxy_for_github {
            self.local_http_proxy_url()
        } else {
            None
        };
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = core_manager::fetch_github_releases(include_pre, proxy_url.as_deref()).await;
            let _ = tx.send(UiEvent::ReleasesFetched(res.map_err(|e| e.to_string())));
        });
    }

    pub fn download_core_asset(&mut self, tag: String, asset: GitHubAsset) {
        self.busy_action = Some(format!("正在下载并解压 {} ...", asset.name));
        let proxy_url = if self.settings.use_proxy_for_github {
            self.local_http_proxy_url()
        } else {
            None
        };
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res =
                core_manager::download_and_extract_asset(&tag, &asset, proxy_url.as_deref()).await;
            let _ = tx.send(UiEvent::CoreDownloaded(res.map_err(|e| e.to_string())));
        });
    }

    pub fn activate_local_core(&mut self, core: LocalCoreVersion) {
        self.busy_action = Some(format!(
            "正在停止服务、替换核心为 {} 并重启 Windows 服务...",
            core.tag
        ));
        let svc_name = self.settings.service_name.clone();
        let target_exe = std::path::PathBuf::from(&self.settings.core_binary_path);
        let source_exe = core.exe_path;
        let tag = core.tag;
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = tokio::task::spawn_blocking(move || {
                service::replace_core_and_restart(&svc_name, &source_exe, &target_exe)
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!(e)));
            let _ = tx.send(UiEvent::ActionFinished {
                label: format!("切换核心版本 {tag}"),
                result: res
                    .map(|_| format!("已成功切换服务核心为「{tag}」并重启服务"))
                    .map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn delete_local_core(&mut self, tag: &str) {
        match core_manager::delete_local_core(tag) {
            Ok(()) => {
                self.local_cores = core_manager::list_local_cores();
                self.set_notice(false, format!("已删除本地缓存核心「{tag}」"));
            }
            Err(e) => self.set_notice(true, format!("删除核心失败: {e}")),
        }
    }

    // ---- Mihomo Management API Actions ----

    pub fn run_core_management_action(&mut self, action: &'static str) {
        let label = match action {
            "reload" => "热重载服务配置 (PUT /configs?force=true)",
            "geo" => "更新 GEO 数据库 (POST /configs/geo)",
            "fakeip" => "清空 FakeIP 缓存 (POST /cache/fakeip/flush)",
            "dns" => "清空 DNS 缓存 (POST /cache/dns/flush)",
            "restart" => "通过 API 重启内核 (POST /restart)",
            "upgrade" => "通过 API 更新内核 (POST /upgrade)",
            "upgrade_ui" => "更新外部 UI (POST /upgrade/ui)",
            _ => "执行核心接口",
        };
        self.busy_action = Some(format!("正在执行: {label}..."));
        let client = self.client();
        let active_cfg = self.settings.active_config_path.clone();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = match action {
                "reload" => client.reload_configs(&active_cfg, true).await,
                "geo" => client.update_geo().await,
                "fakeip" => client.flush_fakeip_cache().await,
                "dns" => client.flush_dns_cache().await,
                "restart" => client.restart_core().await,
                "upgrade" => client.upgrade_core().await,
                "upgrade_ui" => client.upgrade_ui().await,
                _ => Ok(()),
            };
            let _ = tx.send(UiEvent::ActionFinished {
                label: label.to_string(),
                result: res.map(|_| format!("{label} 完成")).map_err(|e| e.to_string()),
                refresh_after: true,
            });
        });
    }

    pub fn query_dns(&mut self, cx: &App) {
        let domain = self.dns_domain_input.read(cx).value().to_string();
        if domain.trim().is_empty() {
            self.set_notice(true, "请输入要查询的域名");
            return;
        }
        self.busy_action = Some(format!("正在查询 {} ({}) ...", domain.trim(), self.dns_record_type));
        let client = self.client();
        let rtype = self.dns_record_type.clone();
        let tx = self.tx.clone();
        tokio_rt().spawn(async move {
            let res = client.query_dns(&domain, &rtype).await;
            let _ = tx.send(UiEvent::DnsQueried(res.map_err(|e| e.to_string())));
        });
    }

    // ---- Settings Save & Auto-Discover ----

    pub fn save_all_settings(&mut self, cx: &App) {
        self.settings.service_name = self.setting_service_name.read(cx).value().trim().to_string();
        self.settings.core_binary_path = self.setting_core_path.read(cx).value().trim().to_string();
        self.settings.work_dir = self.setting_work_dir.read(cx).value().trim().to_string();
        self.settings.active_config_path =
            self.setting_config_path.read(cx).value().trim().to_string();
        self.settings.controller_addr = config::normalize_controller_addr(
            &self.setting_controller_addr.read(cx).value(),
        );
        self.settings.controller_secret = self.setting_controller_secret.read(cx).value().to_string();
        if let Ok(port) = self.setting_mixed_port.read(cx).value().trim().parse::<u16>() {
            if port > 0 {
                self.settings.mixed_port = port;
            }
        }
        self.settings.proxy_bypass = self.setting_proxy_bypass.read(cx).value().trim().to_string();
        self.settings.delay_test_url = self.setting_test_url.read(cx).value().trim().to_string();

        if let Err(e) = config::save_settings(&self.settings) {
            self.set_notice(true, format!("保存设置失败: {e}"));
            return;
        }

        // Reconfigure background WebSocket workers
        {
            let mut t = self.telemetry.lock().unwrap();
            let lvl = t.log_level.clone();
            t.reconfigure(
                self.settings.controller_addr.clone(),
                self.settings.controller_secret.clone(),
                lvl,
            );
        }

        // If system proxy is currently managed, reapply with updated port/bypass
        if self.settings.system_proxy_managed {
            let server = format!("127.0.0.1:{}", self.settings.mixed_port);
            let _ = proxy::set_system_proxy(true, &server, &self.settings.proxy_bypass);
            self.system_proxy = proxy::get_system_proxy().unwrap_or_default();
        }
        self.sync_tray_state();

        self.set_notice(false, "设置已保存并生效");
        self.trigger_full_refresh();
    }

    pub fn auto_discover_service(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(info) = service::query_service(&self.settings.service_name) {
            if let Some(bin) = &info.binary_path {
                if let Some(d) = config::discover_from_service_binary_cmdline(bin) {
                    if let Some(exe) = d.core_binary_path {
                        self.settings.core_binary_path = exe.clone();
                        self.setting_core_path
                            .update(cx, |s, cx| s.set_value(exe, window, cx));
                    }
                    if let Some(work) = d.work_dir {
                        self.settings.work_dir = work.clone();
                        self.setting_work_dir
                            .update(cx, |s, cx| s.set_value(work, window, cx));
                    }
                    if let Some(cfg) = d.config_path {
                        self.settings.active_config_path = cfg.clone();
                        self.setting_config_path
                            .update(cx, |s, cx| s.set_value(cfg, window, cx));
                    }
                }
            }
        }
        config::sync_settings_from_yaml(&mut self.settings);
        let addr = self.settings.controller_addr.clone();
        let sec = self.settings.controller_secret.clone();
        let port = self.settings.mixed_port.to_string();
        self.setting_controller_addr
            .update(cx, |s, cx| s.set_value(addr, window, cx));
        self.setting_controller_secret
            .update(cx, |s, cx| s.set_value(sec, window, cx));
        self.setting_mixed_port
            .update(cx, |s, cx| s.set_value(port, window, cx));
        let _ = config::save_settings(&self.settings);
        self.set_notice(false, "已从 Windows 服务配置与 YAML 自动同步路径与控制器参数");
        self.trigger_full_refresh();
    }

    pub fn set_dark_mode(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.dark_mode = dark;
        let mode = if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(mode, Some(window), cx);
        let _ = config::save_settings(&self.settings);
    }

    pub fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_dark_mode(!self.settings.dark_mode, window, cx);
    }

    // ==================== Root Layout Shell ====================

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let svc_running = self.service_status.state.is_running();
        let svc_color = if svc_running {
            theme.success
        } else {
            theme.danger
        };
        let svc_text = match self.service_status.pid {
            Some(pid) if svc_running => format!("服务运行中 · PID {pid}"),
            _ => format!("服务: {}", self.service_status.state.label()),
        };

        let is_dark = self.settings.dark_mode;

        TitleBar::new().child(
            div()
                .h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .pr_2()
                .child(
                    div()
                        .h_flex()
                        .flex_1()
                        .min_w_0()
                        .mr_2()
                        .items_center()
                        .gap_2p5()
                        .child(
                            Icon::new(IconName::Shield)
                                .size_4()
                                .flex_shrink_0()
                                .text_color(theme.primary),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_sm()
                                .font_semibold()
                                .text_color(theme.foreground)
                                .child("Mihomo Desktop"),
                        )
                        .child(pill_badge(svc_text, svc_color).flex_shrink_0())
                        .when(self.api_online, |el| {
                            let ver = self
                                .version
                                .as_ref()
                                .map(|v| v.version.clone())
                                .unwrap_or_else(|| "API 已连接".to_string());
                            el.child(pill_badge(ver, theme.info).flex_shrink_0())
                        })
                        .when_some(self.busy_action.clone(), |el, busy| {
                            el.child(
                                div()
                                    .h_flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2()
                                    .py(px(2.))
                                    .max_w(px(320.))
                                    .rounded_full()
                                    .bg(theme.info.opacity(0.14))
                                    .border_1()
                                    .border_color(theme.info.opacity(0.35))
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.info)
                                    .child(Spinner::new().xsmall())
                                    .child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(busy),
                                    ),
                            )
                        }),
                )
                .child(
                    // Occlude the parent TitleBar's WindowControlArea::Drag hitbox so Windows
                    // WM_NCHITTEST treats these action buttons as HTCLIENT instead of HTCAPTION
                    // (otherwise DefWindowProcW enters the caption move loop on WM_NCLBUTTONDOWN
                    // and swallows mouse-up, preventing on_click from firing).
                    div()
                        .h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_1p5()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .child(
                            Button::new("global-refresh")
                                .ghost()
                                .xsmall()
                                .icon(IconName::RefreshCw)
                                .tooltip("刷新状态")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.trigger_full_refresh();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("toggle-theme")
                                .ghost()
                                .xsmall()
                                .icon(if is_dark {
                                    IconName::Sun
                                } else {
                                    IconName::Moon
                                })
                                .tooltip(if is_dark {
                                    "切换明亮模式"
                                } else {
                                    "切换暗黑模式"
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_theme(window, cx);
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (up, down, mem_inuse) = {
            let t = self.telemetry.lock().unwrap();
            (t.current_traffic.up, t.current_traffic.down, t.memory.inuse)
        };
        let active_page = self.page;

        div()
            .v_flex()
            .w(px(210.))
            .h_full()
            .flex_shrink_0()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .justify_between()
            .p_3()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .children(AppPage::ALL.into_iter().map(|p| {
                        let selected = active_page == p;
                        div()
                            .id(SharedString::from(format!("nav-{p:?}")))
                            .h_flex()
                            .items_center()
                            .gap_2p5()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .cursor_pointer()
                            .text_sm()
                            .when(selected, |el| {
                                el.bg(theme.primary.opacity(0.16))
                                    .text_color(theme.primary)
                                    .font_semibold()
                            })
                            .when(!selected, |el| {
                                el.text_color(theme.sidebar_foreground)
                                    .hover(|s| s.bg(theme.list_hover))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.page = p;
                                if p == AppPage::Connections {
                                    this.trigger_connections_refresh();
                                } else if p == AppPage::Cores {
                                    this.local_cores = core_manager::list_local_cores();
                                }
                                cx.notify();
                            }))
                            .child(Icon::new(p.icon()).size_4())
                            .child(p.title())
                    })),
            )
            .child(
                div()
                    .v_flex()
                    .gap_1p5()
                    .p_2p5()
                    .rounded_lg()
                    .bg(theme.secondary.opacity(0.5))
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .text_xs()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_1()
                                    .text_color(theme.success)
                                    .child(Icon::new(IconName::ArrowDown).size_3())
                                    .child("下载"),
                            )
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child(format_speed(down)),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .text_xs()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_1()
                                    .text_color(theme.info)
                                    .child(Icon::new(IconName::ArrowUp).size_3())
                                    .child("上传"),
                            )
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(theme.foreground)
                                    .child(format_speed(up)),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .text_xs()
                            .pt_1()
                            .border_t_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_color(theme.muted_foreground)
                                    .child("内存占用"),
                            )
                            .child(
                                div()
                                    .text_color(theme.foreground)
                                    .child(format_bytes(mem_inuse)),
                            ),
                    ),
            )
    }

}

impl Render for MihomoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.flush_pending_notices(window, cx);

        let page_content: AnyElement = match self.page {
            AppPage::Dashboard => self.render_dashboard_page(window, cx).into_any_element(),
            AppPage::Proxies => self.render_proxies_page(window, cx).into_any_element(),
            AppPage::Profiles => self.render_profiles_page(window, cx).into_any_element(),
            AppPage::Providers => self.render_providers_page(window, cx).into_any_element(),
            AppPage::Connections => self.render_connections_page(window, cx).into_any_element(),
            AppPage::Cores => self.render_cores_page(window, cx).into_any_element(),
            AppPage::Tools => self.render_tools_page(window, cx).into_any_element(),
            AppPage::Settings => self.render_settings_page(window, cx).into_any_element(),
        };
        let title_bar = self.render_title_bar(cx);
        let sidebar = self.render_sidebar(cx);
        let bg = cx.theme().background;
        let fg = cx.theme().foreground;

        div()
            .v_flex()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .child(title_bar)
            .child(
                div()
                    .h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .p_5()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .child(page_content),
                            ),
                    ),
            )
    }
}
