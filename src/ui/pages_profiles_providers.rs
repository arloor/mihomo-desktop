use crate::config::{self, format_bytes};
use crate::ui::app::MihomoApp;
use crate::ui::{ProvidersTab, card, delay_color, delay_text, pill_badge, section_header, section_title};

use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl MihomoApp {
    pub fn render_profiles_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let active_id = self.settings.active_profile_id.clone();

        div()
            .id("profiles-scroll")
            .v_flex()
            .size_full()
            .gap_4()
            .pr_2()
            .overflow_y_scrollbar()
            // Import from URL card
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(section_title("导入配置", cx))
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .child(
                                div()
                                    .w(px(220.))
                                    .child(Input::new(&self.profile_name_input).cleanable(true).small()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(280.))
                                    .child(Input::new(&self.profile_url_input).cleanable(true).small()),
                            )
                            .child(
                                Button::new("import-url-btn")
                                    .disabled(self.profile_url_input.read(cx).value().trim().is_empty())
                                    .primary()
                                    .small()
                                    .icon(IconName::Download)
                                    .label("从 URL 导入")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_profile_from_url(cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(300.))
                                    .child(Input::new(&self.local_import_input).cleanable(true).small()),
                            )
                            .child(
                                Button::new("import-local-btn")
                                    .disabled(self.local_import_input.read(cx).value().trim().is_empty())
                                    .small()
                                    .icon(IconName::FolderOpen)
                                    .label("导入本地 YAML")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_profile_from_local_path(cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("sync-svc-cfg-btn")
                                    .small()
                                    .icon(IconName::Copy)
                                    .label("同步当前服务配置")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sync_current_service_config_as_profile();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            // Default Merge Policy Card
            .child(
                card(cx)
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_3()
                    .child(section_header("默认配置", "导入时自动补全缺失项。", cx))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Switch::new("force-default-tun-dns")
                                    .checked(self.settings.force_default_tun_dns)
                                    .label("强制覆盖原配置 TUN/DNS")
                                    .on_change(cx.listener(|this, checked, _, cx| {
                                        this.settings.force_default_tun_dns = *checked;
                                        let _ = config::save_settings(&this.settings);
                                        this.set_notice(
                                            false,
                                            if *checked {
                                                "已开启：切换配置或开启 TUN 时将强制使用内置默认 TUN/DNS"
                                            } else {
                                                "已恢复默认：仅当配置缺失对应字段时自动补全"
                                            },
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("view-default-tun-dns")
                                    .small()
                                    .icon(IconName::Eye)
                                    .label("查看内置默认配置")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.preview_yaml = Some((
                                            "内置默认合并配置（用户未覆盖时生效）".to_string(),
                                            config::default_merge_config_preview(),
                                        ));
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            // Profiles List
            .child(
                div()
                    .v_flex()
                    .gap_2p5()
                    .children(self.profiles.profiles.iter().map(|profile| {
                        let is_active = active_id.as_deref() == Some(profile.id.as_str());
                        let pid = profile.id.clone();
                        let pid_upd = profile.id.clone();
                        let pid_prev = profile.id.clone();
                        let pid_del = profile.id.clone();
                        let updated_str = profile
                            .updated_at
                            .with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M")
                            .to_string();

                        card(cx)
                            .when(is_active, |el| {
                                el.border_color(theme.primary)
                                    .bg(theme.primary.opacity(0.08))
                            })
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .flex_wrap()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .flex_wrap()
                                            .child(
                                                div()
                                                    .text_base()
                                                    .font_semibold()
                                                    .child(profile.name.clone()),
                                            )
                                            .when(is_active, |el| {
                                                el.child(pill_badge("当前生效", theme.primary))
                                            })
                                            .child(pill_badge(
                                                format!("{} 节点", profile.node_count),
                                                theme.info,
                                            ))
                                            .child(pill_badge(
                                                if profile.has_custom_dns {
                                                    "含自定义 DNS"
                                                } else {
                                                    "自动补全默认 DNS"
                                                },
                                                if profile.has_custom_dns {
                                                    theme.muted_foreground
                                                } else {
                                                    theme.success
                                                },
                                            ))
                                            .child(pill_badge(
                                                if profile.has_custom_tun {
                                                    "含自定义 TUN"
                                                } else {
                                                    "自动补全默认 TUN"
                                                },
                                                if profile.has_custom_tun {
                                                    theme.muted_foreground
                                                } else {
                                                    theme.success
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .gap_1p5()
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "act-{pid}"
                                                )))
                                                .primary()
                                                .small()
                                                .icon(IconName::Check)
                                                .label(if is_active { "重新应用" } else { "启用" })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.activate_profile(&pid);
                                                    cx.notify();
                                                })),
                                            )
                                            .when(profile.url.is_some(), |el| {
                                                el.child(
                                                    Button::new(SharedString::from(format!(
                                                        "upd-{pid_upd}"
                                                    )))
                                                    .small()
                                                    .icon(IconName::RefreshCw)
                                                    .label("更新")
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.update_profile(&pid_upd);
                                                            cx.notify();
                                                        },
                                                    )),
                                                )
                                            })
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "prev-{pid_prev}"
                                                )))
                                                .small()
                                                .icon(IconName::Eye)
                                                .label("预览合并")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.preview_merged_profile(&pid_prev);
                                                    cx.notify();
                                                })),
                                            )
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "del-{pid_del}"
                                                )))
                                                .danger()
                                                .small()
                                                .icon(IconName::Trash)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.delete_profile(&pid_del);
                                                    cx.notify();
                                                })),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(theme.muted_foreground)
                                    .child(SelectableText::new(
                                        SharedString::from(format!("profile-source-{}", profile.id)),
                                        format!(
                                            "{}  ·  更新于 {}",
                                            profile.url.as_deref().unwrap_or("本地配置文件"),
                                            updated_str
                                        ),
                                    )),
                            )
                            .when_some(profile.subscription_info.as_ref(), |el, sub| {
                                let ratio = sub.usage_ratio();
                                let expire_txt = sub
                                    .expire
                                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                                    .map(|dt| {
                                        format!(
                                            "  ·  到期 {}",
                                            dt.with_timezone(&chrono::Local).format("%Y-%m-%d")
                                        )
                                    })
                                    .unwrap_or_default();
                                el.child(
                                    div()
                                        .v_flex()
                                        .gap_1()
                                        .pt_1()
                                        .child(
                                            div()
                                                .h_flex()
                                                .justify_between()
                                                .text_size(px(13.))
                                                .text_color(theme.muted_foreground)
                                                .child(format!(
                                                    "已用 {} / {}{}",
                                                    format_bytes(sub.used()),
                                                    format_bytes(sub.total),
                                                    expire_txt
                                                ))
                                                .child(format!("{:.1}%", ratio * 100.0)),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .h(px(6.))
                                                .rounded_full()
                                                .bg(theme.muted)
                                                .child(
                                                    div()
                                                        .h_full()
                                                        .w(relative(ratio))
                                                        .rounded_full()
                                                        .bg(if ratio > 0.85 {
                                                            theme.danger
                                                        } else {
                                                            theme.primary
                                                        }),
                                                ),
                                        ),
                                )
                            })
                    })),
            )
            // YAML Preview Panel
            .when_some(self.preview_yaml.clone(), |el, (title, yaml)| {
                el.child(
                    card(cx)
                        .v_flex()
                        .gap_2()
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_semibold()
                                        .child(title),
                                )
                                .child(
                                    Button::new("close-yaml-preview")
                                        .ghost()
                                        .small()
                                        .icon(IconName::X)
                                        .label("关闭预览")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.preview_yaml = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .p_3()
                                .rounded_md()
                                .bg(theme.background)
                                .border_1()
                                .border_color(theme.border)
                                .font_family(theme.mono_font_family.clone())
                                .text_size(theme.mono_font_size)
                                .line_height(px(20.))
                                .text_color(theme.foreground)
                                .child(SelectableText::new("yaml-preview", yaml)),
                        ),
                )
            })
    }

    pub fn render_providers_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let active_tab = self.providers_tab;

        let mut proxy_provs: Vec<_> = self.proxy_providers.values().cloned().collect();
        proxy_provs.sort_by(|a, b| a.name.cmp(&b.name));

        let mut rule_provs: Vec<_> = self.rule_providers.values().cloned().collect();
        rule_provs.sort_by(|a, b| a.name.cmp(&b.name));

        let rule_kw = self.rule_search_input.read(cx).value().to_lowercase();
        let filtered_rules: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                rule_kw.is_empty()
                    || r.rule_type.to_lowercase().contains(&rule_kw)
                    || r.payload.to_lowercase().contains(&rule_kw)
                    || r.proxy.to_lowercase().contains(&rule_kw)
            })
            .take(250)
            .collect();

        div()
            .v_flex()
            .size_full()
            .gap_3()
            // Sub-tab Header
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        div()
                            .h_flex()
                            .gap_1p5()
                            .children(
                                [
                                    (
                                        ProvidersTab::RuleProviders,
                                        format!("规则集合 ({})", rule_provs.len()),
                                    ),
                                    (
                                        ProvidersTab::ProxyProviders,
                                        format!("代理集合 ({})", proxy_provs.len()),
                                    ),
                                    (
                                        ProvidersTab::Rules,
                                        format!("分流规则 ({})", self.rules.len()),
                                    ),
                                ]
                                .into_iter()
                                .map(|(tab, label)| {
                                    let mut b = Button::new(SharedString::from(format!("ptab-{tab:?}")))
                                        .small()
                                        .label(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.providers_tab = tab;
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
                        ProvidersTab::ProxyProviders => Button::new("upd-all-pp")
                            .primary()
                            .small()
                            .icon(IconName::RefreshCw)
                            .label("更新全部代理集合")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.update_all_proxy_providers();
                                cx.notify();
                            }))
                            .into_any_element(),
                        ProvidersTab::RuleProviders => Button::new("upd-all-rp")
                            .primary()
                            .small()
                            .icon(IconName::RefreshCw)
                            .label("更新全部规则集合")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.update_all_rule_providers();
                                cx.notify();
                            }))
                            .into_any_element(),
                        ProvidersTab::Rules => div()
                            .w(px(300.))
                            .child(Input::new(&self.rule_search_input).cleanable(true).small())
                            .into_any_element(),
                    }),
            )
            // Content
            .child(
                div()
                    .id("providers-scroll")
                    .flex_1()
                    .min_h_0()
                    .pr_2()
                    .overflow_y_scrollbar()
                    .child(match active_tab {
                        ProvidersTab::ProxyProviders => div()
                            .v_flex()
                            .gap_3()
                            .when(proxy_provs.is_empty(), |el| {
                                el.child(
                                    card(cx)
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child("当前配置未定义外部代理集合 (proxy-providers)"),
                                )
                            })
                            .children(proxy_provs.into_iter().map(|pp| {
                                let pname = pp.name.clone();
                                let pname_hc = pp.name.clone();
                                let can_update = pp.vehicle_type != "Compatible";
                                card(cx)
                                    .v_flex()
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
                                                    .h_flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .text_base()
                                                            .font_semibold()
                                                            .child(SelectableText::new(
                                                                SharedString::from(format!("pp-name-{}", pp.name)),
                                                                pp.name.clone(),
                                                            )),
                                                    )
                                                    .child(pill_badge(
                                                        pp.vehicle_type.clone(),
                                                        theme.info,
                                                    ))
                                                    .child(pill_badge(
                                                        format!("{} 节点", pp.proxies.len()),
                                                        theme.primary,
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .gap_1p5()
                                                    .child(
                                                        Button::new(SharedString::from(format!(
                                                            "hc-{pname_hc}"
                                                        )))
                                                        .xsmall()
                                                        .icon(IconName::Activity)
                                                        .label("健康检查")
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.healthcheck_proxy_provider(
                                                                    pname_hc.clone(),
                                                                );
                                                                cx.notify();
                                                            },
                                                        )),
                                                    )
                                                    .when(can_update, |el| {
                                                        el.child(
                                                            Button::new(SharedString::from(
                                                                format!("upp-{pname}"),
                                                            ))
                                                            .primary()
                                                            .xsmall()
                                                            .icon(IconName::RefreshCw)
                                                            .label("刷新资源")
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.update_proxy_provider(
                                                                        pname.clone(),
                                                                    );
                                                                    cx.notify();
                                                                },
                                                            )),
                                                        )
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(theme.muted_foreground)
                                            .child(format!(
                                                "更新时间: {}{}",
                                                pp.updated_at.as_deref().unwrap_or("-"),
                                                pp.subscription_info
                                                    .as_ref()
                                                    .map(|s| format!(
                                                        "  ·  流量: {} / {}",
                                                        format_bytes(s.upload + s.download),
                                                        format_bytes(s.total)
                                                    ))
                                                    .unwrap_or_default()
                                            )),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .flex_wrap()
                                            .gap_1p5()
                                            .children(pp.proxies.iter().take(40).map(|n| {
                                                let d = n.latest_delay();
                                                let dc = delay_color(d, cx);
                                                div()
                                                    .h_flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .px_2()
                                                    .py(px(2.))
                                                    .rounded_sm()
                                                    .bg(theme.background.opacity(0.6))
                                                    .border_1()
                                                    .border_color(theme.border)
                                                    .text_xs()
                                                    .child(SelectableText::new(
                                                        SharedString::from(format!("pp-node-{}-{}", pp.name, n.name)),
                                                        n.name.clone(),
                                                    ))
                                                    .child(
                                                        div()
                                                            .text_color(dc)
                                                            .child(delay_text(d)),
                                                    )
                                            })),
                                    )
                            }))
                            .into_any_element(),
                        ProvidersTab::RuleProviders => div()
                            .v_flex()
                            .gap_2()
                            .when(rule_provs.is_empty(), |el| {
                                el.child(
                                    card(cx)
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child("当前配置未定义外部规则集合 (rule-providers)"),
                                )
                            })
                            .children(rule_provs.into_iter().map(|rp| {
                                let rname = rp.name.clone();
                                card(cx)
                                    .py_3()
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
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
                                                        div()
                                                            .text_sm()
                                                            .font_semibold()
                                                            .child(SelectableText::new(
                                                                SharedString::from(format!("rp-name-{}", rp.name)),
                                                                rp.name.clone(),
                                                            )),
                                                    )
                                                    .child(pill_badge(
                                                        rp.behavior.clone(),
                                                        theme.info,
                                                    ))
                                                    .child(pill_badge(
                                                        rp.vehicle_type.clone(),
                                                        theme.muted_foreground,
                                                    ))
                                                    .child(pill_badge(
                                                        format!("{} 条规则", rp.rule_count),
                                                        theme.primary,
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(theme.muted_foreground)
                                                    .child(format!(
                                                        "格式: {}  ·  更新时间: {}",
                                                        if rp.format.is_empty() {
                                                            "yaml"
                                                        } else {
                                                            &rp.format
                                                        },
                                                        rp.updated_at.as_deref().unwrap_or("-")
                                                    )),
                                            ),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("urp-{rname}")))
                                            .xsmall()
                                            .icon(IconName::RefreshCw)
                                            .label("更新")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.update_rule_provider(rname.clone());
                                                cx.notify();
                                            })),
                                    )
                            }))
                            .into_any_element(),
                        ProvidersTab::Rules => div()
                            .v_flex()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(theme.muted_foreground)
                                    .child(format!(
                                        "共 {} 条规则，当前显示前 {} 条匹配结果",
                                        self.rules.len(),
                                        filtered_rules.len()
                                    )),
                            )
                            .children(filtered_rules.into_iter().map(|(idx, rule)| {
                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .bg(theme.secondary.opacity(0.35))
                                    .border_1()
                                    .border_color(theme.border)
                                    .text_xs()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .w(px(36.))
                                                    .text_color(theme.muted_foreground)
                                                    .child(format!("#{}", idx + 1)),
                                            )
                                            .child(pill_badge(
                                                rule.rule_type.clone(),
                                                theme.info,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_color(theme.foreground)
                                                    .child(SelectableText::new(
                                                        ("rule-payload", idx),
                                                        if rule.payload.is_empty() {
                                                            "-".to_string()
                                                        } else {
                                                            rule.payload.clone()
                                                        },
                                                    )),
                                            ),
                                    )
                                    .child(pill_badge(rule.proxy.clone(), theme.primary))
                            }))
                            .into_any_element(),
                    }),
            )
    }
}
