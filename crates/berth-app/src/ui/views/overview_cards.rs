//! Overview cards: fleet activity, host split, engine facts and disk stores.

use berth_core::format;
use berth_core::history::Which;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{Context, Div, FontWeight, div, px};

use crate::app::{BerthApp, Tab};
use crate::ui::graph;
use crate::ui::kit::{Card, button, caption, card, card_title};

pub(crate) fn fleet_card(
    app: &BerthApp,
    tokens: Tokens,
    width: f32,
    complete: bool,
    cpu: &str,
    memory: &str,
) -> Card {
    let chart_w = ((width - 32.0 - 12.0) / 2.0).max(120.0);
    let mut card = card(tokens).w(px(width)).gap(px(10.0)).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .gap(px(8.0))
            .child(card_title("Fleet activity", tokens))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(type_scale::META))
                    .text_color(tokens.muted)
                    .child("recent"),
            ),
    );
    card = card.child(
        div()
            .flex()
            .flex_row()
            .items_end()
            .w_full()
            .gap(px(14.0))
            .child(
                div()
                    .text_size(px(type_scale::METRIC))
                    .font_weight(FontWeight::BOLD)
                    .font_features(crate::ui::tabular())
                    .text_color(tokens.text)
                    .child(app.snapshot.running().to_string()),
            )
            .child(
                div().flex().flex_col().gap(px(1.0)).pb(px(3.0)).child(
                    div()
                        .text_size(px(type_scale::SECONDARY))
                        .text_color(tokens.text)
                        .child(if app.snapshot.running() == 1 {
                            "container running"
                        } else {
                            "containers running"
                        }),
                ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(type_scale::LABEL))
                    .text_color(if complete {
                        tokens.muted
                    } else {
                        tokens.warning_text()
                    })
                    .child(if complete {
                        "live sample".to_owned()
                    } else {
                        "partial sample".to_owned()
                    }),
            ),
    );
    card = card.child(
        div()
            .flex()
            .flex_row()
            .w_full()
            .gap(px(12.0))
            .child(spark(
                "CPU",
                cpu,
                app.engine.history.values(Which::Cpu).to_vec(),
                tokens.accent_fill(),
                tokens,
                chart_w,
                Some(100.0),
            ))
            .child(spark(
                "Memory",
                memory,
                app.engine.history.values(Which::Mem).to_vec(),
                tokens.success,
                tokens,
                chart_w,
                memory_ceiling(app.engine.active_runtime()),
            )),
    );
    card.child(div().w_full().h(px(1.0)).bg(tokens.border))
        .child(fleet_io(app, tokens))
}

fn fleet_io(app: &BerthApp, tokens: Tokens) -> Div {
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
        .items_center()
        .w_full()
        .gap(px(24.0))
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
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_features(crate::ui::tabular())
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

fn spark(
    label: &'static str,
    value: &str,
    values: Vec<f32>,
    color: gpui::Hsla,
    tokens: Tokens,
    width: f32,
    ceiling: Option<f32>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(width))
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .w_full()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(type_scale::LABEL))
                        .text_color(tokens.muted)
                        .child(label),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(type_scale::SECONDARY))
                        .font_weight(FontWeight::SEMIBOLD)
                        .font_features(crate::ui::tabular())
                        .text_color(tokens.text)
                        .child(value.to_owned()),
                ),
        )
        .child(graph::cell_graph(values, color, tokens, ceiling, 56.0))
}

pub(crate) fn host_card(
    app: &BerthApp,
    tokens: Tokens,
    width: f32,
    containers_mem: Option<u64>,
) -> Card {
    let sample = &app.snapshot.host;
    let mut card = card(tokens).w(px(width)).gap(px(10.0)).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .child(card_title("Host", tokens))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(type_scale::LABEL))
                    .font_features(crate::ui::tabular())
                    .text_color(tokens.muted)
                    .child(format!(
                        "CPU {}",
                        if sample.cpu_known {
                            format::percent(sample.cpu_percent as f64)
                        } else {
                            "—".to_owned()
                        }
                    )),
            ),
    );
    if sample.mem_total == 0 {
        return card.child(caption(
            "Host memory is unavailable on this platform.",
            tokens,
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
                    .font_features(crate::ui::tabular())
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
    let Some(containers_mem) = containers_mem else {
        return card.child(caption(
            "Containers — · per-container stats incomplete",
            tokens,
        ));
    };
    let used = sample.mem_used.max(1);
    let in_containers = containers_mem.min(used);
    let engine = sample.engine_rss.min(used.saturating_sub(in_containers));
    let elsewhere = used.saturating_sub(in_containers).saturating_sub(engine);
    let total = sample.mem_total.max(1) as f32;
    let free = sample.mem_total.saturating_sub(used);
    card = card
        .child(graph::stacked_bar(
            vec![
                (in_containers as f32 / total, tokens.accent_fill()),
                (engine as f32 / total, tokens.warning_fill),
                (elsewhere as f32 / total, tokens.elsewhere()),
                (free as f32 / total, tokens.track),
            ],
            tokens.track,
        ))
        .child(div().w_full().h(px(1.0)).bg(tokens.border))
        .child(host_split(
            "containers",
            in_containers,
            used,
            tokens.accent_fill(),
            tokens,
        ))
        .child(host_split(
            "engine helpers",
            engine,
            used,
            tokens.warning_fill,
            tokens,
        ))
        .child(host_split(
            "elsewhere",
            elsewhere,
            used,
            tokens.muted,
            tokens,
        ));
    card
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

fn host_split(
    label: &'static str,
    bytes: u64,
    used: u64,
    color: gpui::Hsla,
    tokens: Tokens,
) -> Div {
    let fraction = if used > 0 {
        bytes as f32 / used as f32
    } else {
        0.0
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(7.0))
        .child(div().w(px(7.0)).h(px(7.0)).rounded(px(4.0)).bg(color))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(div().flex_1())
        .child(
            div()
                .w(px(64.0))
                .h(px(4.0))
                .rounded(px(2.0))
                .bg(tokens.track)
                .child(
                    div()
                        .w(px((64.0 * fraction).max(if bytes > 0 { 2.0 } else { 0.0 })))
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(color),
                ),
        )
        .child(
            div()
                .w(px(82.0))
                .text_size(px(type_scale::LABEL))
                .font_features(crate::ui::tabular())
                .text_color(tokens.text)
                .child(format::bytes(bytes)),
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
