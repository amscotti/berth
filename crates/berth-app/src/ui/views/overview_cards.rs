//! Overview cards: the four series the history records, the host, and the
//! engine's facts.
//!
//! A series card has the reference's anatomy: a tinted chip and a coloured
//! label, a figure with a note beside it, and the well the series is drawn in.

use berth_core::format;
use berth_core::history::Which;
use berth_ui::layout::{self};
use berth_ui::theme::{TintKind, Tokens, tint};
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Hsla, div, px};

use crate::app::{BerthApp, Tab};
use crate::ui::graph;
use crate::ui::kit::{Card, button, caption, card, card_title, icon_chip};
use crate::ui::tabular;

/// What a series card says about one series: its figure and the note beside it.
pub(crate) struct Reading {
    pub(crate) figure: String,
    pub(crate) note: String,
}

/// What a card is drawn in: its icon, its name, and three colours, each the
/// one job it is for. The label is text on the surface and takes a text
/// colour; the chip is a wash of its tint; the graph takes the series' own.
pub(crate) struct Series {
    pub(crate) icon: &'static str,
    pub(crate) label: &'static str,
    tint: Hsla,
    ink: Hsla,
    line: Hsla,
}

impl Series {
    pub(crate) fn cpu(tokens: Tokens) -> Self {
        Self {
            icon: "containers",
            label: "Containers · CPU",
            tint: tint(tokens, TintKind::Containers),
            ink: tokens.accent_text_pill(),
            line: tokens.accent_fill(),
        }
    }

    pub(crate) fn memory(tokens: Tokens) -> Self {
        Self {
            icon: "containers",
            label: "Containers · Memory",
            tint: tokens.success,
            ink: tokens.success_text(),
            line: tokens.success,
        }
    }

    pub(crate) fn running(tokens: Tokens) -> Self {
        Self {
            icon: "running",
            label: "Containers running",
            tint: tokens.warning_fill,
            ink: tokens.warning_text(),
            line: tokens.warning_fill,
        }
    }

    pub(crate) fn disk(tokens: Tokens) -> Self {
        Self {
            icon: "disk",
            label: "Disk",
            tint: tint(tokens, TintKind::Disk),
            ink: tokens.text,
            line: tokens.muted,
        }
    }

    /// The card with its series in the well. `ceiling` is the value the top
    /// of the well stands for; `None` keeps the well empty rather than
    /// drawing against a ceiling nobody knows.
    pub(crate) fn card(
        self,
        reading: Reading,
        values: Vec<f32>,
        ceiling: Option<f32>,
        width: f32,
        tokens: Tokens,
    ) -> Card {
        let well = graph::cell_graph(values, self.line, tokens, ceiling, layout::GRAPH_HEIGHT);
        self.card_with(reading, well, width, tokens)
    }

    /// The card with something else in its well: the disk card's split of
    /// the stores.
    pub(crate) fn card_with(
        self,
        reading: Reading,
        well: impl IntoElement,
        width: f32,
        tokens: Tokens,
    ) -> Card {
        let Self {
            icon,
            label,
            tint,
            ink,
            line: _,
        } = self;
        card(tokens)
            .w(px(width))
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap(px(8.0))
                    .child(icon_chip(icon, 12.0, tint, tokens, layout::CHIP))
                    .child(
                        div()
                            .text_size(px(type_scale::TITLE_SMALL))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(ink)
                            .child(label),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(type_scale::META))
                            .text_color(tokens.muted)
                            .child("recent"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_end()
                    .w_full()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(type_scale::METRIC))
                            .font_weight(FontWeight::BOLD)
                            .font_features(tabular())
                            .text_color(tokens.text)
                            .child(reading.figure),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(type_scale::SECONDARY))
                            .text_color(tokens.muted)
                            .pb(px(3.0))
                            .child(reading.note),
                    ),
            )
            .child(well)
    }
}

/// The disk card's well: what the engine's stores hold, each as its share of
/// the whole, in the disk page's colours. Empty when the figures haven't
/// arrived.
pub(crate) fn store_well(snapshot: &crate::state::Snapshot, tokens: Tokens) -> AnyElement {
    let bar = match &snapshot.df {
        Some(df) => {
            let shares = df.shares();
            graph::stacked_bar(
                vec![
                    (shares.images, tokens.accent_fill()),
                    (shares.containers, tokens.success),
                    (shares.volumes, tokens.store_volume()),
                    (shares.build_cache, tokens.muted),
                ],
                tokens.track,
            )
            .into_any_element()
        }
        None => {
            caption("The stores' figures arrive on the next slow pass.", tokens).into_any_element()
        }
    };
    div()
        .w_full()
        .h(px(layout::GRAPH_HEIGHT))
        .rounded(px(8.0))
        .bg(tokens.well())
        .border_1()
        .border_color(tokens.well_border())
        .p(px(14.0))
        .flex()
        .flex_col()
        .justify_center()
        .child(bar)
        .into_any_element()
}

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
                    tokens,
                ))
                .child(host_spark(
                    "Host memory",
                    format::bytes(sample.mem_used),
                    history.values(Which::HostMem).to_vec(),
                    tokens.muted,
                    Some(total),
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
        .child(graph::cell_graph(values, color, tokens, ceiling, 44.0))
}

/// Network, disk and PIDs across the fleet, as totals since each container
/// started: one line, no card of its own.
pub(crate) fn io_strip(app: &BerthApp, tokens: Tokens) -> Div {
    let mut net_rx = 0u64;
    let mut net_tx = 0u64;
    let mut blk_read = 0u64;
    let mut blk_write = 0u64;
    let mut pids = 0u32;
    for sample in &app.snapshot.stats {
        if !sample.sampled {
            continue;
        }
        net_rx = net_rx.saturating_add(sample.net_rx);
        net_tx = net_tx.saturating_add(sample.net_tx);
        blk_read = blk_read.saturating_add(sample.blk_read);
        blk_write = blk_write.saturating_add(sample.blk_write);
        pids = pids.saturating_add(sample.pids.unwrap_or(0));
    }
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .w_full()
        .gap_x(px(24.0))
        .gap_y(px(6.0))
        .child(io_stat(
            "Network",
            &format!("↓ {} · ↑ {}", format::bytes(net_rx), format::bytes(net_tx)),
            tokens,
        ))
        .child(io_stat(
            "Disk",
            &format!(
                "read {} · wrote {}",
                format::bytes(blk_read),
                format::bytes(blk_write)
            ),
            tokens,
        ))
        .child(io_stat("PIDs", &pids.to_string(), tokens))
}

fn io_stat(label: &'static str, value: &str, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .child(caption(label, tokens))
        .child(
            div()
                .text_size(px(type_scale::SECONDARY))
                .font_features(tabular())
                .text_color(tokens.text)
                .child(value.to_owned()),
        )
}

/// The ceiling of the memory graph: the engine's `MemTotal`. `None` when the
/// engine reported none, so the graph stays empty rather than filling against
/// a ceiling of one byte.
pub(crate) fn memory_ceiling(runtime: Option<&berth_engine::discovery::Runtime>) -> Option<f32> {
    runtime
        .filter(|runtime| runtime.info_read)
        .map(|runtime| runtime.info.mem_total)
        .filter(|total| *total > 0)
        .map(|total| total as f32)
}

pub(crate) fn engine_card(
    tokens: Tokens,
    width: f32,
    runtime: Option<&berth_engine::discovery::Runtime>,
    info: &berth_engine::dto::Info,
    snapshot: &crate::state::Snapshot,
    cx: &mut Context<BerthApp>,
) -> Card {
    let mut card = card(tokens).w(px(width)).gap(px(6.0)).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .child(card_title("Engine", tokens))
            .child(div().flex_1())
            .child(
                button("open-runtimes", "Runtimes →", tokens)
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.switch_to(Tab::Runtimes, cx))),
            ),
    );
    card = match runtime {
        Some(runtime) if runtime.reachable => card
            .child(
                div()
                    .text_size(px(type_scale::SECONDARY))
                    .text_color(tokens.text)
                    .child(format!(
                        "{} {} · API {} · storage {}",
                        runtime.kind.label(),
                        runtime.version.version,
                        runtime.version.api_version,
                        info.driver
                    )),
            )
            .child(caption(format!("root {}", info.docker_root_dir), tokens))
            .child(caption(
                format!(
                    "{} · {}/{} · cgroups {}",
                    info.operating_system, info.os_type, info.architecture, info.cgroup_version
                ),
                tokens,
            ))
            .child(div().w_full().h(px(1.0)).my(px(2.0)).bg(tokens.border))
            .child(caption(
                format!(
                    "{} · {} · {} · {}",
                    format::count(snapshot.containers.len(), "container", "containers"),
                    format::count(snapshot.images.len(), "image", "images"),
                    format::count(snapshot.volumes.len(), "volume", "volumes"),
                    format::count(snapshot.networks.len(), "network", "networks"),
                ),
                tokens,
            )),
        Some(_) => card.child(caption(
            "Nothing is answering on the sockets berth knows. Open Runtimes to see what was tried.",
            tokens,
        )),
        None => card.child(caption(
            "No engine selected. Open Runtimes to see what was found.",
            tokens,
        )),
    };
    card.child(
        div()
            .flex()
            .flex_row()
            .w_full()
            .justify_end()
            .pt(px(2.0))
            .child(
                button("overview-refresh", "Refresh now", tokens)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_now(cx))),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(info_read: bool, mem_total: u64) -> berth_engine::discovery::Runtime {
        berth_engine::discovery::Runtime {
            info_read,
            info: berth_engine::dto::Info {
                mem_total,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn the_memory_ceiling_is_the_engine_total_and_nothing_else() {
        assert_eq!(
            memory_ceiling(Some(&runtime(true, 4_000_000_000))),
            Some(4.0e9)
        );
        // No engine, an engine that didn't answer /info, or one that reported
        // no total: the ceiling is unknown, not one.
        assert_eq!(memory_ceiling(None), None);
        assert_eq!(memory_ceiling(Some(&runtime(false, 4_000_000_000))), None);
        assert_eq!(memory_ceiling(Some(&runtime(true, 0))), None);
    }
}
