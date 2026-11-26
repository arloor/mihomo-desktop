use crate::config::{format_bytes, format_speed};
use crate::geo::{GeoRegion, NodeGeoInfo};
use crate::system::proxy::SystemProxyTakeoverState;
use crate::system::service::ServiceState;
use crate::ui::app::MihomoApp;
use crate::ui::flags;
use crate::ui::{
    AppPage, ProxySort, card, delay_color, delay_text, pill_badge, section_header, section_title,
};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl MihomoApp {
    pub fn render_dashboard_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let svc_running = self.service_status.state.is_running();
        let svc_not_installed = self.service_status.state == ServiceState::NotInstalled;
        let core_missing = !std::path::Path::new(&self.settings.core_binary_path).exists();
        let svc_color = if svc_running {
            theme.success
        } else {
            theme.danger
        };

        let current_mode = self
            .mihomo_config
            .as_ref()
            .map(|c| c.mode.to_ascii_lowercase())
            .unwrap_or_default();

        let (up, down, mem_inuse, history) = {
            let t = self.telemetry.lock().unwrap();
            (
                t.current_traffic.up,
                t.current_traffic.down,
                t.memory.inuse,
                t.traffic_history.iter().copied().collect::<Vec<_>>(),
            )
        };

        let active_conns = self
            .connections
            .connections
            .as_ref()
            .map(|c| c.len())
            .unwrap_or(0);
        let total_down = self.connections.download_total;
        let total_up = self.connections.upload_total;

        let active_profile = self
            .settings
            .active_profile_id
            .as_ref()
            .and_then(|id| self.profiles.profiles.iter().find(|p| &p.id == id));

        let (proxy_takeover_state, proxy_takeover_reason) = self.system_proxy_takeover_state();
        let proxy_state_color = match proxy_takeover_state {
            SystemProxyTakeoverState::ManagedEffective => theme.success,
            SystemProxyTakeoverState::ManagedIneffective => theme.warning,
            SystemProxyTakeoverState::Unmanaged => theme.muted_foreground,
        };
        let proxy_badge_label = match proxy_takeover_state {
            SystemProxyTakeoverState::ManagedEffective => "已生效",
            SystemProxyTakeoverState::ManagedIneffective => "未生效",
            SystemProxyTakeoverState::Unmanaged => "未接管",
        };
        let proxy_sub = match proxy_takeover_state {
            SystemProxyTakeoverState::ManagedEffective => {
                format!("127.0.0.1:{}", self.settings.mixed_port)
            }
            SystemProxyTakeoverState::ManagedIneffective => proxy_takeover_reason
                .unwrap_or("请检查代理设置或服务状态")
                .to_string(),
            SystemProxyTakeoverState::Unmanaged => {
                format!("端口 127.0.0.1:{}", self.settings.mixed_port)
            }
        };

        let tun_info = self.mihomo_config.as_ref().and_then(|c| c.tun.as_ref());
        let tun_active = tun_info.is_some_and(|t| t.enable);
        let tun_sub = tun_info.filter(|t| t.enable).map(|t| format!(
                "{} · {}",
                if t.device.is_empty() { "Mihomo" } else { &t.device },
                if t.stack.is_empty() { "system" } else { &t.stack }
            ));

        let max_speed = history
            .iter()
            .map(|p| p.down.max(p.up))
            .max()
            .unwrap_or(1)
            .max(1024);

        div()
            .id("dashboard-scroll")
            .v_flex()
            .size_full()
            .gap_4()
            .pr_2()
            .overflow_y_scrollbar()
            // 1. Windows Service Lifecycle Hero Card
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .flex_wrap()
                            .gap_3()
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Icon::new(IconName::Server)
                                                    .size_5()
                                                    .text_color(theme.primary),
                                            )
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .font_bold()
                                                    .child(format!(
                                                        "Windows 服务: {}",
                                                        self.service_status.service_name
                                                    )),
                                            )
                                            .child(pill_badge(
                                                self.service_status.state.label(),
                                                svc_color,
                                            ))
                                            .when_some(self.service_status.pid, |el, pid| {
                                                el.child(pill_badge(
                                                    format!("PID {pid}"),
                                                    theme.muted_foreground,
                                                ))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!(
                                                "启动类型：{}　控制器：http://{}",
                                                self.service_status.start_type,
                                                self.settings.controller_addr
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .when(svc_not_installed && !core_missing, |el| {
                                        el.child(
                                            Button::new("svc-install-now")
                                                .primary()
                                                .small()
                                                .icon(IconName::Wrench)
                                                .label("安装并启动")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.install_windows_service();
                                                    cx.notify();
                                                })),
                                        )
                                    })
                                    .when(self.service_status.state == ServiceState::Stopped && !core_missing, |el| {
                                        el.child(
                                            Button::new("svc-start")
                                                .primary()
                                                .small()
                                                .icon(IconName::Play)
                                                .label("启动服务")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.start_windows_service();
                                                    cx.notify();
                                                })),
                                        )
                                    })
                                    .when(svc_running || self.service_status.state == ServiceState::Paused, |el| {
                                        el.child(
                                            Button::new("svc-restart")
                                                .small()
                                                .icon(IconName::RotateCw)
                                                .label("重启服务")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.restart_windows_service();
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            Button::new("svc-stop")
                                                .danger()
                                                .small()
                                                .icon(IconName::Square)
                                                .label("停止服务")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.stop_windows_service();
                                                    cx.notify();
                                                })),
                                        )
                                    }),
                            ),
                    )
                    .when(svc_not_installed || core_missing, |el| {
                        el.child(
                            div()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.info.opacity(0.14))
                                .border_1()
                                .border_color(theme.info.opacity(0.35))
                                .text_xs()
                                .text_color(theme.info)
                                .child(if svc_not_installed && core_missing {
                                    "请先下载核心，再安装服务。"
                                } else if svc_not_installed {
                                    "安装服务需要管理员权限。"
                                } else {
                                    "找不到核心文件 mihomo.exe。"
                                })
                                .when(core_missing, |inner| {
                                    inner.child(
                                        Button::new("goto-cores-fix")
                                            .xsmall()
                                            .primary()
                                            .label("前往核心管理")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.page = AppPage::Cores;
                                                cx.notify();
                                            })),
                                    )
                                }),
                        )
                    })
                    .when(!self.api_online && !svc_not_installed, |el| {
                        let err = self
                            .api_error
                            .as_deref()
                            .unwrap_or("无法连接到 Mihomo 外部控制接口");
                        el.child(
                            div()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.warning.opacity(0.14))
                                .border_1()
                                .border_color(theme.warning.opacity(0.35))
                                .text_xs()
                                .text_color(theme.warning)
                                .child(format!(
                                    "提示：控制器 http://{} 暂未响应（{err}）。请确认服务已启动或在「应用设置」核对地址与密钥。",
                                    self.settings.controller_addr
                                )),
                        )
                    }),
            )
            // 2. Quick Toggles Row (3 uniform 2-row cards: System Proxy / TUN Mode / Resident Mode)
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_stretch()
                    .gap_3()
                    // System Proxy Card (uniform 2-row layout)
                    .child(
                        card(cx)
                            .flex_1()
                            .min_w_0()
                            .v_flex()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .h_flex()
                                    .h(px(24.))
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .min_w_0()
                                            .child(
                                                Icon::new(IconName::Globe)
                                                    .size_4()
                                                    .text_color(theme.primary),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .whitespace_nowrap()
                                                    .child("系统代理"),
                                            )
                                            .child(pill_badge(
                                                proxy_badge_label,
                                                proxy_state_color,
                                            )),
                                    )
                                    .child(
                                        Switch::new("dash-sys-proxy")
                                            .checked(self.settings.system_proxy_managed)
                                            .on_change(cx.listener(|this, checked, _, cx| {
                                                this.toggle_system_proxy(*checked);
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .h(px(22.))
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_xs()
                                            .text_color(
                                                if proxy_takeover_state
                                                    == SystemProxyTakeoverState::ManagedIneffective
                                                {
                                                    theme.warning
                                                } else {
                                                    theme.muted_foreground
                                                },
                                            )
                                            .child(proxy_sub),
                                    )
                                    .when(
                                        proxy_takeover_state
                                            == SystemProxyTakeoverState::ManagedIneffective,
                                        |el| {
                                            el.child(
                                                Button::new("dash-proxy-reapply")
                                                    .xsmall()
                                                    .primary()
                                                    .label("重新生效")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.reapply_system_proxy();
                                                        cx.notify();
                                                    })),
                                            )
                                        },
                                    ),
                            ),
                    )
                    // TUN Mode Card (uniform 2-row layout)
                    .child(
                        card(cx)
                            .flex_1()
                            .min_w_0()
                            .v_flex()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .h_flex()
                                    .h(px(24.))
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .min_w_0()
                                            .child(
                                                Icon::new(IconName::ShieldCheck)
                                                    .size_4()
                                                    .text_color(theme.success),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .whitespace_nowrap()
                                                    .child("TUN 模式"),
                                            )
                                            .child(pill_badge(
                                                if tun_active { "已启用" } else { "未启用" },
                                                if tun_active {
                                                    theme.success
                                                } else {
                                                    theme.muted_foreground
                                                },
                                            )),
                                    )
                                    .child(
                                        Switch::new("dash-tun-mode")
                                            .checked(self.settings.tun_enabled)
                                            .on_change(cx.listener(|this, checked, _, cx| {
                                                this.toggle_tun_mode(*checked);
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .when_some(tun_sub, |el, detail| {
                                el.child(div().text_xs().text_color(theme.muted_foreground).child(detail))
                            }),
                    )
                    // Resident Mode Card (uniform 2-row layout; controls close-to-tray + auto-start)
                    .child(
                        card(cx)
                            .flex_1()
                            .min_w_0()
                            .v_flex()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .h_flex()
                                    .h(px(24.))
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .min_w_0()
                                            .child(
                                                Icon::new(IconName::Pin)
                                                    .size_4()
                                                    .text_color(theme.info),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .whitespace_nowrap()
                                                    .child("常驻模式"),
                                            ),
                                    )
                                    .child(
                                        Switch::new("dash-resident-mode")
                                            .checked(self.settings.resident_mode)
                                            .on_change(cx.listener(|this, checked, _, cx| {
                                                this.toggle_resident_mode(*checked);
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    ),
            )
            // 3. Proxy Mode & Live Stats Row
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        card(cx)
                            .flex_1()
                            .min_w(px(300.))
                            .v_flex()
                            .gap_3()
                            .child(section_title("代理模式", cx))
                            .child(
                                div()
                                    .h_flex()
                                    .gap_2()
                                    .children(
                                        [
                                            ("rule", "规则", IconName::GitBranch),
                                            ("global", "全局", IconName::Globe),
                                            ("direct", "直连", IconName::ArrowRight),
                                        ]
                                        .into_iter()
                                        .map(|(mode_key, label, icon)| {
                                            let active = current_mode == mode_key;
                                            let mut btn = Button::new(SharedString::from(format!(
                                                "mode-{mode_key}"
                                            )))
                                            .small()
                                            .icon(icon)
                                            .disabled(!self.api_online)
                                            .label(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.set_proxy_mode(mode_key);
                                                cx.notify();
                                            }));
                                            if active {
                                                btn = btn.primary();
                                            }
                                            btn
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        card(cx)
                            .flex_1()
                            .min_w(px(300.))
                            .v_flex()
                            .gap_2()
                            .child(section_title("运行统计", cx))
                            .child(
                                div()
                                    .h_flex()
                                    .justify_between()
                                    .pt_1()
                                    .child(stat_cell("活跃连接", format!("{active_conns}"), theme.primary, cx))
                                    .child(stat_cell("累计下载", format_bytes(total_down), theme.success, cx))
                                    .child(stat_cell("累计上传", format_bytes(total_up), theme.info, cx))
                                    .child(stat_cell("核心内存", format_bytes(mem_inuse), theme.warning, cx)),
                            ),
                    ),
            )
            // 4. Live Traffic Sparkline Card
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .child(section_title("实时速率", cx))
                            .child(
                                div()
                                    .h_flex()
                                    .gap_4()
                                    .text_sm()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1()
                                            .text_color(theme.success)
                                            .font_semibold()
                                            .child(format!("↓ {}", format_speed(down))),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1()
                                            .text_color(theme.info)
                                            .font_semibold()
                                            .child(format!("↑ {}", format_speed(up))),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_end()
                            .gap(px(2.))
                            .h(px(88.))
                            .w_full()
                            .p_2()
                            .rounded_md()
                            .bg(theme.background.opacity(0.6))
                            .border_1()
                            .border_color(theme.border)
                            .children((0..60usize).map(|idx| {
                                let offset = 60usize.saturating_sub(history.len());
                                let pt = if idx >= offset {
                                    history.get(idx - offset).copied().unwrap_or_default()
                                } else {
                                    Default::default()
                                };
                                let down_h = ((pt.down as f32 / max_speed as f32) * 68.0)
                                    .clamp(2.0, 68.0);
                                let up_h = ((pt.up as f32 / max_speed as f32) * 68.0)
                                    .clamp(1.0, 68.0);
                                div()
                                    .flex_1()
                                    .h_full()
                                    .h_flex()
                                    .items_end()
                                    .gap(px(1.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .h(px(down_h))
                                            .rounded_t(px(2.))
                                            .bg(theme.success.opacity(0.75)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .h(px(up_h))
                                            .rounded_t(px(2.))
                                            .bg(theme.info.opacity(0.75)),
                                    )
                            })),
                    ),
            )
            // 5. Active Profile Summary Card
            .child(
                card(cx)
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(section_header(
                                format!(
                                    "当前配置: {}",
                                    active_profile
                                        .map(|p| p.name.as_str())
                                        .unwrap_or("直接读取服务配置文件")
                                ),
                                self.settings.active_config_path.clone(),
                                cx,
                            ))
                            .when_some(
                                active_profile.and_then(|p| p.subscription_info.as_ref()),
                                |el, sub| {
                                    el.child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!(
                                                "已用流量: {} / {} ({:.1}%)",
                                                format_bytes(sub.used()),
                                                format_bytes(sub.total),
                                                sub.usage_ratio() * 100.0
                                            )),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                Button::new("goto-proxies")
                                    .small()
                                    .icon(IconName::Globe)
                                    .label("切换节点")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.page = AppPage::Proxies;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("goto-profiles")
                                    .small()
                                    .icon(IconName::FileCode)
                                    .label("管理配置")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.page = AppPage::Profiles;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    pub fn render_proxies_page(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let is_dark = self.settings.dark_mode;

        // Refined palette for proxy cards & controls (avoids dull gray and harsh black fills in Light mode)
        let accent: Hsla = if is_dark {
            rgb(0x60a5fa).into()
        } else {
            rgb(0x2563eb).into()
        };
        let accent_hover: Hsla = if is_dark {
            rgb(0x3b82f6).into()
        } else {
            rgb(0x1d4ed8).into()
        };
        let card_bg: Hsla = if is_dark {
            rgb(0x16161a).into()
        } else {
            rgb(0xffffff).into()
        };
        let card_hover_bg: Hsla = if is_dark {
            rgb(0x1d1d23).into()
        } else {
            rgb(0xf8fafc).into()
        };
        let card_selected_bg: Hsla = if is_dark {
            hsla(217.0 / 360.0, 0.85, 0.56, 0.14)
        } else {
            rgb(0xeff6ff).into()
        };
        let card_border: Hsla = if is_dark {
            rgb(0x27272c).into()
        } else {
            rgb(0xe5e7eb).into()
        };
        let subtle_surface: Hsla = if is_dark {
            rgb(0x141418).into()
        } else {
            rgb(0xf4f6f9).into()
        };
        let chip_bg: Hsla = if is_dark {
            rgb(0x232329).into()
        } else {
            rgb(0xf1f5f9).into()
        };
        let white: Hsla = rgb(0xffffff).into();

        // Responsive column count based on viewport width so cards fill 100% width with no dead margin
        let avail_w = (f32::from(window.viewport_size().width) - 260.0).max(400.0);
        let grid_cols = ((avail_w + 12.0) / 216.0).floor().clamp(2.0, 5.0) as u16;

        let groups = self.sorted_proxy_groups();
        let active_group_name = self
            .selected_group
            .clone()
            .filter(|g| groups.contains(g))
            .or_else(|| groups.first().cloned());

        let search_kw = self.proxy_search_input.read(cx).value().to_lowercase();

        let active_group = active_group_name
            .as_ref()
            .and_then(|name| self.proxies.get(name));

        let mut node_names: Vec<String> = active_group
            .and_then(|g| g.all.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|n| {
                if search_kw.is_empty() {
                    return true;
                }
                if n.to_lowercase().contains(&search_kw) {
                    return true;
                }
                // Also allow searching by resolved country code or Chinese country name
                if let Some(geo) = self.resolve_node_geo(n) {
                    match &geo.region {
                        GeoRegion::Country(cc) => {
                            if cc.to_lowercase().contains(&search_kw) {
                                return true;
                            }
                            if let Some(zh) = flags::country_name_zh(cc) {
                                if zh.to_lowercase().contains(&search_kw) {
                                    return true;
                                }
                            }
                        }
                        GeoRegion::Private => {
                            if "局域网".contains(&search_kw) || "lan".contains(&search_kw) {
                                return true;
                            }
                        }
                    }
                }
                false
            })
            .collect();

        match self.proxy_sort {
            ProxySort::Default => {}
            ProxySort::NameAsc => node_names.sort(),
            ProxySort::LatencyAsc => {
                node_names.sort_by_key(|n| {
                    let d = self
                        .proxy_delays
                        .get(n)
                        .copied()
                        .or_else(|| self.proxies.get(n).and_then(|p| p.latest_delay()));
                    match d {
                        Some(0) | None => u32::MAX,
                        Some(ms) => ms,
                    }
                });
            }
        }

        div()
            .v_flex()
            .size_full()
            .gap_3()
            // 1. Top Search & Segmented Sort Controls
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2p5()
                    .flex_wrap()
                    .child(
                        div()
                            .w(px(270.))
                            .child(Input::new(&self.proxy_search_input).cleanable(true).small()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                // Segmented control track
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(2.))
                                    .p(px(3.))
                                    .rounded_lg()
                                    .bg(subtle_surface)
                                    .border_1()
                                    .border_color(card_border)
                                    .children(
                                        [
                                            (ProxySort::Default, "默认顺序"),
                                            (ProxySort::LatencyAsc, "延迟升序"),
                                            (ProxySort::NameAsc, "名称排序"),
                                        ]
                                        .into_iter()
                                        .map(|(sort, lbl)| {
                                            let active = self.proxy_sort == sort;
                                            div()
                                                .id(SharedString::from(format!("sort-{sort:?}")))
                                                .px_2p5()
                                                .py(px(4.))
                                                .rounded_md()
                                                .cursor_pointer()
                                                .text_xs()
                                                .when(active, |el| {
                                                    el.bg(card_bg)
                                                        .border_1()
                                                        .border_color(card_border)
                                                        .text_color(theme.foreground)
                                                        .font_semibold()
                                                })
                                                .when(!active, |el| {
                                                    el.border_1()
                                                        .border_color(transparent_black())
                                                        .text_color(theme.muted_foreground)
                                                        .hover(|s| s.text_color(theme.foreground))
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.proxy_sort = sort;
                                                    cx.notify();
                                                }))
                                                .child(lbl)
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("proxies-refresh")
                                    .h_flex()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2p5()
                                    .py(px(6.))
                                    .rounded_lg()
                                    .bg(card_bg)
                                    .border_1()
                                    .border_color(card_border)
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.foreground)
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(card_hover_bg)
                                            .border_color(accent.opacity(0.45))
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.trigger_full_refresh();
                                        this.maybe_trigger_geo_resolve(true);
                                        cx.notify();
                                    }))
                                    .child(Icon::new(IconName::RefreshCw).size_3p5())
                                    .child("刷新"),
                            ),
                    ),
            )
            // 2. Group Tabs Strip (polished pills with mini flags for active nodes)
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap_2()
                    .children(groups.iter().map(|gname| {
                        let is_sel = active_group_name.as_deref() == Some(gname.as_str());
                        let now_label = self
                            .proxies
                            .get(gname)
                            .and_then(|g| g.now.clone())
                            .unwrap_or_default();
                        let now_item = self.proxies.get(&now_label);
                        let now_proto = now_item
                            .map(|p| p.proxy_type.as_str())
                            .unwrap_or("");
                        let now_is_group = now_item.is_some_and(|p| p.is_group());
                        let now_geo = self.resolve_node_geo(&now_label);
                        let gname_owned = gname.clone();

                        div()
                            .id(SharedString::from(format!("grp-tab-{gname}")))
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1p5()
                            .rounded_lg()
                            .cursor_pointer()
                            .border_1()
                            .text_xs()
                            .when(is_sel, |el| {
                                el.bg(accent)
                                    .border_color(accent)
                                    .text_color(white)
                                    .font_semibold()
                            })
                            .when(!is_sel, |el| {
                                el.bg(card_bg)
                                    .border_color(card_border)
                                    .text_color(theme.foreground)
                                    .font_medium()
                                    .hover(|s| {
                                        s.bg(card_hover_bg)
                                            .border_color(accent.opacity(0.4))
                                    })
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_group = Some(gname_owned.clone());
                                cx.notify();
                            }))
                            .child(gname.clone())
                            .when(!now_label.is_empty(), |el| {
                                el.child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap_1()
                                        .px_1p5()
                                        .py(px(1.))
                                        .rounded_md()
                                        .bg(if is_sel {
                                            white.opacity(0.18)
                                        } else {
                                            chip_bg
                                        })
                                        .child(render_node_flag_badge(
                                            &now_label,
                                            now_proto,
                                            now_is_group,
                                            now_geo,
                                            is_dark,
                                            cx,
                                            true,
                                        ))
                                        .child(
                                            div()
                                                .text_color(if is_sel {
                                                    white.opacity(0.95)
                                                } else {
                                                    theme.muted_foreground
                                                })
                                                .font_normal()
                                                .max_w(px(92.))
                                                .truncate()
                                                .child(now_label),
                                        ),
                                )
                            })
                    })),
            )
            // 3. Active Group Header & Responsive Node Card Grid
            .child(if let Some(group) = active_group {
                let gname = group.name.clone();
                let gtype = group.proxy_type.clone();
                let current_now = group.now.clone().unwrap_or_default();
                let current_now_item = self.proxies.get(&current_now);
                let current_now_proto = current_now_item
                    .map(|p| p.proxy_type.as_str())
                    .unwrap_or("");
                let current_now_is_group = current_now_item.is_some_and(|p| p.is_group());
                let current_now_geo = self.resolve_node_geo(&current_now);
                let can_select = gtype == "Selector" || gtype == "URLTest" || gtype == "Fallback";
                let can_unfix = gtype == "URLTest" || gtype == "Fallback";

                div()
                    .v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    // Active Group Summary Bar
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .flex_wrap()
                            .gap_2()
                            .px_4()
                            .py_2p5()
                            .rounded_xl()
                            .bg(subtle_surface)
                            .border_1()
                            .border_color(card_border)
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .flex_wrap()
                                    .gap_2p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_bold()
                                            .text_color(theme.foreground)
                                            .child(gname.clone()),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py(px(2.))
                                            .rounded_full()
                                            .bg(accent.opacity(0.12))
                                            .border_1()
                                            .border_color(accent.opacity(0.28))
                                            .text_size(px(11.))
                                            .font_semibold()
                                            .text_color(accent)
                                            .child(gtype.clone()),
                                    )
                                    .child(
                                        div()
                                            .w(px(1.))
                                            .h(px(14.))
                                            .bg(card_border),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1p5()
                                            .text_xs()
                                            .child(
                                                div()
                                                    .text_color(theme.muted_foreground)
                                                    .child("当前"),
                                            )
                                            .when(!current_now.is_empty(), |el| {
                                                el.child(render_node_flag_badge(
                                                    &current_now,
                                                    current_now_proto,
                                                    current_now_is_group,
                                                    current_now_geo,
                                                    is_dark,
                                                    cx,
                                                    true,
                                                ))
                                            })
                                            .child(
                                                div()
                                                    .font_semibold()
                                                    .text_color(theme.foreground)
                                                    .child(if current_now.is_empty() {
                                                        "-".to_string()
                                                    } else {
                                                        current_now.clone()
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!("·  {} 个节点", node_names.len())),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .when(can_unfix, |el| {
                                        let g_unfix = gname.clone();
                                        el.child(
                                            div()
                                                .id("unfix-group")
                                                .px_2p5()
                                                .py_1()
                                                .rounded_lg()
                                                .bg(card_bg)
                                                .border_1()
                                                .border_color(card_border)
                                                .text_xs()
                                                .font_medium()
                                                .text_color(theme.foreground)
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(card_hover_bg)
                                                        .border_color(accent.opacity(0.45))
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.unfix_proxy_group(g_unfix.clone());
                                                    cx.notify();
                                                }))
                                                .child("恢复自动"),
                                        )
                                    })
                                    .child({
                                        let g_test = gname.clone();
                                        div()
                                            .id("test-group-delay")
                                            .h_flex()
                                            .items_center()
                                            .gap_1p5()
                                            .px_3()
                                            .py_1()
                                            .rounded_lg()
                                            .bg(accent)
                                            .text_color(white)
                                            .text_xs()
                                            .font_semibold()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(accent_hover))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.test_group_delay(g_test.clone());
                                                cx.notify();
                                            }))
                                            .child(Icon::new(IconName::Zap).size_3p5())
                                            .child("全组测速")
                                    }),
                            ),
                    )
                    // Responsive Full-Width Node Cards Grid
                    .child(
                        div()
                            .id("proxies-grid-scroll")
                            .flex_1()
                            .min_h_0()
                            .pr_2()
                            .overflow_y_scrollbar()
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(grid_cols)
                                    .gap_3()
                                    .w_full()
                                    .pb_2()
                                    .children(node_names.into_iter().enumerate().map(|(i, node)| {
                                        let is_current = node == current_now;
                                        let item = self.proxies.get(&node);
                                        let raw_proto = item
                                            .map(|p| p.proxy_type.as_str())
                                            .unwrap_or("Proxy");
                                        let proto_label = short_protocol_label(raw_proto);
                                        let is_group = item.is_some_and(|p| p.is_group());
                                        let udp = item.is_some_and(|p| p.udp);
                                        let geo = self.resolve_node_geo(&node);
                                        let region_label =
                                            region_short_label(&node, raw_proto, geo);
                                        let delay = self
                                            .proxy_delays
                                            .get(&node)
                                            .copied()
                                            .or_else(|| item.and_then(|p| p.latest_delay()));
                                        let d_color = delay_color(delay, cx);
                                        let d_text = delay_text(delay);
                                        let has_delay = delay.is_some();
                                        let g_click = gname.clone();
                                        let n_click = node.clone();
                                        let n_test = node.clone();

                                        div()
                                            .id(SharedString::from(format!("node-{i}-{node}")))
                                            .relative()
                                            .v_flex()
                                            .justify_between()
                                            .gap_3()
                                            .min_w_0()
                                            .min_h(px(84.))
                                            .px_3p5()
                                            .py_3()
                                            .rounded_xl()
                                            .border_1()
                                            .overflow_hidden()
                                            .cursor_pointer()
                                            .when(is_current, |el| {
                                                el.bg(card_selected_bg)
                                                    .border_color(accent.opacity(0.75))
                                            })
                                            .when(!is_current, |el| {
                                                el.bg(card_bg)
                                                    .border_color(card_border)
                                                    .hover(|s| {
                                                        s.bg(card_hover_bg)
                                                            .border_color(accent.opacity(0.45))
                                                    })
                                            })
                                            .when(can_select, |el| {
                                                el.on_click(cx.listener(move |this, _, _, cx| {
                                                    this.select_proxy(
                                                        g_click.clone(),
                                                        n_click.clone(),
                                                    );
                                                    cx.notify();
                                                }))
                                            })
                                            // Sleek left accent bar for the active node
                                            .when(is_current, |el| {
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .left_0()
                                                        .top_2p5()
                                                        .bottom_2p5()
                                                        .w(px(3.))
                                                        .rounded_r_full()
                                                        .bg(accent),
                                                )
                                            })
                                            // Card Top Row: Flag + Node Name + Selected Check Circle
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap_2()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .h_flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .child(render_node_flag_badge(
                                                                &node,
                                                                raw_proto,
                                                                is_group,
                                                                geo,
                                                                is_dark,
                                                                cx,
                                                                false,
                                                            ))
                                                            .child(
                                                                div()
                                                                    .flex_1()
                                                                    .min_w_0()
                                                                    .text_sm()
                                                                    .font_semibold()
                                                                    .truncate()
                                                                    .text_color(if is_current {
                                                                        accent
                                                                    } else {
                                                                        theme.foreground
                                                                    })
                                                                    .child(node.clone()),
                                                            ),
                                                    )
                                                    .when(is_current, |el| {
                                                        el.child(
                                                            div()
                                                                .h_flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .flex_shrink_0()
                                                                .size(px(18.))
                                                                .rounded_full()
                                                                .bg(accent)
                                                                .child(
                                                                    Icon::new(IconName::Check)
                                                                        .size(px(11.))
                                                                        .text_color(white),
                                                                ),
                                                        )
                                                    }),
                                            )
                                            // Card Bottom Row: Protocol / UDP / Region | Latency Pill
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap_2()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .h_flex()
                                                            .items_center()
                                                            .gap_1p5()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .overflow_hidden()
                                                            .child(
                                                                div()
                                                                    .flex_shrink_0()
                                                                    .px_1p5()
                                                                    .py(px(1.5))
                                                                    .rounded(px(5.))
                                                                    .bg(if is_current {
                                                                        accent.opacity(0.12)
                                                                    } else {
                                                                        chip_bg
                                                                    })
                                                                    .text_size(px(10.5))
                                                                    .font_medium()
                                                                    .text_color(if is_current {
                                                                        accent.opacity(0.9)
                                                                    } else {
                                                                        theme.muted_foreground
                                                                    })
                                                                    .child(proto_label),
                                                            )
                                                            .when(udp, |el| {
                                                                el.child(
                                                                    div()
                                                                        .flex_shrink_0()
                                                                        .px_1()
                                                                        .py(px(1.5))
                                                                        .rounded(px(5.))
                                                                        .bg(theme.info.opacity(0.12))
                                                                        .text_size(px(10.))
                                                                        .font_semibold()
                                                                        .text_color(theme.info)
                                                                        .child("UDP"),
                                                                )
                                                            })
                                                            .when_some(region_label, |el, lbl| {
                                                                el.child(
                                                                    div()
                                                                        .flex_1()
                                                                        .min_w_0()
                                                                        .truncate()
                                                                        .text_size(px(11.))
                                                                        .text_color(
                                                                            theme.muted_foreground,
                                                                        )
                                                                        .child(lbl),
                                                                )
                                                            }),
                                                    )
                                                    .child(
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "delay-{i}-{node}"
                                                            )))
                                                            .h_flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .flex_shrink_0()
                                                            .px_2()
                                                            .py(px(2.))
                                                            .rounded_full()
                                                            .bg(d_color.opacity(0.12))
                                                            .border_1()
                                                            .border_color(d_color.opacity(0.24))
                                                            .text_size(px(11.))
                                                            .font_semibold()
                                                            .text_color(d_color)
                                                            .cursor_pointer()
                                                            .hover(|s| {
                                                                s.bg(d_color.opacity(0.22))
                                                                    .border_color(
                                                                        d_color.opacity(0.45),
                                                                    )
                                                            })
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    cx.stop_propagation();
                                                                    this.test_single_proxy_delay(
                                                                        n_test.clone(),
                                                                    );
                                                                    cx.notify();
                                                                },
                                                            ))
                                                            .when(has_delay, |el| {
                                                                el.child(
                                                                    div()
                                                                        .size(px(5.5))
                                                                        .rounded_full()
                                                                        .bg(d_color),
                                                                )
                                                            })
                                                            .child(d_text),
                                                    ),
                                            )
                                    })),
                            ),
                    )
                    .into_any_element()
            } else {
                card(cx)
                    .h_flex()
                    .items_center()
                    .justify_center()
                    .h(px(180.))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("暂无策略组数据，请确认 Mihomo 服务已运行且外部控制器连接正常。")
                    .into_any_element()
            })
    }
}

fn short_protocol_label(proto: &str) -> String {
    match proto {
        "Shadowsocks" => "SS".to_string(),
        "ShadowsocksR" => "SSR".to_string(),
        "Http" => "HTTP".to_string(),
        "Socks5" => "SOCKS5".to_string(),
        "WireGuard" => "WG".to_string(),
        "Direct" => "Direct".to_string(),
        "Reject" => "Reject".to_string(),
        other => other.to_string(),
    }
}

fn region_short_label(node_name: &str, proto: &str, geo: Option<&NodeGeoInfo>) -> Option<String> {
    if proto.eq_ignore_ascii_case("Direct") || node_name.eq_ignore_ascii_case("DIRECT") {
        return Some("直连".to_string());
    }
    if proto.eq_ignore_ascii_case("Reject")
        || node_name.eq_ignore_ascii_case("REJECT")
        || node_name.eq_ignore_ascii_case("REJECT-DROP")
    {
        return Some("拦截".to_string());
    }
    match geo.map(|g| &g.region) {
        Some(GeoRegion::Country(cc)) => {
            Some(flags::country_name_zh(cc).unwrap_or(cc.as_str()).to_string())
        }
        Some(GeoRegion::Private) => Some("局域网".to_string()),
        None => None,
    }
}

fn render_node_flag_badge(
    node_name: &str,
    proto: &str,
    is_group: bool,
    geo: Option<&NodeGeoInfo>,
    is_dark: bool,
    cx: &App,
    compact: bool,
) -> AnyElement {
    let theme = cx.theme();
    let (w, h, rad) = if compact {
        (px(16.), px(12.), px(2.5))
    } else {
        (px(24.), px(17.), px(3.5))
    };
    let flag_border: Hsla = if is_dark {
        hsla(0.0, 0.0, 1.0, 0.16)
    } else {
        hsla(0.0, 0.0, 0.0, 0.12)
    };

    // 1. Built-in DIRECT / REJECT special nodes
    if proto.eq_ignore_ascii_case("Direct") || node_name.eq_ignore_ascii_case("DIRECT") {
        return div()
            .h_flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .w(w)
            .h(h)
            .rounded(rad)
            .bg(theme.success.opacity(0.16))
            .border_1()
            .border_color(theme.success.opacity(0.32))
            .child(
                Icon::new(IconName::ArrowRight)
                    .size(if compact { px(9.) } else { px(12.) })
                    .text_color(theme.success),
            )
            .into_any_element();
    }
    if proto.eq_ignore_ascii_case("Reject")
        || node_name.eq_ignore_ascii_case("REJECT")
        || node_name.eq_ignore_ascii_case("REJECT-DROP")
    {
        return div()
            .h_flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .w(w)
            .h(h)
            .rounded(rad)
            .bg(theme.danger.opacity(0.16))
            .border_1()
            .border_color(theme.danger.opacity(0.32))
            .child(
                Icon::new(IconName::X)
                    .size(if compact { px(9.) } else { px(12.) })
                    .text_color(theme.danger),
            )
            .into_any_element();
    }

    // 2. GeoIP-resolved Country Flag or Private LAN badge
    if let Some(info) = geo {
        match &info.region {
            GeoRegion::Country(cc) => {
                if let Some(flag_img) = flags::flag_image(cc) {
                    return div()
                        .flex_shrink_0()
                        .w(w)
                        .h(h)
                        .rounded(rad)
                        .overflow_hidden()
                        .border_1()
                        .border_color(flag_border)
                        .child(img(flag_img).size_full().object_fit(ObjectFit::Cover))
                        .into_any_element();
                }
                return div()
                    .h_flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .w(w)
                    .h(h)
                    .rounded(rad)
                    .bg(theme.info.opacity(0.15))
                    .border_1()
                    .border_color(theme.info.opacity(0.35))
                    .text_size(if compact { px(8.) } else { px(9.5) })
                    .font_bold()
                    .text_color(theme.info)
                    .child(cc.clone())
                    .into_any_element();
            }
            GeoRegion::Private => {
                return div()
                    .h_flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .w(w)
                    .h(h)
                    .rounded(rad)
                    .bg(theme.info.opacity(0.14))
                    .border_1()
                    .border_color(theme.info.opacity(0.3))
                    .child(
                        Icon::new(IconName::Network)
                            .size(if compact { px(9.) } else { px(12.) })
                            .text_color(theme.info),
                    )
                    .into_any_element();
            }
        }
    }

    // 3. Nested strategy group or unresolved fallback
    let icon = if is_group {
        IconName::GitBranch
    } else {
        IconName::Globe
    };
    div()
        .h_flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(w)
        .h(h)
        .rounded(rad)
        .bg(theme.muted)
        .border_1()
        .border_color(flag_border)
        .child(
            Icon::new(icon)
                .size(if compact { px(9.) } else { px(12.) })
                .text_color(theme.muted_foreground),
        )
        .into_any_element()
}

fn stat_cell(label: &str, value: String, color: Hsla, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .v_flex()
        .gap_0p5()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(div().text_base().font_bold().text_color(color).child(value))
}
