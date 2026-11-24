//! Networks and how many containers are attached to each.

use berth_engine::dto::Network;
use berth_ui::layout::{self, Widths};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px};

use crate::app::{BerthApp, DetailKind, SearchKind};
use crate::ui::data_table::{Column, DataTable, RowKey, text_cell};
use crate::ui::page::{empty_state, page_header};

pub fn networks(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Networks).to_ascii_lowercase();
    let (sort_col, sort_asc) = app.sort_for(3);
    // Worked out once per snapshot, query and sort, and kept from one frame
    // to the next while those hold.
    let shown = app.rows_of(SearchKind::Networks, || {
        let mut shown: Vec<usize> = app
            .snapshot
            .networks
            .iter()
            .enumerate()
            .filter(|(_, network)| {
                query.is_empty()
                    || network.name.to_ascii_lowercase().contains(&query)
                    || network.driver.to_ascii_lowercase().contains(&query)
                    || network.scope.to_ascii_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        shown.sort_by(|&a, &b| {
            let (a, b) = (&app.snapshot.networks[a], &app.snapshot.networks[b]);
            let ord = match sort_col {
                1 => a.driver.cmp(&b.driver),
                2 => a.scope.cmp(&b.scope),
                3 => app
                    .snapshot
                    .network_counts
                    .get(&a.name)
                    .cmp(&app.snapshot.network_counts.get(&b.name)),
                _ => a.name.cmp(&b.name),
            };
            if sort_asc { ord } else { ord.reverse() }
        });
        shown
    });

    let header = page_header(
        app,
        widths,
        Some(SearchKind::Networks),
        Some("New network"),
        cx,
    );
    let mut content = div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .child(header);

    if app.snapshot.networks.is_empty() {
        content = content.child(empty_state(
            "No networks",
            "Create one above, or run a container.",
            tokens,
        ));
        return content.into_any_element();
    }
    if shown.is_empty() {
        content = content.child(empty_state(
            &format!("No results for “{}”", app.query(SearchKind::Networks)),
            "Clear the search to see every network.",
            tokens,
        ));
        return content.into_any_element();
    }

    let full = widths.content >= 524.0;
    let (driver, scope, attached) = if full {
        (100.0, 90.0, 90.0)
    } else {
        (70.0, 60.0, 60.0)
    };
    let name = (widths.content - driver - scope - attached - 84.0).clamp(180.0, 360.0);
    let table = DataTable::new(
        "networks-table",
        "Networks",
        "networks-header",
        3,
        row_key,
        open_row,
    )
    .column(Column::sorted("name", "NAME", name, false, 0, name_cell))
    .column(Column::sorted(
        "driver",
        "DRIVER",
        driver,
        false,
        1,
        driver_cell,
    ))
    .column(Column::sorted(
        "scope", "SCOPE", scope, false, 2, scope_cell,
    ))
    .column(Column::sorted(
        "attached",
        "ATTACHED",
        attached,
        true,
        3,
        attached_cell,
    ))
    .hairline();
    let header = table.header(app, tokens, cx);
    let body = table.body(
        shown,
        layout::ROW_CONTAINER,
        &app.view.lists[SearchKind::Networks as usize].scroll,
        tokens,
        cx,
    );
    content.child(header).child(body).into_any_element()
}

/// What the inspector knows a network by: its id, or its name where the
/// engine gave none.
fn known_as(network: &Network) -> String {
    if network.id.is_empty() {
        network.name.clone()
    } else {
        network.id.clone()
    }
}

fn row_key(app: &BerthApp, ix: usize) -> RowKey {
    let network = &app.snapshot.networks[ix];
    let known_as = known_as(network);
    RowKey {
        id: SharedString::from(format!("network-{known_as}")),
        name: network.name.clone(),
        selected: app.view.detail_kind == DetailKind::Network
            && app.view.selected_id.as_deref() == Some(known_as.as_str()),
        opens: true,
    }
}

fn open_row(app: &mut BerthApp, ix: usize, cx: &mut Context<BerthApp>) {
    let network = &app.snapshot.networks[ix];
    let (known_as, name) = (known_as(network), network.name.clone());
    app.select_network(&known_as, &name, cx);
}

fn name_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let network = &app.snapshot.networks[ix];
    text_cell(network.name.clone(), width, false, app.tokens).into_any_element()
}

fn driver_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let network = &app.snapshot.networks[ix];
    let text = if network.driver.is_empty() {
        "—".to_owned()
    } else {
        network.driver.clone()
    };
    text_cell(text, width, false, app.tokens).into_any_element()
}

fn scope_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let network = &app.snapshot.networks[ix];
    text_cell(network.scope.clone(), width, false, app.tokens).into_any_element()
}

fn attached_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let network = &app.snapshot.networks[ix];
    let attached = app.snapshot.network_counts.get(&network.name);
    text_cell(attached.to_string(), width, true, app.tokens).into_any_element()
}
