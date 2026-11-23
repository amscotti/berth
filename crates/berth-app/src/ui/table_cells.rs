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
use gpui::{AnyElement, Context, Div, FontWeight, Hsla, IntoElement, Role, SharedString, div, px};

use super::kit::{button, checkbox};
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
            Shade::Selected => tokens.selected_row(),
            Shade::Alt => tokens.alt_row(),
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
        .px(px(layout::ROW_PADDING))
        .gap(px(layout::ROW_GAP))
        .rounded(px(layout::RADIUS_ROW))
        .bg(shade(position, selected).background(tokens))
        .when(!selected, |this| {
            this.hover(move |style| style.bg(tokens.surface_hover))
        })
        .role(Role::Row)
        .aria_label(container.name())
        .aria_selected(selected)
        // While the table has focus it speaks for its selected row, as a
        // list does: the row is what assistive technology announces.
        .when(selected, |this| this.aria_active_descendant())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select(index, cx);
        }));

    if app.view.bulk_enabled {
        let checked = app.view.bulk_selected.contains(&container.id);
        let id = container.id.clone();
        element = element.child(
            checkbox(
                SharedString::from(format!("bulk-check-{}", container.id)),
                format!("Select {}", container.name()),
                checked,
                tokens,
            )
            .box_only()
            // The table is the tab stop: a hundred rows must not mean a
            // hundred presses. Space toggles the selected row's box.
            .tab_stop(false)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.bulk_toggle(&id, cx);
                cx.stop_propagation();
            })),
        );
        element = element.child(name_cell(
            container,
            columns.name - layout::BULK_COLUMN,
            tokens,
        ));
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
                .text_size(px(type_scale::BODY))
                .text_color(tokens.text)
                .child(heading.to_owned()),
        )
        .child(
            div()
                .text_size(px(type_scale::SECONDARY))
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
        cell = cell.justify_end().pr(px(layout::CELL_PADDING_END));
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
        .h(px(layout::CONTROL_SMALL))
        .rounded(px(layout::RADIUS_ROW))
        // The hairline that the focus ring is drawn in.
        .border_1()
        .border_color(tokens.invisible())
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
        cell = cell.justify_end().pr(px(layout::CELL_PADDING_END));
    }
    // A header sorts, so it is a control: Tab reaches it, and Enter or Space
    // sorts by it.
    crate::ui::kit::focusable(cell, tokens.focus_ring())
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
                .text_size(px(type_scale::LABEL))
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
        cell = cell.justify_end().pr(px(layout::CELL_PADDING_END));
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
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .text_size(px(type_scale::LABEL))
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
        .flex_shrink_0()
        .h(px(layout::PILL_HEIGHT))
        .px(px(layout::PILL_PADDING))
        .rounded(px(layout::PILL_HEIGHT / 2.0))
        .bg(tokens.pill_wash(color))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
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
            button(
                SharedString::from(format!("row-port-{}-{port}", container.id)),
                binding.port.to_string(),
                tokens,
            )
            .small()
            .port()
            .tabular()
            .name(format!("Open port {port}"))
            // Skipped by Tab, like everything else that repeats in a row.
            .tab_stop(false)
            .on_click(cx.listener(move |this, _, _, cx| {
                // The press opens the port; it doesn't open the row.
                cx.stop_propagation();
                this.open_port(&host, port, cx);
            })),
        );
        if ports.len() > 1 {
            cell = cell.child(
                div()
                    .text_size(px(type_scale::LABEL))
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
