//! Overview: the fleet, the host, the engine and the disk. The hierarchy is
//! deliberate: one summary band, then fleet + host, then engine + disk. The
//! primary question is "is the engine healthy, and what are the containers
//! using right now?"; everything else is supporting detail.

use berth_core::format;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, IntoElement, Role, div, px};

use super::overview_cards::{engine_card, fleet_card, host_card};
use super::overview_disk::disk_card;
use crate::app::{BerthApp, Tab};
use crate::ui::table::page_header;

pub fn overview(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let snapshot = &app.snapshot;
    let runtime = app.engine.active_runtime();
    let info = runtime
        .map(|runtime| runtime.info.clone())
        .unwrap_or_default();
    let cores = info.ncpu.max(1);
    let connected = runtime.is_some_and(|runtime| runtime.reachable);
    let ready = app.engine.last_update > 0;
    let reading = !ready && app.engine.error.is_empty();
    let complete = snapshot.stats_complete();
    let cpu = snapshot.container_cpu(cores as f32);
    let memory = snapshot.container_memory();

    let cpu_value = if ready && connected && complete {
        cpu.map(|value| format::percent(value as f64))
            .unwrap_or_else(|| "—".to_owned())
    } else {
        "—".to_owned()
    };
    let mem_value = if ready && connected && complete {
        format::bytes(memory)
    } else {
        "—".to_owned()
    };
    let disk_total = snapshot
        .df
        .as_ref()
        .map(|df| df.stores().total())
        .unwrap_or(0);
    let disk_value = if ready && connected && snapshot.df.is_some() {
        format::bytes(disk_total.max(0) as u64)
    } else {
        "—".to_owned()
    };
    let reclaimable = snapshot
        .df
        .as_ref()
        .map(|df| format::bytes(df.reclaimable().max(0) as u64))
        .unwrap_or_else(|| "—".to_owned());

    let status = status_band(app, tokens, runtime, connected, reading, cx);

    let wide = widths.content >= 800.0;
    let gap = 12.0;
    let right_w = if wide {
        (widths.content * 0.36).clamp(300.0, 380.0)
    } else {
        widths.content
    };
    let left_w = if wide {
        widths.content - right_w - gap
    } else {
        widths.content
    };

    let fleet = fleet_card(app, tokens, left_w, complete, &cpu_value, &mem_value);
    let host = host_card(app, tokens, right_w, complete.then_some(memory));

    let row_one = if wide {
        div()
            .flex()
            .flex_row()
            .w_full()
            .gap(px(gap))
            .child(fleet)
            .child(host)
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(gap))
            .child(fleet)
            .child(host)
    };

    let engine = engine_card(tokens, left_w, runtime, &info, snapshot, cx);
    let disk = disk_card(tokens, right_w, snapshot, &disk_value, &reclaimable, cx);
    let row_two = if wide {
        div()
            .flex()
            .flex_row()
            .items_start()
            .w_full()
            .gap(px(gap))
            .child(engine)
            .child(disk)
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(gap))
            .child(engine)
            .child(disk)
    };

    let header = page_header(app, widths, None, None, cx);
    div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(12.0))
        .child(header)
        .child(status)
        .child(row_one)
        .child(row_two)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn status_band(
    app: &BerthApp,
    tokens: Tokens,
    runtime: Option<&berth_engine::discovery::Runtime>,
    connected: bool,
    reading: bool,
    cx: &mut Context<BerthApp>,
) -> Div {
    let error = !app.engine.error.is_empty();
    let (label, note) = if error {
        ("Engine warning", app.engine.error.clone())
    } else if reading {
        ("Reading engine…", "probing known sockets".to_owned())
    } else if connected {
        (
            "Engine healthy",
            runtime
                .map(|runtime| {
                    format!(
                        "{} · v{} · API {}",
                        runtime.name, runtime.version.version, runtime.version.api_version
                    )
                })
                .unwrap_or_default(),
        )
    } else {
        (
            "Engine not answering",
            "open Runtimes to see what was tried".to_owned(),
        )
    };
    let dot = if error {
        tokens.danger_text()
    } else if connected {
        tokens.success_text()
    } else {
        tokens.warning_fill
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(18.0))
        .p(px(16.0))
        .rounded(px(layout::CARD_RADIUS))
        .bg(tokens.surface)
        .border_1()
        .border_color(tokens.border)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .min_w(px(220.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(7.0))
                        .child(div().w(px(7.0)).h(px(7.0)).rounded(px(4.0)).bg(dot))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(tokens.text)
                                .child(label),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(tokens.muted)
                        .truncate()
                        .child(note),
                ),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_features(crate::ui::tabular())
                        .text_color(tokens.muted)
                        .child(if app.engine.last_update == 0 {
                            "updated —".to_owned()
                        } else {
                            format!("updated {}", format::clock(app.engine.last_update))
                        }),
                )
                .child(
                    div()
                        .id("view-containers")
                        .flex()
                        .items_center()
                        .h(px(30.0))
                        .px(px(12.0))
                        .rounded(px(15.0))
                        .bg(tokens.surface_hover)
                        .hover(move |style| style.bg(tokens.accent.alpha(0.12)))
                        .role(Role::Button)
                        .aria_label("View containers")
                        .on_click(cx.listener(|this, _, _, cx| this.switch_to(Tab::Containers, cx)))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(tokens.text)
                                .child("View containers"),
                        ),
                ),
        )
}
