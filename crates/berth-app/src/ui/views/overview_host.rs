//! The Overview's host card: the machine's memory split three ways, and its
//! own CPU and memory over time.

use berth_core::format;
use berth_core::history::Which;
use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{Div, FontWeight, Hsla, div, px};

use super::overview_cards::Unit;
use crate::app::BerthApp;
use crate::ui::graph;
use crate::ui::kit::{Card, caption, card, card_title, icon_chip};
use crate::ui::tabular;

/// The host: its memory split three ways, and its own CPU and memory over
/// time, from the series the history records.
pub(crate) fn host_card(app: &BerthApp, tokens: Tokens, containers_mem: Option<u64>) -> Card {
    let sample = &app.host;
    let history = &app.engine.history;
    let cpu = if sample.cpu_known {
        format::percent(sample.cpu_percent as f64)
    } else {
        "—".to_owned()
    };
    let mut card = card(tokens).gap(px(8.0)).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .gap(px(8.0))
            .child(icon_chip(
                "host",
                12.0,
                tokens.elsewhere(),
                tokens,
                layout::CHIP,
            ))
            .child(card_title("Host", tokens))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(type_scale::LABEL))
                    .font_features(tabular())
                    .text_color(tokens.muted)
                    .child(format!("CPU {cpu}")),
            ),
    );
    if sample.mem_total == 0 {
        return card
            .child(caption(
                "Host memory is unavailable on this platform.",
                tokens,
            ))
            .child(graph::cell_graph(
                history.values(Which::HostCpu).to_vec(),
                tokens.elsewhere(),
                tokens,
                Some(100.0),
                44.0,
                Unit::Percent.ceiling(Some(100.0)),
            ));
    }
    card = card.child(
        div()
            .flex()
            .flex_row()
            .items_end()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(type_scale::METRIC))
                    .font_weight(FontWeight::BOLD)
                    .font_features(tabular())
                    .text_color(tokens.text)
                    .child(format::bytes(sample.mem_used)),
            )
            .child(
                div()
                    .text_size(px(type_scale::SECONDARY))
                    .text_color(tokens.muted)
                    .pb(px(3.0))
                    .child(format!("of {} in use", format::bytes(sample.mem_total))),
            ),
    );
    let used = sample.mem_used;
    let total = sample.mem_total.max(1) as f32;
    let free = sample.mem_total.saturating_sub(used);
    // What the containers hold is known only from a complete sample. Until
    // then the split says so, and the bar shows what is used and what is free.
    let split = containers_mem.map(|containers_mem| {
        let in_containers = containers_mem.min(used);
        let engine = sample.engine_rss.min(used.saturating_sub(in_containers));
        let elsewhere = used.saturating_sub(in_containers).saturating_sub(engine);
        (in_containers, engine, elsewhere)
    });
    let (segments, legend) = match split {
        Some((in_containers, engine, elsewhere)) => (
            vec![
                (in_containers as f32 / total, tokens.accent_fill()),
                (engine as f32 / total, tokens.warning_fill),
                (elsewhere as f32 / total, tokens.elsewhere()),
                (free as f32 / total, tokens.track),
            ],
            format!(
                "containers {} · engine helpers {} · elsewhere {}",
                format::bytes(in_containers),
                format::bytes(engine),
                format::bytes(elsewhere)
            ),
        ),
        None => (
            vec![
                (used as f32 / total, tokens.elsewhere()),
                (free as f32 / total, tokens.track),
            ],
            "Containers — · per-container stats incomplete".to_owned(),
        ),
    };
    card.child(graph::stacked_bar(segments, tokens.track))
        .child(caption(legend, tokens))
        .child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .gap(px(layout::CARD_GAP))
                .child(host_spark(
                    "Host CPU",
                    cpu,
                    history.values(Which::HostCpu).to_vec(),
                    tokens.elsewhere(),
                    Some(100.0),
                    Unit::Percent,
                    tokens,
                ))
                .child(host_spark(
                    "Host memory",
                    format::bytes(sample.mem_used),
                    history.values(Which::HostMem).to_vec(),
                    tokens.muted,
                    Some(total),
                    Unit::Bytes,
                    tokens,
                )),
        )
}

/// One of the host's own series, small, under its split.
fn host_spark(
    label: &'static str,
    value: String,
    values: Vec<f32>,
    color: Hsla,
    ceiling: Option<f32>,
    unit: Unit,
    tokens: Tokens,
) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .w_full()
                .gap(px(6.0))
                .child(caption(label, tokens))
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(type_scale::SECONDARY))
                        .font_weight(FontWeight::SEMIBOLD)
                        .font_features(tabular())
                        .text_color(tokens.text)
                        .child(value),
                ),
        )
        .child(graph::cell_graph(
            values,
            color,
            tokens,
            ceiling,
            44.0,
            unit.ceiling(ceiling),
        ))
}
