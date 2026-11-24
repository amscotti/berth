//! Volumes, with the size/in-use figures `/system/df` fills in.

use berth_core::format;
use berth_ui::layout::{self, Widths};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px};

use crate::app::{BerthApp, DetailKind, SearchKind};
use crate::ui::data_table::{Column, DataTable, RowKey, text_cell};
use crate::ui::page::{empty_state, page_header};

pub fn volumes(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Volumes).to_ascii_lowercase();
    let (sort_col, sort_asc) = app.sort_for(2);
    // Worked out once per snapshot, query and sort, and kept from one frame
    // to the next while those hold.
    let shown = app.rows_of(SearchKind::Volumes, || {
        let mut shown: Vec<usize> = app
            .snapshot
            .volumes
            .iter()
            .enumerate()
            .filter(|(_, volume)| {
                query.is_empty()
                    || volume.name.to_ascii_lowercase().contains(&query)
                    || volume.driver.to_ascii_lowercase().contains(&query)
                    || volume.mountpoint.to_ascii_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        shown.sort_by(|&a, &b| {
            let (a, b) = (&app.snapshot.volumes[a], &app.snapshot.volumes[b]);
            let ua = a.usage_data.as_ref();
            let ub = b.usage_data.as_ref();
            let ord = match sort_col {
                1 => a.driver.cmp(&b.driver),
                2 => ua.map_or(-1, |u| u.size).cmp(&ub.map_or(-1, |u| u.size)),
                3 => ua
                    .map_or(-1, |u| u.ref_count)
                    .cmp(&ub.map_or(-1, |u| u.ref_count)),
                4 => a.mountpoint.cmp(&b.mountpoint),
                _ => a.name.cmp(&b.name),
            };
            if sort_asc { ord } else { ord.reverse() }
        });
        shown
    });

    let header = page_header(
        app,
        widths,
        Some(SearchKind::Volumes),
        Some("New volume"),
        cx,
    );
    let mut content = div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .child(header);

    if app.snapshot.volumes.is_empty() {
        content = content.child(empty_state(
            "No volumes",
            "Create one above, or run a container that declares one.",
            tokens,
        ));
        return content.into_any_element();
    }
    if shown.is_empty() {
        content = content.child(empty_state(
            &format!("No results for “{}”", app.query(SearchKind::Volumes)),
            "Clear the search to see every volume.",
            tokens,
        ));
        return content.into_any_element();
    }

    let full = widths.content >= 824.0;
    let (name, driver, size, in_use) = if full {
        (220.0, 80.0, 90.0, 80.0)
    } else {
        (160.0, 70.0, 0.0, 0.0)
    };
    let mount = (widths.content - name - driver - size - in_use - 84.0).max(166.0);
    let mut table = DataTable::new(
        "volumes-table",
        "Volumes",
        "volumes-header",
        2,
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
    ));
    if size > 0.0 {
        table = table
            .column(Column::sorted("size", "SIZE", size, true, 2, size_cell))
            .column(Column::sorted(
                "in-use",
                "IN USE",
                in_use,
                true,
                3,
                in_use_cell,
            ));
    }
    let table = table
        .column(Column::sorted(
            "mountpoint",
            "MOUNTPOINT",
            mount,
            false,
            4,
            mountpoint_cell,
        ))
        .hairline();
    let header = table.header(app, tokens, cx);
    let body = table.body(
        shown,
        layout::ROW_CONTAINER,
        &app.view.lists[SearchKind::Volumes as usize].scroll,
        tokens,
        cx,
    );
    content.child(header).child(body).into_any_element()
}

fn row_key(app: &BerthApp, ix: usize) -> RowKey {
    let volume = &app.snapshot.volumes[ix];
    RowKey {
        id: SharedString::from(format!("volume-{}", volume.name)),
        name: volume.name.clone(),
        selected: app.view.detail_kind == DetailKind::Volume
            && app.view.selected_id.as_deref() == Some(volume.name.as_str()),
        opens: true,
    }
}

fn open_row(app: &mut BerthApp, ix: usize, cx: &mut Context<BerthApp>) {
    let name = app.snapshot.volumes[ix].name.clone();
    app.select_volume(&name, cx);
}

fn name_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let volume = &app.snapshot.volumes[ix];
    text_cell(volume.name.clone(), width, false, app.tokens).into_any_element()
}

fn driver_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let volume = &app.snapshot.volumes[ix];
    text_cell(volume.driver.clone(), width, false, app.tokens).into_any_element()
}

/// The size, where the engine's disk figures gave one: unknown is a dash.
fn size_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let volume = &app.snapshot.volumes[ix];
    let text = volume
        .usage_data
        .as_ref()
        .filter(|usage| usage.size >= 0)
        .map(|usage| format::bytes(usage.size as u64))
        .unwrap_or_else(|| "—".to_owned());
    text_cell(text, width, true, app.tokens).into_any_element()
}

fn in_use_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let volume = &app.snapshot.volumes[ix];
    let text = volume
        .usage_data
        .as_ref()
        .map(|usage| usage.ref_count.to_string())
        .unwrap_or_else(|| "—".to_owned());
    text_cell(text, width, true, app.tokens).into_any_element()
}

fn mountpoint_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let volume = &app.snapshot.volumes[ix];
    text_cell(volume.mountpoint.clone(), width, false, app.tokens).into_any_element()
}
