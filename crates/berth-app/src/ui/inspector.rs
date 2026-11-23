//! The right-hand inspector. M0 shows the selected container's facts; logs,
//! actions and the raw inspect document arrive in M4.

use berth_core::format;
use berth_engine::detail::up_seconds;
use berth_engine::dto::Container;
use berth_engine::stats::Sample;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{FontWeight, IntoElement, Role, ScrollEvent, div, px};

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
                    .track_scroll(&app.inspect_scroll)
                    .debug_selector(|| "inspect-well".to_string())
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
    let actions = container.actions();
    // What the state allows, in the order a person reaches for it, then the
    // actions that need no particular state.
    pane = pane.child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .w_full()
            .gap(px(6.0))
            .when(actions.resume, |this| {
                this.child(lifecycle("Resume", "/unpause", "Resumed", &id, tokens, cx))
            })
            .when(actions.pause, |this| {
                this.child(lifecycle("Pause", "/pause", "Paused", &id, tokens, cx))
            })
            .when(actions.start, |this| {
                this.child(lifecycle("Start", "/start", "Started", &id, tokens, cx))
            })
            .when(actions.restart, |this| {
                this.child(lifecycle(
                    "Restart",
                    "/restart",
                    "Restarted",
                    &id,
                    tokens,
                    cx,
                ))
            })
            .when(actions.stop, |this| {
                this.child(lifecycle("Stop", "/stop", "Stopped", &id, tokens, cx))
            })
            .when(actions.terminal, |this| {
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
            .when(container.state != "removing", |this| {
                this.child(delete_button(
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
                ))
            }),
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

    let body = log_body(app, cx);
    pane.child(
        div()
            .id("log-well")
            .track_scroll(&app.log_scroll)
            .debug_selector(|| "log-well".to_string())
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
            // Scrolling back up pauses Follow, as it does in every log viewer.
            // The toggle shows that it has paused.
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                if event.delta.pixel_delta(px(1.0)).y > px(0.0) && this.logs_follow {
                    this.logs_follow = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .text_size(px(11.5))
                    .font_family("Menlo")
                    .text_color(tokens.text)
                    .child(body),
            ),
    )
}

/// What the log well shows. A poll in flight never blanks the lines already
/// on screen: only the very first read, with nothing to show yet, says it is
/// reading.
pub(crate) fn log_body(app: &crate::app::BerthApp, cx: &gpui::App) -> String {
    let query = app.log_search.read(cx).text().to_owned();
    if app.logs_loading && app.logs_text.is_empty() {
        return "Reading…".to_owned();
    }
    if query.is_empty() {
        if app.logs_text.is_empty() {
            "No output yet. A container that has said nothing says nothing.".to_owned()
        } else {
            app.logs_text.clone()
        }
    } else {
        let matched = berth_engine::logs::filter_lines(&app.logs_text, &query);
        if matched.is_empty() {
            format!("No line contains {query:?}.")
        } else {
            matched.join("\n")
        }
    }
}

/// A button for one container lifecycle verb. The request path is built when
/// it is pressed.
fn lifecycle(
    label: &'static str,
    route: &'static str,
    past: &'static str,
    id: &str,
    tokens: Tokens,
    cx: &mut gpui::Context<crate::app::BerthApp>,
) -> impl IntoElement {
    let id = id.to_owned();
    verb_button(
        label,
        tokens,
        cx.listener(move |this, _, _, cx| {
            this.run_action(
                "POST".into(),
                format!("/containers/{id}{route}"),
                past.into(),
                cx,
            )
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Point, ScrollDelta, ScrollEvent, TestAppContext, px};

    #[gpui::test]
    fn a_follow_poll_never_blanks_the_logs_already_on_screen(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.logs_text = "line one\nline two\n".into();
            // A poll is in flight.
            app.logs_loading = true;
            assert_eq!(log_body(app, cx), "line one\nline two\n");
            // Only the very first read has nothing to show yet.
            app.logs_text.clear();
            assert_eq!(log_body(app, cx), "Reading…");
            app.logs_loading = false;
            assert_eq!(
                log_body(app, cx),
                "No output yet. A container that has said nothing says nothing."
            );
            // A search that matches nothing says so.
            app.logs_text = "alpha\nbeta\n".into();
            app.log_search
                .update(cx, |input, cx| input.set_value("gamma", cx));
            assert_eq!(log_body(app, cx), "No line contains \"gamma\".");
            app.log_search
                .update(cx, |input, cx| input.set_value("beta", cx));
            assert_eq!(log_body(app, cx), "beta");
        });
    }

    #[gpui::test]
    fn follow_keeps_the_well_at_its_end_until_the_reader_scrolls_up(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.select(0, cx);
            app.logs_text = (0..400).map(|line| format!("line {line}\n")).collect();
            app.logs_loading = false;
            app.logs_follow = true;
            // What a finished read does when Follow is on.
            app.log_scroll.scroll_to_bottom();
        });
        crate::testkit::draw(cx);
        app.read_with(&*cx, |app, _| {
            let max = app.log_scroll.max_offset().y;
            assert!(max > px(0.0), "the logs overflow the well ({max:?})");
            assert_eq!(
                app.log_scroll.offset().y,
                -max,
                "the newest lines are on screen"
            );
        });

        // The reader scrolls back up: Follow pauses, and the toggle shows it.
        let well = cx
            .debug_bounds("log-well")
            .expect("the log well was laid out");
        cx.simulate_event(ScrollEvent {
            position: well.center(),
            delta: ScrollDelta::Pixels(Point::new(px(0.0), px(40.0))),
            ..Default::default()
        });
        assert!(
            !app.read_with(&*cx, |app, _| app.logs_follow),
            "scrolling up pauses Follow"
        );
    }
}
