use crate::config;
use crate::ui::app::MihomoApp;
use crate::ui::{ToolsTab, card, pill_badge, section_header, section_title};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::json;

impl MihomoApp {
    pub fn render_tools_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_tab = self.tools_tab;

        div()
            .v_flex()
            .size_full()
            .gap_3()
            // Sub-tab switcher
            .child(
                div()
                    .h_flex()
                    .gap_1p5()
                    .children(
                        [
                            (ToolsTab::Logs, "实时日志 (WS /logs)"),
                            (ToolsTab::ApiActions, "核心管理接口"),
                            (ToolsTab::DnsQuery, "DNS 查询 (/dns/query)"),
                        ]
                        .into_iter()
                        .map(|(tab, label)| {
                            let mut b = Button::new(SharedString::from(format!("ttab-{tab:?}")))
                                .small()
                                .label(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.tools_tab = tab;
                                    cx.notify();
                                }));
                            if active_tab == tab {
                                b = b.primary();
                            }
                            b
                        }),
                    ),
            )
            .child(match active_tab {
                ToolsTab::Logs => self.render_logs_tab(cx).into_any_element(),
                ToolsTab::ApiActions => self.render_api_actions_tab(cx).into_any_element(),
                ToolsTab::DnsQuery => self.render_dns_query_tab(cx).into_any_element(),
            })
    }

    fn render_logs_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let kw = self.log_search_input.read(cx).value().to_lowercase();
        let (logs, paused, current_level) = {
            let t = self.telemetry.lock().unwrap();
            let filtered = t
                .logs
                .iter()
                .rev()
                .filter(|l| {
                    kw.is_empty()
                        || l.payload.to_lowercase().contains(&kw)
                        || l.level.to_lowercase().contains(&kw)
                })
                .take(250)
                .cloned()
                .collect::<Vec<_>>();
            (filtered, t.logs_paused, t.log_level.clone())
        };

        div()
            .v_flex()
            .flex_1()
            .min_h_0()
            .gap_2p5()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        div()
                            .w(px(280.))
                            .child(Input::new(&self.log_search_input).cleanable(true).small()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_1p5()
                            .children(["debug", "info", "warning", "error"].into_iter().map(
                                |lvl| {
                                    let mut b = Button::new(SharedString::from(format!(
                                        "loglvl-{lvl}"
                                    )))
                                    .xsmall()
                                    .label(lvl)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let mut t = this.telemetry.lock().unwrap();
                                        let addr = t.controller_addr.clone();
                                        let sec = t.controller_secret.clone();
                                        t.reconfigure(addr, sec, lvl.to_string());
                                        cx.notify();
                                    }));
                                    if current_level == lvl {
                                        b = b.primary();
                                    }
                                    b
                                },
                            ))
                            .child(
                                Button::new("log-pause")
                                    .xsmall()
                                    .icon(if paused {
                                        IconName::Play
                                    } else {
                                        IconName::Pause
                                    })
                                    .label(if paused { "继续" } else { "暂停" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let mut t = this.telemetry.lock().unwrap();
                                        t.logs_paused = !t.logs_paused;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("log-clear")
                                    .xsmall()
                                    .icon(IconName::Trash)
                                    .label("清空")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.telemetry.lock().unwrap().logs.clear();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("logs-scroll")
                    .flex_1()
                    .min_h_0()
                    .p_3()
                    .rounded_lg()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .font_family("Consolas")
                            .text_xs()
                            .when(logs.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_color(theme.muted_foreground)
                                        .child("等待 Mihomo /logs WebSocket 日志流..."),
                                )
                            })
                            .children(logs.into_iter().map(|entry| {
                                let lvl_color = match entry.level.as_str() {
                                    "error" => theme.danger,
                                    "warning" => theme.warning,
                                    "debug" => theme.muted_foreground,
                                    _ => theme.info,
                                };
                                div()
                                    .h_flex()
                                    .gap_2()
                                    .items_start()
                                    .child(
                                        div()
                                            .w(px(58.))
                                            .flex_shrink_0()
                                            .font_semibold()
                                            .text_color(lvl_color)
                                            .child(format!("[{}]", entry.level.to_uppercase())),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_color(theme.foreground)
                                            .child(entry.payload),
                                    )
                            })),
                    ),
            )
    }

    fn render_api_actions_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = self.mihomo_config.clone().unwrap_or_default();

        div()
            .id("api-actions-scroll")
            .v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .pr_2()
            .overflow_y_scrollbar()
            // Management Actions Grid
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(section_title("核心操作", cx))
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .gap_2()
                            .children(
                                [
                                    ("reload", "热重载配置 (PUT /configs)", IconName::RefreshCw),
                                    ("geo", "更新 GEO 数据库 (POST /configs/geo)", IconName::Database),
                                    ("fakeip", "清空 FakeIP 缓存", IconName::Eraser),
                                    ("dns", "清空 DNS 缓存", IconName::Trash),
                                    ("restart", "API 重启内核 (POST /restart)", IconName::RotateCw),
                                    ("upgrade", "API 升级内核 (POST /upgrade)", IconName::CloudDownload),
                                    ("upgrade_ui", "升级外部 UI (POST /upgrade/ui)", IconName::LayoutGrid),
                                ]
                                .into_iter()
                                .map(|(key, label, icon)| {
                                    Button::new(SharedString::from(format!("api-act-{key}")))
                                        .small()
                                        .icon(icon)
                                        .label(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.run_core_management_action(key);
                                            cx.notify();
                                        }))
                                }),
                            ),
                    ),
            )
            // Runtime PATCH /configs switches
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(section_header("运行参数", "修改后立即生效。", cx))
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .gap_6()
                            .child(
                                Switch::new("patch-allow-lan")
                                    .checked(cfg.allow_lan)
                                    .label("允许局域网 (allow-lan)")
                                    .on_change(cx.listener(|this, v, _, cx| {
                                        this.patch_runtime_config(
                                            "允许局域网",
                                            json!({ "allow-lan": *v }),
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Switch::new("patch-ipv6")
                                    .checked(cfg.ipv6)
                                    .label("IPv6 支持")
                                    .on_change(cx.listener(|this, v, _, cx| {
                                        this.patch_runtime_config("IPv6", json!({ "ipv6": *v }));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Switch::new("patch-tcp-concurrent")
                                    .checked(cfg.tcp_concurrent)
                                    .label("TCP 并发 (tcp-concurrent)")
                                    .on_change(cx.listener(|this, v, _, cx| {
                                        this.patch_runtime_config(
                                            "TCP 并发",
                                            json!({ "tcp-concurrent": *v }),
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Switch::new("patch-unified-delay")
                                    .checked(cfg.unified_delay)
                                    .label("统一延迟 (unified-delay)")
                                    .on_change(cx.listener(|this, v, _, cx| {
                                        this.patch_runtime_config(
                                            "统一延迟",
                                            json!({ "unified-delay": *v }),
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Switch::new("patch-sniffing")
                                    .checked(cfg.sniffing)
                                    .label("域名嗅探 (sniffing)")
                                    .on_change(cx.listener(|this, v, _, cx| {
                                        this.patch_runtime_config(
                                            "域名嗅探",
                                            json!({ "sniffing": *v }),
                                        );
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .pt_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("进程匹配模式 (find-process-mode):"),
                            )
                            .children(["strict", "always", "off"].into_iter().map(|m| {
                                let mut b = Button::new(SharedString::from(format!("fpm-{m}")))
                                    .xsmall()
                                    .label(m)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.patch_runtime_config(
                                            "进程匹配模式",
                                            json!({ "find-process-mode": m }),
                                        );
                                        cx.notify();
                                    }));
                                if cfg.find_process_mode.eq_ignore_ascii_case(m) {
                                    b = b.primary();
                                }
                                b
                            })),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("内核日志级别 (log-level):"),
                            )
                            .children(["debug", "info", "warning", "error", "silent"].into_iter().map(
                                |lvl| {
                                    let mut b = Button::new(SharedString::from(format!(
                                        "cfg-lvl-{lvl}"
                                    )))
                                    .xsmall()
                                    .label(lvl)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.patch_runtime_config(
                                            "内核日志级别",
                                            json!({ "log-level": lvl }),
                                        );
                                        cx.notify();
                                    }));
                                    if cfg.log_level.eq_ignore_ascii_case(lvl) {
                                        b = b.primary();
                                    }
                                    b
                                },
                            )),
                    ),
            )
    }

    fn render_dns_query_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .v_flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(section_title("DNS 查询", cx))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(260.))
                                    .child(Input::new(&self.dns_domain_input).cleanable(true).small()),
                            )
                            .children(["A", "AAAA", "CNAME", "TXT", "MX", "HTTPS"].into_iter().map(
                                |rt| {
                                    let mut b = Button::new(SharedString::from(format!("dnst-{rt}")))
                                        .xsmall()
                                        .label(rt)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.dns_record_type = rt.to_string();
                                            cx.notify();
                                        }));
                                    if self.dns_record_type == rt {
                                        b = b.primary();
                                    }
                                    b
                                },
                            ))
                            .child(
                                Button::new("dns-query-go")
                                    .primary()
                                    .small()
                                    .icon(IconName::Search)
                                    .label("查询")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.query_dns(cx);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .when_some(self.dns_result.clone(), |el, resp| {
                let answers = resp.answer.unwrap_or_default();
                el.child(
                    card(cx)
                        .v_flex()
                        .gap_2()
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_2()
                                .child(div().text_sm().font_semibold().child("查询结果"))
                                .child(pill_badge(
                                    format!("Status: {}", resp.status),
                                    if resp.status == 0 {
                                        theme.success
                                    } else {
                                        theme.warning
                                    },
                                ))
                                .child(pill_badge(
                                    format!("{} 条记录", answers.len()),
                                    theme.info,
                                )),
                        )
                        .when(answers.is_empty(), |el| {
                            el.child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("无 Answer 记录返回"),
                            )
                        })
                        .children(answers.into_iter().map(|a| {
                            div()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .bg(theme.background.opacity(0.6))
                                .border_1()
                                .border_color(theme.border)
                                .text_xs()
                                .font_family("Consolas")
                                .child(format!("{}  (type={}, TTL={}s)", a.name, a.record_type, a.ttl))
                                .child(
                                    div()
                                        .font_semibold()
                                        .text_color(theme.primary)
                                        .child(a.data),
                                )
                        })),
                )
            })
    }

    pub fn render_settings_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("settings-scroll")
            .v_flex()
            .size_full()
            .gap_4()
            .pr_2()
            .overflow_y_scrollbar()
            // Windows Service Settings
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
                            .gap_2()
                            .child(section_title("Windows 服务配置", cx))
                            .child(
                                div()
                                    .h_flex()
                                    .gap_1p5()
                                    .child(
                                        Button::new("auto-discover-svc")
                                            .small()
                                            .icon(IconName::Sparkles)
                                            .label("自动探测")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.auto_discover_service(window, cx);
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new("settings-install-svc")
                                            .primary()
                                            .small()
                                            .icon(IconName::Wrench)
                                            .label("安装 / 修复服务")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.install_windows_service();
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new("settings-uninstall-svc")
                                            .danger()
                                            .small()
                                            .icon(IconName::Trash)
                                            .label("卸载服务")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.uninstall_windows_service();
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    .child(labeled_input("Windows 服务名", &self.setting_service_name, cx))
                    .child(labeled_input("Mihomo 核心路径 (mihomo.exe)", &self.setting_core_path, cx))
                    .child(labeled_input("工作目录 (-d)", &self.setting_work_dir, cx))
                    .child(labeled_input("服务生效配置文件 (-f)", &self.setting_config_path, cx)),
            )
            // Appearance & Theme Card
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(section_title("主题", cx))
                    .child(
                        Switch::new("setting-dark-mode")
                            .checked(self.settings.dark_mode)
                            .label(if self.settings.dark_mode {
                                "暗黑模式（当前：暗黑）"
                            } else {
                                "暗黑模式（当前：明亮）"
                            })
                            .on_change(cx.listener(|this, v, window, cx| {
                                this.set_dark_mode(*v, window, cx);
                                cx.notify();
                            })),
                    ),
            )
            // Resident Mode Card (controls close-to-tray + auto-start)
            .child(
                card(cx)
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(section_header("常驻模式", "登录 Windows 时启动，关闭窗口时收至托盘。", cx))
                    .child(
                        Switch::new("setting-resident-mode")
                            .checked(self.settings.resident_mode)
                            .on_change(cx.listener(|this, v, _, cx| {
                                this.toggle_resident_mode(*v);
                                cx.notify();
                            })),
                    ),
            )
            // Controller & Proxy Settings
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(section_title("控制器与系统代理", cx))
                    .child(labeled_input("外部控制器地址 (external-controller)", &self.setting_controller_addr, cx))
                    .child(labeled_input("访问密钥 (secret)", &self.setting_controller_secret, cx))
                    .child(labeled_input("混合代理端口 (mixed-port)", &self.setting_mixed_port, cx))
                    .child(labeled_input("系统代理绕过列表 (ProxyOverride)", &self.setting_proxy_bypass, cx))
                    .child(labeled_input("节点测速 URL", &self.setting_test_url, cx)),
            )
            // Action Bar
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("应用数据目录: {}", config::app_data_dir().display())),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                Button::new("open-data-dir")
                                    .small()
                                    .icon(IconName::FolderOpen)
                                    .label("打开数据目录")
                                    .on_click(cx.listener(|_, _, _, _| {
                                        let dir = config::app_data_dir();
                                        let _ = std::process::Command::new("explorer")
                                            .arg(dir)
                                            .spawn();
                                    })),
                            )
                            .child(
                                Button::new("save-settings")
                                    .primary()
                                    .small()
                                    .icon(IconName::Save)
                                    .label("保存设置")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.save_all_settings(cx);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }
}

fn labeled_input(
    label: &str,
    state: &Entity<gpui_kit::component::input::InputState>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    div()
        .v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(Input::new(state).small())
}
