//! Volumes, with the size/in-use figures `/system/df` fills in.

use berth_core::format;
use berth_engine::dto::Volume;
use berth_ui::layout::{self, Widths};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Role, SharedString, div, px};

use crate::app::{BerthApp, DetailKind, SearchKind};
use crate::ui::table::{empty_state, page_header, sortable_header_cell, text_cell};

pub fn volumes(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Volumes).to_ascii_lowercase();
    let mut shown: Vec<&Volume> = app
        .snapshot
        .volumes
        .iter()
        .filter(|volume| {
            query.is_empty()
                || volume.name.to_ascii_lowercase().contains(&query)
                || volume.driver.to_ascii_lowercase().contains(&query)
                || volume.mountpoint.to_ascii_lowercase().contains(&query)
        })
        .collect();
    let (sort_col, sort_asc) = app.sort_for(2);
    shown.sort_by(|a, b| {
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
    let sort = (sort_col, sort_asc);
    let mut header_row = div()
        .id("volumes-header")
        .role(Role::Row)
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .px(px(12.0))
        .pb(px(6.0))
        .gap(px(10.0))
        .child(sortable_header_cell(
            2, 0, "NAME", name, false, sort, tokens, cx,
        ))
        .child(sortable_header_cell(
            2, 1, "DRIVER", driver, false, sort, tokens, cx,
        ));
    if size > 0.0 {
        header_row = header_row
            .child(sortable_header_cell(
                2, 2, "SIZE", size, true, sort, tokens, cx,
            ))
            .child(sortable_header_cell(
                2, 3, "IN USE", in_use, true, sort, tokens, cx,
            ));
    }
    header_row = header_row.child(sortable_header_cell(
        2,
        4,
        "MOUNTPOINT",
        mount,
        false,
        sort,
        tokens,
        cx,
    ));
    content = content.child(
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header_row)
            .child(div().w_full().h(px(1.0)).bg(tokens.border)),
    );

    let rows = shown.into_iter().enumerate().map(|(index, volume)| {
        let selected = app.detail_kind == DetailKind::Volume
            && app.selected_id.as_deref() == Some(volume.name.as_str());
        let click_name = volume.name.clone();
        let mut row = div()
            .id(SharedString::from(format!("volume-{}", volume.name)))
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
            } else if index % 2 == 1 {
                tokens.surface_hover.alpha(0.35)
            } else {
                tokens.surface
            })
            .when(!selected, |this| {
                this.hover(move |style| style.bg(tokens.surface_hover))
            })
            .role(Role::Row)
            .aria_label(&volume.name)
            .aria_selected(selected)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_volume(&click_name, cx);
            }));
        row = row
            .child(text_cell(volume.name.clone(), name, false, tokens))
            .child(text_cell(volume.driver.clone(), driver, false, tokens));
        if size > 0.0 {
            let usage = volume.usage_data.as_ref();
            row = row
                .child(text_cell(
                    usage
                        .filter(|usage| usage.size >= 0)
                        .map(|usage| format::bytes(usage.size as u64))
                        .unwrap_or_else(|| "—".to_owned()),
                    size,
                    true,
                    tokens,
                ))
                .child(text_cell(
                    usage
                        .map(|usage| usage.ref_count.to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                    in_use,
                    true,
                    tokens,
                ));
        }
        row.child(text_cell(volume.mountpoint.clone(), mount, false, tokens))
    });
    content
        .child(
            div()
                .id("volumes-table")
                .flex()
                .flex_col()
                .w_full()
                .role(Role::Table)
                .aria_label("Volumes")
                .children(rows),
        )
        .into_any_element()
}
