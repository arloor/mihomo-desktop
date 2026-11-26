pub mod app;
pub mod pages_connections_cores;
pub mod pages_dashboard;
pub mod pages_profiles_providers;
pub mod pages_tools_settings;

pub use app::MihomoApp;

use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPage {
    Dashboard,
    Proxies,
    Profiles,
    Providers,
    Connections,
    Cores,
    Tools,
    Settings,
}

impl AppPage {
    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "概览",
            Self::Proxies => "代理切换",
            Self::Profiles => "配置管理",
            Self::Providers => "外部资源",
            Self::Connections => "连接管理",
            Self::Cores => "核心管理",
            Self::Tools => "接口与日志",
            Self::Settings => "应用设置",
        }
    }

    pub fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName;
        match self {
            Self::Dashboard => IconName::LayoutDashboard,
            Self::Proxies => IconName::Globe,
            Self::Profiles => IconName::FileCode,
            Self::Providers => IconName::Database,
            Self::Connections => IconName::Network,
            Self::Cores => IconName::Cpu,
            Self::Tools => IconName::Terminal,
            Self::Settings => IconName::Settings,
        }
    }

    pub const ALL: [Self; 8] = [
        Self::Dashboard,
        Self::Proxies,
        Self::Profiles,
        Self::Providers,
        Self::Connections,
        Self::Cores,
        Self::Tools,
        Self::Settings,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxySort {
    Default,
    LatencyAsc,
    NameAsc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvidersTab {
    ProxyProviders,
    RuleProviders,
    Rules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnSort {
    Newest,
    DownloadDesc,
    UploadDesc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolsTab {
    Logs,
    ApiActions,
    DnsQuery,
}

/// Reusable elevated card container matching active theme.
pub fn card(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .bg(theme.secondary.opacity(0.45))
        .border_1()
        .border_color(theme.border)
        .rounded_lg()
        .p_4()
}

/// Reusable compact pill badge.
pub fn pill_badge(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .px_2()
        .py(px(2.))
        .rounded_full()
        .bg(color.opacity(0.16))
        .border_1()
        .border_color(color.opacity(0.35))
        .text_xs()
        .font_semibold()
        .text_color(color)
        .child(text.into())
}

/// Color-coded latency badge helper.
pub fn delay_color(delay: Option<u32>, cx: &App) -> Hsla {
    let theme = cx.theme();
    match delay {
        None => theme.muted_foreground,
        Some(0) => theme.danger,
        Some(1..=180) => theme.success,
        Some(181..=450) => theme.warning,
        Some(_) => theme.danger,
    }
}

pub fn delay_text(delay: Option<u32>) -> String {
    match delay {
        None => "测速".to_string(),
        Some(0) => "超时".to_string(),
        Some(ms) => format!("{ms} ms"),
    }
}

pub fn section_title(title: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_base()
        .font_semibold()
        .text_color(cx.theme().foreground)
        .child(title.into())
}

/// Section header with title and optional subtitle.
pub fn section_header(title: impl Into<SharedString>, subtitle: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .v_flex()
        .gap_0p5()
        .child(section_title(title, cx))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(subtitle.into()),
        )
}
