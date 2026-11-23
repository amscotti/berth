//! The container table's rows and cells. Split from the page frame so a cell
//! change cannot make the header module unreadable.

use berth_core::format;
use berth_engine::detail::up_seconds;
use berth_engine::dto::Container;
use berth_engine::stats::Sample;
use berth_ui::layout::{self, ContainerColumns};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Hsla, IntoElement, Role, div, px};

use super::row_actions::action_cell;
use super::tabular;
use crate::app::BerthApp;

/// How a row is shaded. Striping follows the row's position on screen, not
/// its place in the snapshot: a sorted or filtered table still alternates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shade {
    Selected,
    Alt,
    Plain,
}

pub(crate) fn shade(position: usize, selected: bool) -> Shade {
    if selected {
        Shade::Selected
    } else if position % 2 == 1 {
        Shade::Alt
    } else {
        Shade::Plain
    }
}

impl Shade {
    fn background(self, tokens: Tokens) -> gpui::Hsla {
        match self {
            Shade::Selected => tokens.accent.alpha(0.10),
            Shade::Alt => tokens.surface_hover.alpha(0.35),
            Shade::Plain => tokens.surface,
        }
    }
}

/// One container row. `index` is its place in the snapshot and `position` its
/// place among the rows on screen: striping follows the position, so a sorted
/// or filtered table still alternates evenly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn container_row(
    app: &BerthApp,
    index: usize,
    position: usize,
    container: &Container,
    stat: Sample,
    selected: bool,
    tokens: Tokens,
    columns: ContainerColumns,
    cx: &mut Context<BerthApp>,
) -> AnyElement {
    let row_id = gpui::SharedString::from(format!("container-{}", container.id));
    let mut element = div()
        .id(row_id)
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .h(px(layout::ROW_CONTAINER))
        .px(px(12.0))
        .gap(px(10.0))
        .rounded(px(4.0))
        .bg(shade(position, selected).background(tokens))
        .when(!selected, |this| {
            this.hover(move |style| style.bg(tokens.surface_hover))
        })
        .role(Role::Row)
        .aria_label(container.name())
        .aria_selected(selected)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select(index, cx);
        }));

    if app.bulk_enabled {
        let checked = app.bulk_selected.contains(&container.id);
        let check_id = gpui::SharedString::from(format!("bulk-check-{}", container.id));
        let id = container.id.clone();
        element = element.child(
            div()
                .id(check_id)
                .flex()
                .items_center()
                .justify_center()
                .w(px(28.0))
                .h(px(20.0))
                .role(Role::CheckBox)
                .aria_label(format!("Select {}", container.name()))
                .aria_selected(checked)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.bulk_toggle(&id, cx);
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .w(px(14.0))
                        .h(px(14.0))
                        .rounded(px(4.0))
                        .bg(if checked { tokens.accent } else { tokens.track })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(checked, |this| {
                            this.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(tokens.accent_text)
                                    .child("✓"),
                            )
                        }),
                ),
        );
        element = element.child(name_cell(container, columns.name - 28.0, tokens));
    } else {
        element = element.child(name_cell(container, columns.name, tokens));
    }
    element = element.child(pill(status_label(container), tokens));
    element = element.child(text_cell(
        if container.is_up() && stat.sampled && stat.cpu_known {
            format::percent(stat.cpu)
        } else {
            "—".to_owned()
        },
        columns.cpu,
        true,
        tokens,
    ));
    if columns.show_mem {
        element = element.child(text_cell(
            if container.is_up() && stat.sampled && stat.mem_known {
                format::bytes(stat.mem)
            } else {
                "—".to_owned()
            },
            columns.mem,
            true,
            tokens,
        ));
    }
    if columns.show_net {
        element = element.child(io_cell(
            "↓",
            if stat.sampled {
                format::bytes(stat.net_rx)
            } else {
                "—".to_owned()
            },
            "↑",
            if stat.sampled {
                format::bytes(stat.net_tx)
            } else {
                "—".to_owned()
            },
            columns.net,
            tokens,
        ));
    }
    if columns.show_disk {
        element = element.child(io_cell(
            "read",
            if stat.sampled && stat.blk_known {
                format::bytes(stat.blk_read)
            } else {
                "—".to_owned()
            },
            "wrote",
            if stat.sampled && stat.blk_known {
                format::bytes(stat.blk_write)
            } else {
                "—".to_owned()
            },
            columns.disk,
            tokens,
        ));
    }
    if columns.show_pids {
        element = element.child(text_cell(
            stat.pids
                .map(|pids| pids.to_string())
                .unwrap_or_else(|| "—".into()),
            columns.pids,
            true,
            tokens,
        ));
    }
    if columns.show_uptime {
        element = element.child(text_cell(
            up_seconds(&container.status)
                .map(format::duration)
                .unwrap_or_else(|| "—".into()),
            columns.uptime,
            false,
            tokens,
        ));
    }
    if columns.show_ports {
        element = element.child(ports_cell(container, columns.ports, tokens, cx));
    }
    element
        .child(action_cell(container, columns.actions, tokens, cx))
        .into_any_element()
}

fn status_label(container: &Container) -> &str {
    container.health().unwrap_or(&container.state)
}

pub(crate) fn empty_state(heading: &str, detail: &str, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .w_full()
        .py(px(48.0))
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(tokens.text)
                .child(heading.to_owned()),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.muted)
                .child(detail.to_owned()),
        )
}

pub(crate) fn header_cell(label: &'static str, width: f32, align_end: bool, tokens: Tokens) -> Div {
    let mut cell = div()
        .flex()
        .flex_row()
        .items_center()
        .w(px(width))
        .h(px(layout::TABLE_HEADER))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.muted)
                .child(label),
        );
    if align_end {
        cell = cell.justify_end().pr(px(8.0));
    }
    cell
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sortable_header_cell(
    table: usize,
    col: usize,
    label: &'static str,
    width: f32,
    align_end: bool,
    sort: (usize, bool),
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement + use<> {
    let active = sort.0 == col;
    let arrow = if active {
        if sort.1 { " ▲" } else { " ▼" }
    } else {
        ""
    };
    let aria = if active {
        format!(
            "{label}, sorted {}",
            if sort.1 { "ascending" } else { "descending" }
        )
    } else {
        label.to_owned()
    };
    let mut cell = div()
        .id(("sort-header", table * 16 + col))
        .flex()
        .flex_row()
        .items_center()
        .w(px(width))
        .h(px(layout::TABLE_HEADER))
        .role(Role::ColumnHeader)
        .aria_label(aria)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_sort(table, col, cx);
        }))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if active { tokens.accent } else { tokens.muted })
                .child(format!("{label}{arrow}")),
        );
    if align_end {
        cell = cell.justify_end().pr(px(8.0));
    }
    cell
}

fn name_cell(container: &Container, width: f32, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_col()
        .justify_center()
        .w(px(width))
        .gap(px(1.0))
        .child(
            div()
                .text_size(px(type_scale::BODY))
                .text_color(tokens.text)
                .truncate()
                .child(container.name().to_owned()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(tokens.muted)
                .truncate()
                .child(container.image.clone()),
        )
}

pub(crate) fn text_cell(text: String, width: f32, align_end: bool, tokens: Tokens) -> Div {
    let mut cell = div().flex().flex_row().items_center().w(px(width)).child(
        div()
            .text_size(px(type_scale::BODY))
            .font_features(tabular())
            .text_color(tokens.text)
            .truncate()
            .child(text),
    );
    if align_end {
        cell = cell.justify_end().pr(px(8.0));
    }
    cell
}

fn io_cell(
    first_label: &'static str,
    first: String,
    second_label: &'static str,
    second: String,
    width: f32,
    tokens: Tokens,
) -> Div {
    div()
        .flex()
        .flex_col()
        .justify_center()
        .w(px(width))
        .gap(px(1.0))
        .child(io_line(first_label, first, tokens))
        .child(io_line(second_label, second, tokens))
}

fn io_line(label: &'static str, value: String, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .text_size(px(11.0))
                .font_features(tabular())
                .text_color(tokens.text)
                .child(value),
        )
}

pub(crate) fn pill(state: &str, tokens: Tokens) -> Div {
    let color = status_color(tokens, state);
    div()
        .flex()
        .items_center()
        .h(px(20.0))
        .px(px(8.0))
        .rounded(px(10.0))
        .bg(color.alpha(0.14))
        .child(
            div()
                .text_size(px(type_scale::PILL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .child(state.to_owned()),
        )
}

fn ports_cell(
    container: &Container,
    width: f32,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let ports = container.published_ports();
    let mut cell = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .w(px(width));
    if let Some(binding) = ports.first() {
        let host = binding.ip.clone();
        let port = binding.port;
        cell = cell.child(
            div()
                .id(gpui::SharedString::from(format!(
                    "row-port-{}-{}",
                    container.id, port
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
                    cx.stop_propagation();
                    this.open_port(&host, port, cx);
                }))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_features(tabular())
                        .text_color(tokens.warning_text())
                        .child(binding.port.to_string()),
                ),
        );
        if ports.len() > 1 {
            cell = cell.child(
                div()
                    .text_size(px(10.5))
                    .text_color(tokens.muted)
                    .child(format!("+{}", ports.len() - 1)),
            );
        }
    } else {
        cell = cell.child(
            div()
                .text_size(px(type_scale::BODY))
                .text_color(tokens.muted)
                .child("—"),
        );
    }
    cell
}

pub(crate) fn status_color(tokens: Tokens, state: &str) -> Hsla {
    match state {
        "running" | "healthy" => tokens.success_text(),
        "exited" | "stopped" => tokens.muted_text_pill(),
        "paused" | "restarting" | "starting" => tokens.warning_text(),
        "dead" | "unhealthy" => tokens.danger_text(),
        "created" => tokens.accent_text_pill(),
        _ => tokens.muted_text_pill(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn striping_follows_the_position_on_screen_not_the_snapshot() {
        // Rows two and four of the snapshot, shown as the first and second
        // rows of a filtered table: they still alternate.
        for (position, shade_expected) in [(0, Shade::Plain), (1, Shade::Alt), (2, Shade::Plain)] {
            assert_eq!(shade(position, false), shade_expected);
        }
        // Whatever its position, the open row is the selected one.
        assert_eq!(shade(0, true), Shade::Selected);
        assert_eq!(shade(3, true), Shade::Selected);
    }
}
