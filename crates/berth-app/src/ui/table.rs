//! Page headers and the container table, including its cell vocabulary.

use berth_core::format;
use berth_ui::layout::{ContainerColumns, Widths};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, IntoElement, Role, div, px};

use crate::app::{BerthApp, Filter, SearchKind, Tab};
use crate::ui::table_cells as cells;
pub(crate) use crate::ui::table_cells::{
    empty_state, header_cell, pill, sortable_header_cell, text_cell,
};

pub(crate) fn page_header(
    app: &BerthApp,
    widths: Widths,
    search_kind: Option<SearchKind>,
    action_label: Option<&'static str>,
    cx: &mut Context<BerthApp>,
) -> AnyElement {
    let tokens = app.tokens;
    let running = app.snapshot.running();
    let total = app.snapshot.containers.len();
    let subtitle = match app.tab {
        Tab::Containers => format!("{running} running · {total} total"),
        Tab::Overview => app.engine.engine_label(),
        Tab::Disk => app
            .snapshot
            .df
            .as_ref()
            .map(|_| "as of the last refresh".to_owned())
            .unwrap_or_else(|| "no disk figures yet".to_owned()),
        Tab::Images => format::count(app.snapshot.images.len(), "image", "images"),
        Tab::Volumes => format::count(app.snapshot.volumes.len(), "volume", "volumes"),
        Tab::Networks => format::count(app.snapshot.networks.len(), "network", "networks"),
        Tab::Events => format::count(app.snapshot.events.len(), "event", "events"),
        Tab::Stacks => format::count(app.snapshot.projects.len(), "project", "projects"),
        Tab::Runtimes => format::count(app.engine.reachable(), "engine", "engines"),
        Tab::Settings => format!(
            "berth {} · a native container monitor",
            env!("CARGO_PKG_VERSION")
        ),
    };

    let mut header = div()
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .gap(px(10.0))
        .pb(px(10.0))
        .child(
            div()
                .text_size(px(type_scale::TITLE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.text)
                .child(app.tab.label()),
        )
        .child(
            div()
                .text_size(px(type_scale::SUBTITLE))
                .text_color(tokens.muted)
                .child(subtitle),
        )
        .child(div().flex_1());

    if let Some(kind) = search_kind {
        let search_width = if widths.content < 760.0 { 132.0 } else { 190.0 };
        if let Some(label) = action_label {
            header = header.child(
                div()
                    .id("page-action")
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .px(px(10.0))
                    .rounded(px(13.0))
                    .bg(tokens.surface_hover)
                    .role(Role::Button)
                    .aria_label(label)
                    .on_click(cx.listener(|this, _, _, cx| match this.tab {
                        Tab::Containers => this.toggle_bulk(cx),
                        Tab::Volumes => this.open_create(crate::app::CreateKind::Volume, cx),
                        Tab::Networks => this.open_create(crate::app::CreateKind::Network, cx),
                        _ => {}
                    }))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(tokens.text)
                            .child(label),
                    ),
            );
        }
        let search = app.search(kind).clone();
        search.update(cx, |input, _| input.set_width(search_width));
        header = header.child(search);
    }

    let chips = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .pb(px(10.0))
        .children(Filter::ALL.into_iter().map(|filter| {
            let selected = app.filter == filter;
            div()
                .id(("filter", filter as usize))
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(12.0))
                .rounded(px(13.0))
                .bg(if selected {
                    tokens.surface_hover
                } else {
                    tokens.surface
                })
                .border_1()
                .border_color(tokens.border)
                .role(Role::Button)
                .aria_label(filter.label())
                .aria_selected(selected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.filter != filter {
                        this.filter = filter;
                        this.selected = None;
                        this.selected_id = None;
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(if selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(tokens.text)
                        .child(filter.label()),
                )
        }));

    if app.tab == Tab::Containers {
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header)
            .child(chips)
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header)
            .into_any_element()
    }
}

pub(crate) fn table_header(
    app: &BerthApp,
    columns: ContainerColumns,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement + use<> {
    let tokens = app.tokens;
    let sort = app.sort_for(0);
    let mut row = div()
        .id("table-header")
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .px(px(12.0))
        .pb(px(6.0))
        .gap(px(10.0))
        .role(Role::Row)
        .aria_label("Containers table header");

    if app.bulk_enabled {
        row = row.child(header_cell("", 28.0, false, tokens));
        row = row.child(sortable_header_cell(
            0,
            0,
            "NAME",
            columns.name - 28.0,
            false,
            sort,
            tokens,
            cx,
        ));
    } else {
        row = row.child(sortable_header_cell(
            0,
            0,
            "NAME",
            columns.name,
            false,
            sort,
            tokens,
            cx,
        ));
    }
    row = row.child(sortable_header_cell(
        0,
        1,
        "STATUS",
        columns.status,
        false,
        sort,
        tokens,
        cx,
    ));
    row = row.child(sortable_header_cell(
        0,
        2,
        "CPU",
        columns.cpu,
        true,
        sort,
        tokens,
        cx,
    ));
    if columns.show_mem {
        row = row.child(sortable_header_cell(
            0,
            3,
            "MEM",
            columns.mem,
            true,
            sort,
            tokens,
            cx,
        ));
    }
    if columns.show_net {
        row = row.child(sortable_header_cell(
            0,
            4,
            "NET",
            columns.net,
            false,
            sort,
            tokens,
            cx,
        ));
    }
    if columns.show_disk {
        row = row.child(sortable_header_cell(
            0,
            5,
            "DISK",
            columns.disk,
            false,
            sort,
            tokens,
            cx,
        ));
    }
    if columns.show_pids {
        row = row.child(sortable_header_cell(
            0,
            6,
            "PIDS",
            columns.pids,
            true,
            sort,
            tokens,
            cx,
        ));
    }
    if columns.show_uptime {
        row = row.child(sortable_header_cell(
            0,
            7,
            "UPTIME",
            columns.uptime,
            false,
            sort,
            tokens,
            cx,
        ));
    }
    if columns.show_ports {
        row = row.child(sortable_header_cell(
            0,
            8,
            "PORTS",
            columns.ports,
            false,
            sort,
            tokens,
            cx,
        ));
    }
    row = row.child(header_cell("ACTIONS", columns.actions, true, tokens));

    div()
        .flex()
        .flex_col()
        .w_full()
        .child(row)
        .child(div().w_full().h(px(1.0)).bg(tokens.border))
}

pub(crate) fn table(
    app: &BerthApp,
    columns: ContainerColumns,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let tokens = app.tokens;
    let rows = app.visible_rows();

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
        return empty_state(heading, &detail, tokens).into_any_element();
    }

    if rows.is_empty() {
        let message = if !app.query(crate::app::SearchKind::Containers).is_empty() {
            format!(
                "No results for “{}”",
                app.query(crate::app::SearchKind::Containers)
            )
        } else {
            match app.filter {
                Filter::Stopped => "No stopped containers — All shows every container".to_owned(),
                Filter::Running => "No running containers — All shows every container".to_owned(),
                Filter::All => "No containers".to_owned(),
            }
        };
        return empty_state(
            &message,
            "Clear the filter or search to see every container.",
            tokens,
        )
        .into_any_element();
    }

    let bar = bulk_bar(app, tokens, cx);
    let children = rows
        .iter()
        .map(|index| {
            let container = &app.snapshot.containers[*index];
            let stat = app.snapshot.stat(*index);
            let selected = app.selected_id.as_deref() == Some(container.id.as_str());
            cells::container_row(app, container, stat, selected, tokens, columns, cx)
        })
        .collect::<Vec<_>>();

    div()
        .id("containers-table")
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .role(Role::Table)
        .aria_label("Containers")
        .child(bar)
        .children(children)
        .into_any_element()
}

fn bulk_bar(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Div {
    if !app.bulk_enabled {
        return div();
    }
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(8.0))
        .py(px(6.0))
        .child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.muted)
                .child(format!("{} selected", app.bulk_selected.len())),
        )
        .child(small(
            "All",
            tokens,
            cx.listener(|this, _, _, cx| this.bulk_select_all(cx)),
        ))
        .child(small(
            "None",
            tokens,
            cx.listener(|this, _, _, cx| this.bulk_clear(cx)),
        ))
        .child(div().flex_1())
        .child(small(
            "Start",
            tokens,
            cx.listener(|this, _, _, cx| this.bulk_request("Start", cx)),
        ))
        .child(small(
            "Restart",
            tokens,
            cx.listener(|this, _, _, cx| this.bulk_request("Restart", cx)),
        ))
        .child(small(
            "Stop",
            tokens,
            cx.listener(|this, _, _, cx| this.bulk_request("Stop", cx)),
        ))
        .child(small(
            "Delete",
            tokens,
            cx.listener(|this, _, _, cx| this.bulk_request("Delete", cx)),
        ))
        .child(small(
            "Cancel",
            tokens,
            cx.listener(|this, _, _, cx| this.toggle_bulk(cx)),
        ))
}

fn small(
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(format!("bulk-{label}")))
        .flex()
        .items_center()
        .h(px(24.0))
        .px(px(8.0))
        .rounded(px(12.0))
        .bg(tokens.surface_hover)
        .role(Role::Button)
        .aria_label(label)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.text)
                .child(label),
        )
}
