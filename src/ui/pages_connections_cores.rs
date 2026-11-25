use crate::config::format_bytes;
use crate::system::core_manager::short_variant_name;
use crate::ui::app::MihomoApp;
use crate::ui::{ConnSort, card, pill_badge, section_header};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl MihomoApp {
    pub fn render_connections_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let search = self.conn_search_input.read(cx).value().to_lowercase();
        let all_conns = self.connections.connections.as_deref().unwrap_or(&[]);

        let mut list: Vec<_> = all_conns
            .iter()
            .filter(|c| {
                if search.is_empty() {
                    return true;
                }
                c.metadata.display_host().to_lowercase().contains(&search)
                    || c.metadata.process.to_lowercase().contains(&search)
                    || c.rule.to_lowercase().contains(&search)
                    || c.rule_payload.to_lowercase().contains(&search)
                    || c.chains
                        .iter()
                        .any(|ch| ch.to_lowercase().contains(&search))
            })
            .cloned()
            .collect();

        match self.conn_sort {
            ConnSort::Newest => list.sort_by(|a, b| b.start.cmp(&a.start)),
            ConnSort::DownloadDesc => list.sort_by(|a, b| b.download.cmp(&a.download)),
            ConnSort::UploadDesc => list.sort_by(|a, b| b.upload.cmp(&a.upload)),
        }

        let shown_count = list.len().min(150);

        div()
            .v_flex()
            .size_full()
            .gap_3()
            // Summary & Controls Bar
            .child(
                card(cx)
                    .py_3()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .child(pill_badge(
                                format!("活跃连接 {}", all_conns.len()),
                                theme.primary,
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.success)
                                    .font_semibold()
                                    .child(format!(
                                        "↓ {}",
                                        format_bytes(self.connections.download_total)
                                    )),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.info)
                                    .font_semibold()
                                    .child(format!(
                                        "↑ {}",
                                        format_bytes(self.connections.upload_total)
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                Button::new("conn-pause")
                                    .xsmall()
                                    .icon(if self.connections_paused {
                                        IconName::Play
                                    } else {
                                        IconName::Pause
                                    })
                                    .label(if self.connections_paused {
                                        "继续刷新"
                                    } else {
                                        "暂停刷新"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.connections_paused = !this.connections_paused;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("conn-close-all")
                                    .danger()
                                    .xsmall()
                                    .icon(IconName::Trash)
                                    .label("断开全部")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_all_connections();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            // Search & Sort Bar
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(260.))
                            .child(Input::new(&self.conn_search_input).cleanable(true).small()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_1p5()
                            .children(
                                [
                                    (ConnSort::DownloadDesc, "按下载"),
                                    (ConnSort::UploadDesc, "按上传"),
                                    (ConnSort::Newest, "按时间"),
                                ]
                                .into_iter()
                                .map(|(s, lbl)| {
                                    let mut b = Button::new(SharedString::from(format!("csort-{s:?}")))
                                        .xsmall()
                                        .label(lbl)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.conn_sort = s;
                                            cx.notify();
                                        }));
                                    if self.conn_sort == s {
                                        b = b.primary();
                                    }
                                    b
                                }),
                            ),
                    ),
            )
            // Connections List
            .child(
                div()
                    .id("connections-scroll")
                    .flex_1()
                    .min_h_0()
                    .pr_2()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .v_flex()
                            .gap_1p5()
                            .when(list.is_empty(), |el| {
                                el.child(
                                    card(cx)
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child("当前没有匹配的活跃连接"),
                                )
                            })
                            .children(list.into_iter().take(shown_count).map(|conn| {
                                let cid = conn.id.clone();
                                let net = conn.metadata.network.to_uppercase();
                                let net_color = if net == "UDP" {
                                    theme.warning
                                } else {
                                    theme.info
                                };
                                let chain = conn
                                    .chains
                                    .iter()
                                    .rev()
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(" → ");
                                let rule_str = if conn.rule_payload.is_empty() {
                                    conn.rule.clone()
                                } else {
                                    format!("{} ({})", conn.rule, conn.rule_payload)
                                };

                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .bg(theme.secondary.opacity(0.4))
                                    .border_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(pill_badge(net, net_color))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_semibold()
                                                            .overflow_hidden()
                                                            .whitespace_nowrap()
                                                            .child(conn.metadata.display_host()),
                                                    )
                                                    .when(!conn.metadata.process.is_empty(), |el| {
                                                        el.child(pill_badge(
                                                            conn.metadata.process.clone(),
                                                            theme.muted_foreground,
                                                        ))
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .child(format!(
                                                        "链路: {}  ·  规则: {}  ·  来源: {}:{}",
                                                        chain,
                                                        rule_str,
                                                        conn.metadata.source_ip,
                                                        conn.metadata.source_port
                                                    )),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_3()
                                            .flex_shrink_0()
                                            .child(
                                                div()
                                                    .v_flex()
                                                    .items_end()
                                                    .text_xs()
                                                    .child(
                                                        div()
                                                            .text_color(theme.success)
                                                            .child(format!(
                                                                "↓ {}",
                                                                format_bytes(conn.download)
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(theme.info)
                                                            .child(format!(
                                                                "↑ {}",
                                                                format_bytes(conn.upload)
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "close-conn-{cid}"
                                                )))
                                                .ghost()
                                                .xsmall()
                                                .icon(IconName::X)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.close_connection(cid.clone());
                                                    cx.notify();
                                                })),
                                            ),
                                    )
                            })),
                    ),
            )
    }

    pub fn render_cores_page(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let current_ver = self
            .version
            .as_ref()
            .map(|v| v.version.clone())
            .unwrap_or_else(|| "未知（服务未连接）".to_string());

        div()
            .id("cores-scroll")
            .v_flex()
            .size_full()
            .gap_4()
            .pr_2()
            .overflow_y_scrollbar()
            // Current Service Core Card
            .child(
                card(cx)
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
                                    .child(Icon::new(IconName::Cpu).size_5().text_color(theme.primary))
                                    .child(
                                        div()
                                            .text_base()
                                            .font_bold()
                                            .child("当前 Windows 服务核心"),
                                    )
                                    .child(pill_badge(current_ver, theme.info)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!(
                                        "目标路径: {}  ·  服务名: {} ({})",
                                        self.settings.core_binary_path,
                                        self.settings.service_name,
                                        self.service_status.state.label()
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                Button::new("core-svc-restart")
                                    .small()
                                    .icon(IconName::RotateCw)
                                    .label("重启服务")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.restart_windows_service();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            // Local Downloaded Cores
            .child(
                card(cx)
                    .v_flex()
                    .gap_3()
                    .child(section_header(
                        format!("本地已下载核心 ({})", self.local_cores.len()),
                        "切换版本时将自动执行：停止 Windows 服务 → 备份并替换 mihomo.exe → 启动服务",
                        cx,
                    ))
                    .when(self.local_cores.is_empty(), |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("尚未下载本地核心，请从下方 GitHub Release 列表中选择版本下载。"),
                        )
                    })
                    .children(self.local_cores.iter().map(|core| {
                        let c_activate = core.clone();
                        let tag_del = core.tag.clone();
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .bg(theme.background.opacity(0.6))
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .child(core.tag.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!(
                                                "{}  ·  {}",
                                                format_bytes(core.size_bytes),
                                                core.exe_path.display()
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .gap_1p5()
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "use-core-{}",
                                            core.tag
                                        )))
                                        .primary()
                                        .xsmall()
                                        .icon(IconName::Check)
                                        .label("切换为此核心")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.activate_local_core(c_activate.clone());
                                            cx.notify();
                                        })),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "del-core-{}",
                                            core.tag
                                        )))
                                        .danger()
                                        .xsmall()
                                        .icon(IconName::Trash)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.delete_local_core(&tag_del);
                                            cx.notify();
                                        })),
                                    ),
                            )
                    })),
            )
            // GitHub Releases Card
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
                            .child(section_header(
                                "GitHub Release 列表 (MetaCubeX/mihomo)",
                                "拉取官方发布列表并选择适合本机架构的 Windows 构建包下载",
                                cx,
                            ))
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        Switch::new("gh-prerelease")
                                            .checked(self.settings.include_prerelease)
                                            .label("包含 Alpha/预发布")
                                            .on_change(cx.listener(|this, checked, _, cx| {
                                                this.settings.include_prerelease = *checked;
                                                let _ = crate::config::save_settings(&this.settings);
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Switch::new("gh-use-proxy")
                                            .checked(self.settings.use_proxy_for_github)
                                            .label("走本地代理")
                                            .on_change(cx.listener(|this, checked, _, cx| {
                                                this.settings.use_proxy_for_github = *checked;
                                                let _ = crate::config::save_settings(&this.settings);
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new("fetch-gh-releases")
                                            .primary()
                                            .small()
                                            .icon(IconName::CloudDownload)
                                            .label("拉取 Release 列表")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.fetch_github_releases();
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    .when(self.github_releases.is_empty(), |el| {
                        el.child(
                            div()
                                .py_4()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("点击右上角「拉取 Release 列表」从 GitHub 获取最新 Mihomo 核心版本。"),
                        )
                    })
                    .children(self.github_releases.iter().map(|rel| {
                        let tag = rel.tag_name.clone();
                        let tag_toggle = rel.tag_name.clone();
                        let expanded = self.expanded_release_tag.as_deref() == Some(tag.as_str());
                        let win_assets = rel.windows_assets();
                        let pub_date = rel
                            .published_at
                            .split('T')
                            .next()
                            .unwrap_or(&rel.published_at)
                            .to_string();

                        div()
                            .v_flex()
                            .gap_2()
                            .p_3()
                            .rounded_md()
                            .bg(theme.background.opacity(0.6))
                            .border_1()
                            .border_color(if expanded { theme.primary } else { theme.border })
                            .child(
                                div()
                                    .id(SharedString::from(format!("rel-hdr-{tag}")))
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if this.expanded_release_tag.as_deref()
                                            == Some(tag_toggle.as_str())
                                        {
                                            this.expanded_release_tag = None;
                                        } else {
                                            this.expanded_release_tag = Some(tag_toggle.clone());
                                        }
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(Icon::new(if expanded {
                                                IconName::ChevronDown
                                            } else {
                                                IconName::ChevronRight
                                            }))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .child(tag.clone()),
                                            )
                                            .when(rel.prerelease, |el| {
                                                el.child(pill_badge("Pre-release", theme.warning))
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child(format!(
                                                        "{pub_date} · {} 个 Windows 构建",
                                                        win_assets.len()
                                                    )),
                                            ),
                                    ),
                            )
                            .when(expanded, |el| {
                                el.child(
                                    div()
                                        .h_flex()
                                        .flex_wrap()
                                        .gap_2()
                                        .pt_1()
                                        .children(win_assets.into_iter().map(|asset| {
                                            let asset_owned = asset.clone();
                                            let tag_dl = tag.clone();
                                            let label = format!(
                                                "{} ({})",
                                                short_variant_name(&asset.name),
                                                format_bytes(asset.size)
                                            );
                                            Button::new(SharedString::from(format!(
                                                "dl-{}-{}",
                                                tag, asset.name
                                            )))
                                            .xsmall()
                                            .icon(IconName::Download)
                                            .label(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.download_core_asset(
                                                    tag_dl.clone(),
                                                    asset_owned.clone(),
                                                );
                                                cx.notify();
                                            }))
                                        })),
                                )
                            })
                    })),
            )
    }
}
