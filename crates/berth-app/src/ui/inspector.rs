//! The right-hand inspector. M0 shows the selected container's facts; logs,
//! actions and the raw inspect document arrive in M4.

use berth_core::format;
use berth_engine::detail::up_seconds;
use berth_engine::dto::Container;
use berth_engine::stats::Sample;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{FontWeight, IntoElement, Role, div, px};

pub(crate) use super::inspect_objects::{image_pane, network_pane, volume_pane};
use super::inspect_shared::*;
use super::inspect_tabs::{config_tab, runtime_tab};
use crate::app::InspectorTab;

pub(crate) fn container_pane(
    app: &crate::app::BerthApp,
    container: &Container,
    stat: Sample,
    width: f32,
    tokens: Tokens,
    cx: &mut gpui::Context<crate::app::BerthApp>,
) -> impl IntoElement {
    let tab = app.inspector_tab;
    let mut pane = pane_shell(container.name(), "Container inspector", width, tokens, cx).child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .child(pane_tab(
                "Facts",
                tab == InspectorTab::Facts,
                tokens,
                cx.listener(|this, _, _, cx| {
                    this.inspector_tab = InspectorTab::Facts;
                    cx.notify();
                }),
            ))
            .child(pane_tab(
                "Config",
                tab == InspectorTab::Config,
                tokens,
                cx.listener(|this, _, _, cx| {
                    this.inspector_tab = InspectorTab::Config;
                    if this.detail_lifetime.is_none() {
                        this.reload_detail(cx);
                    }
                    cx.notify();
                }),
            ))
            .child(pane_tab(
                "Runtime",
                tab == InspectorTab::Runtime,
                tokens,
                cx.listener(|this, _, _, cx| {
                    this.inspector_tab = InspectorTab::Runtime;
                    if this.detail_top.is_none() && this.detail_changes.is_empty() {
                        this.reload_detail(cx);
                    }
                    cx.notify();
                }),
            ))
            .child(pane_tab(
                "Inspect",
                tab == InspectorTab::Inspect,
                tokens,
                cx.listener(|this, _, _, cx| {
                    this.inspector_tab = InspectorTab::Inspect;
                    if this.inspect_text.is_none() {
                        this.reload_detail(cx);
                    }
                    cx.notify();
                }),
            )),
    );

    if tab == InspectorTab::Config {
        return pane.child(config_tab(app, container, tokens, cx));
    }
    if tab == InspectorTab::Runtime {
        return pane.child(runtime_tab(app, container, tokens, cx));
    }
    if tab == InspectorTab::Inspect {
        pane = pane
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(small_action(
                        "inspect-copy",
                        "Copy",
                        tokens,
                        cx.listener(|this, _, _, cx| this.copy_inspect(cx)),
                    ))
                    .child(small_action(
                        "inspect-refresh",
                        "Refresh",
                        tokens,
                        cx.listener(|this, _, _, cx| this.reload_detail(cx)),
                    )),
            )
            .child(
                div()
                    .id("inspect-well")
                    .flex()
                    .flex_col()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(px(8.0))
                    .rounded(px(8.0))
                    .bg(tokens.background)
                    .border_1()
                    .border_color(tokens.border)
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .text_color(tokens.text)
                            .child(app.inspect_text.clone().unwrap_or_else(|| {
                                if app.inspect_loading {
                                    "Reading…".to_owned()
                                } else {
                                    "Nothing loaded yet. Refresh fetches the engine's JSON."
                                        .to_owned()
                                }
                            })),
                    ),
            );
        return pane;
    }

    pane = pane
        .child(fact("Image", &container.image, tokens))
        .child(fact("State", &container.state, tokens))
        .child(fact("Status", &container.status, tokens));
    if let Some(health) = container.health() {
        pane = pane.child(fact("Health", health, tokens));
    }
    if let Some(project) = container.project() {
        pane = pane.child(fact("Project", project, tokens));
    }
    if let Some(service) = container.service() {
        pane = pane.child(fact("Service", service, tokens));
    }
    if let Some(lifetime) = &app.detail_lifetime {
        if let Some(started) = lifetime.started_at {
            pane = pane.child(fact(
                "Started",
                &format::started(started, format::now()),
                tokens,
            ));
        }
        pane = pane.child(fact("Restarts", &lifetime.restarts.to_string(), tokens));
        if let Some(policy) = &lifetime.restart_policy {
            pane = pane.child(fact("Policy", policy, tokens));
        }
        if let Some(code) = lifetime.exit_code {
            let label = if lifetime.oom_killed {
                format!("{code} (OOM killed)")
            } else {
                code.to_string()
            };
            pane = pane.child(fact("Exit", &label, tokens));
        } else if lifetime.oom_killed {
            pane = pane.child(fact("Exit", "OOM killed", tokens));
        }
        if let Some(probe) = &lifetime.health_log {
            pane = pane.child(fact("Probe", probe, tokens));
        }
    } else if app.logs_loading {
        pane = pane.child(fact("Started", "Reading…", tokens));
    } else if let Some(seconds) = up_seconds(&container.status) {
        pane = pane.child(fact("Up", &format::duration(seconds), tokens));
    }
    if container.is_up() {
        pane = pane
            .child(fact(
                "CPU",
                &if stat.sampled && stat.cpu_known {
                    format::percent(stat.cpu)
                } else {
                    "—".to_owned()
                },
                tokens,
            ))
            .child(fact(
                "Memory",
                &if stat.sampled && stat.mem_known {
                    if stat.limit > 0 {
                        let pct = (stat.mem as f64 / stat.limit as f64) * 100.0;
                        format!(
                            "{} of {} ({})",
                            format::bytes(stat.mem),
                            format::bytes(stat.limit),
                            format::percent(pct)
                        )
                    } else {
                        format::bytes(stat.mem)
                    }
                } else {
                    "—".to_owned()
                },
                tokens,
            ))
            .child(fact(
                "Network",
                &if stat.sampled {
                    format!(
                        "↓ {} · ↑ {}",
                        format::bytes(stat.net_rx),
                        format::bytes(stat.net_tx)
                    )
                } else {
                    "↓ — · ↑ —".to_owned()
                },
                tokens,
            ))
            .child(fact(
                "Disk",
                &if stat.sampled && stat.blk_known {
                    format!(
                        "read {} · wrote {}",
                        format::bytes(stat.blk_read),
                        format::bytes(stat.blk_write)
                    )
                } else {
                    "read — · wrote —".to_owned()
                },
                tokens,
            ));
        if let Some(pids) = stat.pids {
            pane = pane.child(fact("PIDs", &pids.to_string(), tokens));
        }
    }
    let ports = container.published_ports();
    if !ports.is_empty() {
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .w_full()
            .gap(px(6.0))
            .child(
                div()
                    .w(px(64.0))
                    .text_size(px(type_scale::FACT_LABEL))
                    .text_color(tokens.muted)
                    .child("Ports"),
            );
        for binding in ports.iter().take(6) {
            let host = binding.ip.clone();
            let port = binding.port;
            row = row.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "port-{}:{}/{}",
                        binding.ip, binding.port, binding.private_port
                    )))
                    .flex()
                    .items_center()
                    .h(px(20.0))
                    .px(px(8.0))
                    .rounded(px(10.0))
                    .bg(tokens.warning_fill.alpha(0.14))
                    .role(Role::Button)
                    .aria_label(format!("Open port {port}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_port(&host, port, cx);
                    }))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_features(super::tabular())
                            .text_color(tokens.warning_text())
                            .child(format!("{}→{}", binding.port, binding.private_port)),
                    ),
            );
        }
        if ports.len() > 6 {
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(20.0))
                    .px(px(6.0))
                    .rounded(px(10.0))
                    .bg(tokens.surface_hover)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_features(super::tabular())
                            .text_color(tokens.muted)
                            .child(format!("+{}", ports.len() - 6)),
                    ),
            );
        }
        pane = pane.child(row);
    }

    let name = container.name().to_owned();
    let id = container.id.clone();
    pane = pane.child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .w_full()
            .gap(px(6.0))
            .when(container.is_up(), |this| {
                this.child(verb_button(
                    if container.is_paused() {
                        "Resume"
                    } else {
                        "Pause"
                    },
                    tokens,
                    cx.listener({
                        let id = id.clone();
                        move |this, _, _, cx| {
                            let (path, verb) = if container_state_paused(this, &id) {
                                (format!("/containers/{id}/unpause"), "Resumed")
                            } else {
                                (format!("/containers/{id}/pause"), "Paused")
                            };
                            this.run_action("POST".into(), path, verb.into(), cx);
                        }
                    }),
                ))
            })
            .when(!container.is_up(), |this| {
                this.child(verb_button(
                    "Start",
                    tokens,
                    cx.listener({
                        let id = id.clone();
                        move |this, _, _, cx| {
                            this.run_action(
                                "POST".into(),
                                format!("/containers/{id}/start"),
                                "Started".into(),
                                cx,
                            );
                        }
                    }),
                ))
            })
            .when(container.is_up(), |this| {
                this.child(verb_button(
                    "Restart",
                    tokens,
                    cx.listener({
                        let id = id.clone();
                        move |this, _, _, cx| {
                            this.run_action(
                                "POST".into(),
                                format!("/containers/{id}/restart"),
                                "Restarted".into(),
                                cx,
                            );
                        }
                    }),
                ))
                .child(verb_button(
                    "Stop",
                    tokens,
                    cx.listener({
                        let id = id.clone();
                        move |this, _, _, cx| {
                            this.run_action(
                                "POST".into(),
                                format!("/containers/{id}/stop"),
                                "Stopped".into(),
                                cx,
                            );
                        }
                    }),
                ))
            })
            .when(container.is_up(), |this| {
                this.child(verb_button(
                    "Terminal",
                    tokens,
                    cx.listener(|this, _, _, cx| this.open_terminal(cx)),
                ))
            })
            .child(verb_button(
                "Clone…",
                tokens,
                cx.listener(|this, _, _, cx| this.clone_container_to_run(cx)),
            ))
            .when(
                app.detail_lifetime
                    .as_ref()
                    .is_some_and(|lifetime| lifetime.run_cmd.is_some()),
                |this| {
                    this.child(verb_button(
                        "Copy run",
                        tokens,
                        cx.listener(|this, _, _, cx| this.copy_run_command(cx)),
                    ))
                },
            )
            .child(delete_button(
                "Delete",
                tokens,
                cx.listener({
                    let id = id.clone();
                    let name = name.clone();
                    move |this, _, _, cx| {
                        this.confirm_action(
                            "DELETE".into(),
                            format!("/containers/{id}?force=1"),
                            "Delete".into(),
                            name.clone(),
                            cx,
                        );
                    }
                }),
            )),
    );

    let mut logs_head = div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.muted)
                .child("Logs · last 500 lines"),
        )
        .child(div().flex_1())
        .child(small_action(
            "logs-refresh",
            "Refresh",
            tokens,
            cx.listener(|this, _, _, cx| this.reload_detail(cx)),
        ));
    logs_head = logs_head.child(small_action(
        "logs-copy",
        "Copy",
        tokens,
        cx.listener(|this, _, _, cx| this.copy_logs(cx)),
    ));
    pane = pane.child(logs_head);

    pane = pane.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .child(toggle(
                "Follow",
                app.logs_follow,
                tokens,
                cx.listener(|this, _, _, cx| {
                    this.logs_follow = !this.logs_follow;
                    if this.logs_follow {
                        this.reload_detail(cx);
                    }
                    cx.notify();
                }),
            ))
            .child(toggle(
                "Times",
                app.logs_timestamps,
                tokens,
                cx.listener(|this, _, _, cx| this.toggle_log_timestamps(cx)),
            ))
            .child(div().flex_1())
            .child(app.log_search.clone()),
    );

    let query = app.log_search.read(cx).text().to_owned();
    let visible = if query.is_empty() {
        app.logs_text.clone()
    } else {
        berth_engine::logs::filter_lines(&app.logs_text, &query).join("\n")
    };
    let body = if app.logs_loading {
        "Reading…".to_owned()
    } else if visible.is_empty() {
        "No output yet. A container that has said nothing says nothing.".to_owned()
    } else {
        visible
    };
    pane.child(
        div()
            .id("log-well")
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p(px(8.0))
            .rounded(px(8.0))
            .bg(tokens.background)
            .border_1()
            .border_color(tokens.border)
            .child(
                div()
                    .text_size(px(11.5))
                    .font_family("Menlo")
                    .text_color(tokens.text)
                    .child(body),
            ),
    )
}

fn container_state_paused(app: &crate::app::BerthApp, id: &str) -> bool {
    app.snapshot
        .containers
        .iter()
        .find(|container| container.id == id)
        .is_some_and(|container| container.is_paused())
}
