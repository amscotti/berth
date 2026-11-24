//! Overview: what the containers are using, the host they sit on, and the
//! engine and its disk. The hierarchy is the reference's: four series, then
//! the host, then the engine's facts and its stores.
//!
//! The primary question is "is the engine healthy, and what are the containers
//! using right now?"; everything else is supporting detail.

use berth_core::format;
use berth_core::history::Which;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, div, px};

use super::overview_cards::{
    Reading, Series, engine_card, io_strip, memory_ceiling, sample, store_well,
};
use super::overview_disk::disk_card as stores_card;
use super::overview_host::host_card;
use crate::app::{BerthApp, Tab};
use crate::ui::kit::{Card, button, card, card_row};
use crate::ui::page::page_header_with;

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
    let memory_used = snapshot.container_memory();
    let total = snapshot.containers.len();

    // What the CPU and memory cards say: a complete sample plainly, a
    // partial one with how partial, and nothing as unknown.
    let mem_total = runtime
        .filter(|runtime| runtime.info_read)
        .map(|runtime| runtime.info.mem_total)
        .filter(|total| *total > 0);
    let sample = sample(snapshot, ready && connected, cores, mem_total);
    let running_value = if ready && connected {
        snapshot.running().to_string()
    } else {
        "—".to_owned()
    };
    let disk_value = if ready && connected {
        format::measured(snapshot.store_bytes())
    } else {
        "—".to_owned()
    };
    let reclaimable = snapshot
        .df
        .as_ref()
        .map(|df| format::bytes(df.reclaimable().max(0) as u64))
        .unwrap_or_else(|| "—".to_owned());

    let health = health(app, connected, reading);
    let status = status_band(&health, tokens, cx);

    let wide = widths.content >= 800.0;
    let gap = layout::CARD_GAP;
    let side = if wide {
        (widths.content * 0.36).clamp(300.0, 380.0)
    } else {
        widths.content
    };
    let main = if wide {
        widths.content - side - gap
    } else {
        widths.content
    };

    let half = if wide {
        (main - gap) / 2.0
    } else {
        widths.content
    };
    // The four series the history records. A ceiling is fixed, never fitted
    // to what has been seen: the memory ceiling is the engine's own total,
    // the running ceiling is how many containers there are.
    let history = &app.engine.history;
    let mem_ceiling = memory_ceiling(runtime);
    let cpu = Series::cpu(tokens).card(
        sample.cpu.clone(),
        history.values(Which::Cpu).to_vec(),
        Some(100.0),
        half,
        tokens,
    );
    let memory = Series::memory(tokens).card(
        sample.memory.clone(),
        history.values(Which::Mem).to_vec(),
        mem_ceiling,
        half,
        tokens,
    );
    let running = Series::running(tokens).card(
        Reading {
            figure: running_value,
            note: format!("of {}", format::count(total, "container", "containers")),
            partial: false,
        },
        history.values(Which::Running).to_vec(),
        (total > 0).then_some(total as f32),
        half,
        tokens,
    );
    // No endpoint berth reads says how large the engine's disk is, so the
    // disk card has no series to draw against a fixed ceiling. Its well
    // shows what the stores hold instead.
    let disk = Series::disk(tokens).card_with(
        Reading {
            figure: disk_value.clone(),
            note: format!("{reclaimable} reclaimable"),
            partial: false,
        },
        store_well(snapshot, tokens),
        half,
        tokens,
    );
    let series = if wide {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .gap(px(gap))
            .child(card_row().child(cpu).child(memory))
            .child(card_row().child(running).child(disk))
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(gap))
            .child(cpu)
            .child(memory)
            .child(running)
            .child(disk)
    };

    let host = host_card(app, tokens, complete.then_some(memory_used));
    let strip = card(tokens).child(io_strip(app, tokens));

    let engine = engine_card(tokens, main, runtime, &info, snapshot, cx);
    let stores = stores_card(
        tokens,
        side,
        snapshot,
        app.engine.dialect,
        &disk_value,
        &reclaimable,
        cx,
    );
    let facts = if wide {
        card_row().child(engine).child(stores)
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(gap))
            .child(engine)
            .child(stores)
    };

    // Wide: the four series beside the host and its fleet's traffic, so that
    // the page fits its window. Narrow: one column, and the page scrolls.
    let body = if wide {
        let beside = div()
            .flex()
            .flex_col()
            .w(px(side))
            .gap(px(gap))
            .child(host)
            .child(strip);
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(12.0))
            .child(card_row().child(series).child(beside))
            .child(facts)
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(12.0))
            .child(series)
            .child(host)
            .child(strip)
            .child(facts)
    };

    // When all is well the header says so with a dot and the clock, and
    // the page gets straight to the containers. Anything else gets a band.
    let header = page_header_with(
        app,
        widths,
        None,
        None,
        (health == Health::Healthy)
            .then(|| all_is_well(tokens, app.engine.last_update).into_any_element()),
        cx,
    );
    div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(12.0))
        .child(header)
        .children(status)
        .child(body)
        .into_any_element()
}

/// How the engine is, as far as the Overview is concerned. One of these is
/// quiet and three get a band of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Health {
    /// Answering, and with nothing to say: the page's header has the dot.
    Healthy,
    /// Answering with a complaint, or gone after it had answered: the last
    /// lists stay on screen and the complaint is read in full.
    Warning(String),
    /// The first pass is on its way.
    Reading,
    /// Nothing is answering on the sockets berth knows.
    NotAnswering,
}

pub(crate) fn health(app: &BerthApp, connected: bool, reading: bool) -> Health {
    if !connected && !reading && app.engine.client.is_none() {
        Health::NotAnswering
    } else if !app.engine.error.is_empty() {
        Health::Warning(app.engine.error.clone())
    } else if reading {
        Health::Reading
    } else if connected {
        Health::Healthy
    } else {
        Health::NotAnswering
    }
}

/// The dot and the clock that stand in for a band when all is well.
fn all_is_well(tokens: Tokens, updated: i64) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(7.0))
        .child(
            div()
                .w(px(7.0))
                .h(px(7.0))
                .rounded(px(4.0))
                .bg(tokens.success_text()),
        )
        .child(
            div()
                .text_size(px(type_scale::SECONDARY))
                .text_color(tokens.muted)
                .child(format!("updated {}", format::clock(updated))),
        )
}

/// The band for the three states that need more than a dot: what is wrong,
/// and the way out of it.
fn status_band(health: &Health, tokens: Tokens, cx: &mut Context<BerthApp>) -> Option<Card> {
    let (label, note, dot): (&str, String, gpui::Hsla) = match health {
        Health::Healthy => return None,
        Health::Warning(error) => ("Engine warning", error.clone(), tokens.danger_text()),
        Health::Reading => (
            "Reading engine…",
            "probing known sockets".to_owned(),
            tokens.warning_fill,
        ),
        Health::NotAnswering => (
            "Engine not answering",
            "open Runtimes to see what was tried".to_owned(),
            tokens.warning_fill,
        ),
    };
    let way_out: Option<gpui::AnyElement> = match health {
        // A complaint is asked about again; a silence is looked into.
        Health::Warning(_) => Some(
            button("overview-retry", "Retry now", tokens)
                .on_click(cx.listener(|this, _, _, cx| this.refresh_now(cx)))
                .into_any_element(),
        ),
        Health::NotAnswering => Some(
            button("overview-runtimes", "Open Runtimes", tokens)
                .on_click(cx.listener(|this, _, _, cx| this.switch_to(Tab::Runtimes, cx)))
                .into_any_element(),
        ),
        _ => None,
    };
    Some(
        card(tokens)
            .flex_row()
            .items_center()
            .w_full()
            .gap(px(18.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap(px(3.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(7.0))
                            .child(div().w(px(7.0)).h(px(7.0)).rounded(px(4.0)).bg(dot))
                            .child(
                                div()
                                    .text_size(px(type_scale::SECONDARY))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(tokens.text)
                                    .child(label),
                            ),
                    )
                    // An engine's complaint is read in full: it is wrapped,
                    // not cut off at the edge of the band.
                    .child(
                        div()
                            .text_size(px(type_scale::LABEL))
                            .text_color(tokens.muted)
                            .child(note),
                    ),
            )
            .children(way_out),
    )
}
