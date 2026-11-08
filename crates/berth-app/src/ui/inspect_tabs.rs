//! Container inspector sub-tabs: Config (mounts, env, network IPs) and Runtime
//! (in-container processes and writable-layer filesystem changes).

use berth_engine::dto::Container;
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{Context, Div, FontWeight, Role, SharedString, Stateful, div, px};

use super::inspect_shared::{fact, small_action};
use super::tabular;
use crate::app::{BerthApp, Tab};

pub(crate) fn config_tab(
    app: &BerthApp,
    container: &Container,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Stateful<Div> {
    let mut scroll = div()
        .id("inspector-config-scroll")
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .gap(px(10.0));

    let life = app.detail_lifetime.as_ref();
    let cmd = life
        .and_then(|l| l.command.as_deref())
        .or_else(|| (!container.command.is_empty()).then_some(container.command.as_str()));
    if let Some(cmd) = cmd {
        scroll = scroll.child(fact("Command", cmd, tokens));
    }
    if let Some(workdir) = life.and_then(|l| l.workdir.as_deref()) {
        scroll = scroll.child(fact("Workdir", workdir, tokens));
    }
    let net_mode = if container.host_config.network_mode.is_empty() {
        "default"
    } else {
        container.host_config.network_mode.as_str()
    };
    scroll = scroll.child(fact("Net mode", net_mode, tokens));

    // Network IPs
    let mut ips: Vec<(String, String)> = life.map(|l| l.ips.clone()).unwrap_or_default();
    if ips.is_empty() {
        for (net_name, ep) in &container.network_settings.networks {
            if !ep.ip_address.is_empty() {
                ips.push((net_name.clone(), ep.ip_address.clone()));
            }
        }
    }
    if !ips.is_empty() {
        scroll = scroll.child(section_heading("Network addresses", ips.len(), tokens));
        for (net_name, ip) in ips {
            let ip_copy = ip.clone();
            scroll = scroll.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .bg(tokens.background)
                    .border_1()
                    .border_color(tokens.border)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(tokens.text)
                            .child(net_name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(11.0))
                            .font_family("Menlo")
                            .font_features(tabular())
                            .text_color(tokens.muted)
                            .child(ip.clone()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("copy-ip-{net_name}")))
                            .flex()
                            .items_center()
                            .h(px(20.0))
                            .px(px(7.0))
                            .rounded(px(10.0))
                            .bg(tokens.surface_hover)
                            .role(Role::Button)
                            .aria_label(format!("Copy IP {ip}"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    ip_copy.clone(),
                                ));
                                this.notify("Container IP copied", false, cx);
                            }))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(tokens.text)
                                    .child("Copy"),
                            ),
                    ),
            );
        }
    }

    // Mounts
    let mounts: Vec<(String, String, String, bool)> = if let Some(l) = life
        && !l.mounts.is_empty()
    {
        l.mounts
            .iter()
            .map(|m| {
                (
                    m.kind.clone(),
                    m.source.clone(),
                    m.destination.clone(),
                    m.rw,
                )
            })
            .collect()
    } else {
        container
            .mounts
            .iter()
            .map(|m| {
                (
                    m.mount_type.clone(),
                    m.name.clone().unwrap_or_else(|| m.source.clone()),
                    m.destination.clone(),
                    m.rw,
                )
            })
            .collect()
    };

    scroll = scroll.child(section_heading("Mounts", mounts.len(), tokens));
    if mounts.is_empty() {
        scroll = scroll.child(empty_note("No volumes or host paths mounted.", tokens));
    } else {
        for (idx, (kind, source, dest, rw)) in mounts.into_iter().enumerate() {
            let is_known_volume = app.snapshot.volumes.iter().any(|v| v.name == source);
            let vol_name = source.clone();
            scroll = scroll.child(
                div()
                    .id(SharedString::from(format!("mount-row-{idx}")))
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(3.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(tokens.background)
                    .border_1()
                    .border_color(tokens.border)
                    .when(is_known_volume, |this| {
                        this.role(Role::Button)
                            .aria_label(format!("Inspect volume {vol_name}"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.tab = Tab::Volumes;
                                this.select_volume(&vol_name, cx);
                            }))
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .w_full()
                            .gap(px(6.0))
                            .child(badge(&kind, tokens.accent_text_pill(), tokens))
                            .child(badge(
                                if rw { "rw" } else { "ro" },
                                if rw {
                                    tokens.muted
                                } else {
                                    tokens.warning_text()
                                },
                                tokens,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(11.0))
                                    .font_family("Menlo")
                                    .text_color(tokens.text)
                                    .child(dest),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .text_color(tokens.muted)
                            .child(format!("from {source}")),
                    ),
            );
        }
    }

    // Environment
    let env = life.map(|l| l.env.as_slice()).unwrap_or(&[]);
    let mut env_header = div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .pt(px(4.0))
        .child(section_heading("Environment", env.len(), tokens))
        .child(div().flex_1());
    if !env.is_empty() {
        let copy_env_text = env
            .iter()
            .map(|(k, v, _)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n");
        env_header = env_header.child(small_action(
            "copy-env",
            "Copy .env",
            tokens,
            cx.listener(move |this, _, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_env_text.clone()));
                this.notify("Environment copied · secrets redacted", false, cx);
            }),
        ));
    }
    scroll = scroll.child(env_header);

    if env.is_empty() {
        let note = if app.inspect_loading {
            "Reading environment…"
        } else {
            "No environment variables reported."
        };
        scroll = scroll.child(empty_note(note, tokens));
    } else {
        let mut env_well = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(4.0))
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(tokens.background)
            .border_1()
            .border_color(tokens.border);
        for (key, value, secret) in env {
            env_well = env_well.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(tokens.text)
                            .child(key.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .text_color(if *secret {
                                tokens.warning_text()
                            } else {
                                tokens.muted
                            })
                            .child(format!("= {value}")),
                    ),
            );
        }
        scroll = scroll.child(env_well);
    }

    scroll
}

pub(crate) fn runtime_tab(
    app: &BerthApp,
    container: &Container,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Stateful<Div> {
    let mut scroll = div()
        .id("inspector-runtime-scroll")
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .gap(px(10.0));

    let proc_count = app
        .detail_top
        .as_ref()
        .map(|t| t.processes.len())
        .unwrap_or(0);
    scroll = scroll.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .child(section_heading("Processes", proc_count, tokens))
            .child(div().flex_1())
            .child(small_action(
                "runtime-refresh",
                "Refresh",
                tokens,
                cx.listener(|this, _, _, cx| this.reload_detail(cx)),
            )),
    );

    if !container.is_up() {
        scroll = scroll.child(empty_note(
            "Container is stopped — start it to inspect live processes.",
            tokens,
        ));
    } else if let Some(top) = &app.detail_top
        && !top.processes.is_empty()
    {
        let pid_ix = top
            .titles
            .iter()
            .position(|t| t.eq_ignore_ascii_case("PID"))
            .unwrap_or(0);
        let user_ix = top
            .titles
            .iter()
            .position(|t| t.eq_ignore_ascii_case("USER"));
        let cmd_ix = top.titles.len().saturating_sub(1);

        let mut proc_well = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(4.0))
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(tokens.background)
            .border_1()
            .border_color(tokens.border);

        for row in top.processes.iter().take(40) {
            let pid = row.get(pid_ix).map(String::as_str).unwrap_or("—");
            let user = user_ix
                .and_then(|ix| row.get(ix))
                .map(String::as_str)
                .unwrap_or("");
            let cmd = row.get(cmd_ix).map(String::as_str).unwrap_or("—");
            proc_well = proc_well.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(48.0))
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .font_features(tabular())
                            .text_color(tokens.muted)
                            .child(pid.to_owned()),
                    )
                    .when(!user.is_empty(), |this| {
                        this.child(
                            div()
                                .w(px(54.0))
                                .truncate()
                                .text_size(px(10.5))
                                .text_color(tokens.muted)
                                .child(user.to_owned()),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .text_color(tokens.text)
                            .child(cmd.to_owned()),
                    ),
            );
        }
        scroll = scroll.child(proc_well);
    } else {
        let note = if app.inspect_loading {
            "Reading processes…"
        } else {
            "No process list returned by engine."
        };
        scroll = scroll.child(empty_note(note, tokens));
    }

    scroll = scroll.child(section_heading(
        "Writable layer changes",
        app.detail_changes.len(),
        tokens,
    ));
    if app.detail_changes.is_empty() {
        let note = if app.inspect_loading {
            "Reading filesystem diff…"
        } else {
            "No modified files in the container's writable layer."
        };
        scroll = scroll.child(empty_note(note, tokens));
    } else {
        let mut diff_well = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(3.0))
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(tokens.background)
            .border_1()
            .border_color(tokens.border);
        for change in app.detail_changes.iter().take(80) {
            let (tag, color) = match change.kind {
                1 => ("A", tokens.success_text()),
                2 => ("D", tokens.danger_text()),
                _ => ("C", tokens.warning_text()),
            };
            diff_well = diff_well.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(16.0))
                            .text_size(px(10.0))
                            .font_family("Menlo")
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(tag),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .text_color(tokens.text)
                            .child(change.path.clone()),
                    ),
            );
        }
        scroll = scroll.child(diff_well);
    }

    scroll
}

fn section_heading(title: &'static str, count: usize, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.muted)
                .child(title),
        )
        .child(
            div()
                .text_size(px(10.5))
                .font_features(tabular())
                .text_color(tokens.muted.alpha(0.75))
                .child(format!("({count})")),
        )
}

fn badge(text: &str, color: gpui::Hsla, tokens: Tokens) -> Div {
    div()
        .flex()
        .items_center()
        .h(px(18.0))
        .px(px(6.0))
        .rounded(px(9.0))
        .bg(tokens.surface_hover)
        .child(
            div()
                .text_size(px(10.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .child(text.to_owned()),
        )
}

fn empty_note(text: &str, tokens: Tokens) -> Div {
    div()
        .text_size(px(11.0))
        .text_color(tokens.muted)
        .child(text.to_owned())
}
