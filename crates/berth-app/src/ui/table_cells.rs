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

use super::tabular;
use crate::app::BerthApp;

pub(crate) fn container_row(
    app: &BerthApp,
    container: &Container,
    stat: Sample,
    selected: bool,
    tokens: Tokens,
    columns: ContainerColumns,
    cx: &mut Context<BerthApp>,
) -> AnyElement {
    let actual_index = app
        .snapshot
        .containers
        .iter()
        .position(|candidate| candidate.id == container.id)
        .unwrap_or(0);
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
        .bg(if selected {
            tokens.accent.alpha(0.10)
        } else if actual_index % 2 == 1 {
            tokens.surface_hover.alpha(0.35)
        } else {
            tokens.surface
        })
        .when(!selected, |this| {
            this.hover(move |style| style.bg(tokens.surface_hover))
        })
        .role(Role::Row)
        .aria_label(container.name())
        .aria_selected(selected)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select(actual_index, cx);
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

fn action_cell(
    container: &Container,
    width: f32,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let name = container.name().to_owned();
    let id = container.id.clone();
    let mut cell = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .w(px(width))
        .gap(px(4.0));
    if container.is_up() {
        cell = cell
            .child(action_pill(
                "Stop",
                &id,
                0,
                false,
                tokens,
                "POST",
                format!("/containers/{id}/stop"),
                "Stopped",
                false,
                &name,
                cx,
            ))
            .child(action_pill(
                "Restart",
                &id,
                1,
                false,
                tokens,
                "POST",
                format!("/containers/{id}/restart"),
                "Restarted",
                false,
                &name,
                cx,
            ))
            .child(action_pill(
                "Delete",
                &id,
                2,
                true,
                tokens,
                "DELETE",
                format!("/containers/{id}?force=1"),
                "Delete",
                true,
                &name,
                cx,
            ));
    } else {
        cell = cell
            .child(action_pill(
                "Start",
                &id,
                3,
                false,
                tokens,
                "POST",
                format!("/containers/{id}/start"),
                "Started",
                false,
                &name,
                cx,
            ))
            .child(action_pill(
                "Delete",
                &id,
                4,
                true,
                tokens,
                "DELETE",
                format!("/containers/{id}?force=1"),
                "Delete",
                true,
                &name,
                cx,
            ));
    }
    cell
}

#[allow(clippy::too_many_arguments)]
fn action_pill(
    label: &'static str,
    container_id: &str,
    slot: usize,
    danger: bool,
    tokens: Tokens,
    method: &'static str,
    path: String,
    verb: &'static str,
    confirm: bool,
    name: &str,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let color = if danger {
        tokens.danger_text()
    } else {
        tokens.text
    };
    let click_name = name.to_owned();
    let method = method.to_owned();
    div()
        .id(gpui::SharedString::from(format!(
            "action-{container_id}-{slot}"
        )))
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .px(px(8.0))
        .rounded(px(12.0))
        .bg(if danger {
            tokens.danger_text().alpha(0.10)
        } else {
            tokens.surface_hover
        })
        .role(Role::Button)
        .aria_label(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            if confirm {
                this.confirm_action(
                    method.clone(),
                    path.clone(),
                    verb.to_owned(),
                    click_name.clone(),
                    cx,
                );
            } else {
                this.run_action(method.clone(), path.clone(), verb.to_owned(), cx);
            }
        }))
        .child(
            div()
                .text_size(px(type_scale::SMALL_BUTTON))
                .text_color(color)
                .child(label),
        )
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
