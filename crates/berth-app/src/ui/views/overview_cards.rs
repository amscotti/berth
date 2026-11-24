//! Overview cards: the four series the history records, the host, and the
//! engine's facts.
//!
//! A series card has the reference's anatomy: a tinted chip and a coloured
//! label, a figure with a note beside it, and the well the series is drawn in.

use berth_core::format;
use berth_ui::layout;
use berth_ui::theme::{TintKind, Tokens, tint};
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Hsla, div, px};

use crate::app::{BerthApp, Tab};
use crate::ui::graph;
use crate::ui::kit::{Card, button, caption, card, card_title, icon_chip};
use crate::ui::tabular;

/// What a series card says about one series: its figure and the note beside
/// it. `partial` colours the note as a warning: the figure is a lower bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    pub(crate) figure: String,
    pub(crate) note: String,
    pub(crate) partial: bool,
}

/// What the CPU and memory cards say about the engine's sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sample {
    pub(crate) cpu: Reading,
    pub(crate) memory: Reading,
}

/// The CPU and memory readings. A complete sample is stated plainly. A
/// partial one is stated with how partial: the figure is what was measured,
/// and the note says "partial sample · 49 of 50". An engine that is not
/// answering, or has not answered yet, or measured nothing, is unknown.
pub(crate) fn sample(
    snapshot: &crate::state::Snapshot,
    answering: bool,
    cores: u32,
    memory_ceiling: Option<u64>,
) -> Sample {
    let unknown = |note: String| Reading {
        figure: "—".to_owned(),
        note,
        partial: false,
    };
    if !answering {
        return Sample {
            cpu: unknown(String::new()),
            memory: unknown(String::new()),
        };
    }
    let coverage = snapshot.coverage();
    if coverage.measured == 0 {
        return Sample {
            cpu: unknown(String::new()),
            memory: unknown(String::new()),
        };
    }
    let (cpu_note, memory_note, partial) = if coverage.measured < coverage.up {
        let note = format!("partial sample · {}", coverage.said());
        (note.clone(), note, true)
    } else {
        (
            format!("of {}", format::count(cores as usize, "core", "cores")),
            memory_ceiling
                .map(|total| format!("of {}", format::bytes(total)))
                .unwrap_or_else(|| "engine total unknown".to_owned()),
            false,
        )
    };
    Sample {
        cpu: Reading {
            figure: snapshot
                .measured_cpu(cores.max(1) as f32)
                .map(|value| format::percent(value as f64))
                .unwrap_or_else(|| "—".to_owned()),
            note: cpu_note,
            partial,
        },
        memory: Reading {
            figure: format::bytes(snapshot.container_memory()),
            note: memory_note,
            partial,
        },
    }
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
    /// What its ceiling is counted in, so that the well can name it.
    unit: Unit,
}

/// What a series is measured in.
#[derive(Clone, Copy)]
pub(crate) enum Unit {
    Percent,
    Bytes,
    Count,
}

impl Unit {
    /// The ceiling's name, on the top edge of the well.
    pub(crate) fn ceiling(self, ceiling: Option<f32>) -> Option<String> {
        ceiling.map(|ceiling| match self {
            Unit::Percent => "100%".to_owned(),
            Unit::Bytes => format::bytes(ceiling as u64),
            Unit::Count => format::count(ceiling as usize, "container", "containers"),
        })
    }
}

impl Series {
    pub(crate) fn cpu(tokens: Tokens) -> Self {
        Self {
            icon: "containers",
            label: "Containers · CPU",
            tint: tint(tokens, TintKind::Containers),
            ink: tokens.accent_text_pill(),
            line: tokens.accent_fill(),
            unit: Unit::Percent,
        }
    }

    pub(crate) fn memory(tokens: Tokens) -> Self {
        Self {
            icon: "containers",
            label: "Containers · Memory",
            tint: tokens.success,
            ink: tokens.success_text(),
            line: tokens.success,
            unit: Unit::Bytes,
        }
    }

    pub(crate) fn running(tokens: Tokens) -> Self {
        Self {
            icon: "running",
            label: "Containers running",
            tint: tokens.warning_fill,
            ink: tokens.warning_text(),
            line: tokens.warning_fill,
            unit: Unit::Count,
        }
    }

    pub(crate) fn disk(tokens: Tokens) -> Self {
        Self {
            icon: "disk",
            label: "Disk",
            tint: tint(tokens, TintKind::Disk),
            ink: tokens.text,
            line: tokens.muted,
            unit: Unit::Count,
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
        let well = graph::cell_graph(
            values,
            self.line,
            tokens,
            ceiling,
            layout::GRAPH_HEIGHT,
            self.unit.ceiling(ceiling),
        );
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
        let partial = reading.partial;
        let Self {
            icon,
            label,
            tint,
            ink,
            line: _,
            unit: _,
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
                            .text_color(if partial {
                                tokens.warning_text()
                            } else {
                                tokens.muted
                            })
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
#[path = "overview_cards_tests.rs"]
mod tests;
