//! Containers: the table, with its header, its bulk bar and its cells.

use berth_core::format;
use berth_engine::detail::up_seconds;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, SharedString, div, px};

use crate::app::{BerthApp, Filter, SearchKind};
use crate::ui::data_table::{Column, DataTable, RowKey, pill, text_cell};
use crate::ui::kit::{Button, button, caption, checkbox};
use crate::ui::page::{empty_state, page_header};
use crate::ui::row_actions::action_cell;
use crate::ui::tabular;

pub fn containers(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let columns = layout::container_columns(widths.content, app.view.bulk_enabled);
    let page = page_header(
        app,
        widths,
        Some(SearchKind::Containers),
        Some("Select"),
        cx,
    );
    let content = div().flex().flex_col().w_full().flex_1().min_h(px(0.0));

    if app.snapshot.containers.is_empty() {
        let (heading, detail) = if !app.engine.error.is_empty() {
            ("Engine not answering", app.engine.error.clone())
        } else if app.engine.last_update == 0 {
            (
                "Reading engine…",
                "Sockets are being probed in the background.".to_owned(),
            )
        } else {
            (
                "No containers",
                "Start one, or check Runtimes for the engine.".to_owned(),
            )
        };
        return content
            .child(page)
            .child(empty_state(heading, &detail, tokens))
            .into_any_element();
    }

    let rows = app.visible_rows();
    if rows.is_empty() {
        let message = if !app.query(SearchKind::Containers).is_empty() {
            format!("No results for “{}”", app.query(SearchKind::Containers))
        } else {
            match app.view.filter {
                Filter::Stopped => "No stopped containers — All shows every container".to_owned(),
                Filter::Running => "No running containers — All shows every container".to_owned(),
                Filter::All => "No containers".to_owned(),
            }
        };
        return content
            .child(page)
            .child(empty_state(
                &message,
                "Clear the filter or search to see every container.",
                tokens,
            ))
            .into_any_element();
    }

    let mut table = DataTable::new(
        "containers-table",
        "Containers",
        "table-header",
        0,
        row_key,
        open_row,
    )
    .header_label("Containers table header")
    .focus(app.view.table_focus.clone());
    // Bulk mode borrows room from the name column for a checkbox; it does not
    // widen the row.
    let name = if app.view.bulk_enabled {
        table = table.column(Column::plain(
            "bulk",
            "",
            layout::BULK_COLUMN,
            false,
            bulk_cell,
        ));
        columns.name - layout::BULK_COLUMN
    } else {
        columns.name
    };
    let mut table = table
        .column(Column::sorted("name", "NAME", name, false, 0, name_cell))
        .column(Column::sorted(
            "status",
            "STATUS",
            columns.status,
            false,
            1,
            status_cell,
        ))
        .column(Column::sorted("cpu", "CPU", columns.cpu, true, 2, cpu_cell));
    if columns.show_mem {
        table = table.column(Column::sorted("mem", "MEM", columns.mem, true, 3, mem_cell));
    }
    if columns.show_net {
        table = table.column(Column::sorted(
            "net",
            "NET",
            columns.net,
            false,
            4,
            net_cell,
        ));
    }
    if columns.show_disk {
        table = table.column(Column::sorted(
            "disk",
            "DISK",
            columns.disk,
            false,
            5,
            disk_cell,
        ));
    }
    if columns.show_pids {
        table = table.column(Column::sorted(
            "pids",
            "PIDS",
            columns.pids,
            true,
            6,
            pids_cell,
        ));
    }
    if columns.show_uptime {
        table = table.column(Column::sorted(
            "uptime",
            "UPTIME",
            columns.uptime,
            false,
            7,
            uptime_cell,
        ));
    }
    if columns.show_ports {
        table = table.column(Column::sorted(
            "ports",
            "PORTS",
            columns.ports,
            false,
            8,
            ports_cell,
        ));
    }
    let mut table = table.column(Column::plain(
        "actions",
        "ACTIONS",
        columns.actions,
        true,
        actions_cell,
    ));

    let header = table.header(app, tokens, cx);
    if app.view.bulk_enabled {
        table = table.above(bulk_bar(app, tokens, cx));
    }
    let table = table.body(
        rows,
        layout::ROW_CONTAINER,
        &app.view.lists[SearchKind::Containers as usize].scroll,
        tokens,
        cx,
    );
    content
        .child(page)
        .child(header)
        .child(table)
        .into_any_element()
}

fn row_key(app: &BerthApp, ix: usize) -> RowKey {
    let container = &app.snapshot.containers[ix];
    RowKey {
        id: SharedString::from(format!("container-{}", container.id)),
        name: container.name().to_owned(),
        selected: app.view.selected_id.as_deref() == Some(container.id.as_str()),
        opens: true,
    }
}

fn open_row(app: &mut BerthApp, ix: usize, cx: &mut Context<BerthApp>) {
    app.select(ix, cx);
}

// --- cells ---------------------------------------------------------------

fn bulk_cell(app: &mut BerthApp, ix: usize, _width: f32, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
    let checked = app.view.bulk_selected.contains(&container.id);
    let id = container.id.clone();
    checkbox(
        SharedString::from(format!("bulk-check-{}", container.id)),
        format!("Select {}", container.name()),
        checked,
        tokens,
    )
    .box_only()
    // The table is the tab stop: a hundred rows must not mean a hundred
    // presses. Space toggles the selected row's box.
    .tab_stop(false)
    .on_click(cx.listener(move |this, _, _, cx| {
        this.bulk_toggle(&id, cx);
        cx.stop_propagation();
    }))
    .into_any_element()
}

fn name_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
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
        .into_any_element()
}

fn status_cell(
    app: &mut BerthApp,
    ix: usize,
    _width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
    pill(container.health().unwrap_or(&container.state), tokens).into_any_element()
}

fn cpu_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
    let stat = app.snapshot.stat(ix);
    let text = if container.is_up() && stat.sampled && stat.cpu_known {
        format::percent(stat.cpu)
    } else {
        "—".to_owned()
    };
    text_cell(text, width, true, tokens).into_any_element()
}

fn mem_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
    let stat = app.snapshot.stat(ix);
    let text = if container.is_up() && stat.sampled && stat.mem_known {
        format::bytes(stat.mem)
    } else {
        "—".to_owned()
    };
    text_cell(text, width, true, tokens).into_any_element()
}

fn net_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let stat = app.snapshot.stat(ix);
    let (rx, tx) = figures(stat.sampled, stat.net_rx, stat.net_tx);
    io_cell("↓", rx, "↑", tx, width, tokens).into_any_element()
}

fn disk_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let stat = app.snapshot.stat(ix);
    let (read, wrote) = if stat.sampled && stat.blk_known {
        (format::bytes(stat.blk_read), format::bytes(stat.blk_write))
    } else {
        ("—".to_owned(), "—".to_owned())
    };
    io_cell("read", read, "wrote", wrote, width, tokens).into_any_element()
}

/// Two figures as the engine gave them, or two dashes when it gave none.
fn figures(known: bool, first: u64, second: u64) -> (String, String) {
    if known {
        (format::bytes(first), format::bytes(second))
    } else {
        ("—".to_owned(), "—".to_owned())
    }
}

fn pids_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let stat = app.snapshot.stat(ix);
    let text = stat
        .pids
        .map(|pids| pids.to_string())
        .unwrap_or_else(|| "—".into());
    text_cell(text, width, true, tokens).into_any_element()
}

fn uptime_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
    let text = up_seconds(&container.status)
        .map(format::duration)
        .unwrap_or_else(|| "—".into());
    text_cell(text, width, false, tokens).into_any_element()
}

fn ports_cell(app: &mut BerthApp, ix: usize, width: f32, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
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
    cell.into_any_element()
}

fn actions_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    cx: &mut Context<BerthApp>,
) -> AnyElement {
    let tokens = app.tokens;
    let container = &app.snapshot.containers[ix];
    action_cell(container, width, tokens, cx).into_any_element()
}

/// A cell of two lines, each a label and a figure: network read and written,
/// or disk.
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

// --- bulk ----------------------------------------------------------------

/// What to do with every selected container while several can be picked.
fn bulk_bar(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Div {
    let none_selected = app.view.bulk_selected.is_empty();
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(8.0))
        .py(px(6.0))
        .child(caption(
            format!("{} selected", app.view.bulk_selected.len()),
            tokens,
        ))
        .child(
            button("bulk-All", "All", tokens)
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.bulk_select_all(cx))),
        )
        .child(
            button("bulk-None", "None", tokens)
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.bulk_clear(cx))),
        )
        .child(div().flex_1())
        .children(
            crate::actions::ContainerAction::BULK
                .map(|action| bulk_action(action, tokens, none_selected, cx)),
        )
        .child(
            button("bulk-Cancel", "Cancel", tokens)
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.toggle_bulk(cx))),
        )
}

/// Something to do to every selected container. With nothing selected there is
/// nothing to do, and the button says so instead of doing nothing at all.
fn bulk_action(
    action: crate::actions::ContainerAction,
    tokens: Tokens,
    none_selected: bool,
    cx: &mut Context<BerthApp>,
) -> Button {
    let button = button(
        SharedString::from(format!("bulk-{}", action.label())),
        action.label(),
        tokens,
    )
    .small();
    // Deleting destroys; starting and stopping don't.
    let button = if action.destroys() {
        button.danger_wash()
    } else {
        button
    };
    button
        .description("Select at least one container")
        .disabled(none_selected)
        .on_click(cx.listener(move |this, _, _, cx| this.bulk_request(action, cx)))
}
