//! Networks and how many containers are attached to each.

use berth_engine::dto::Network;
use berth_ui::layout::{self, Widths};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Role, SharedString, div, px};

use crate::app::{BerthApp, DetailKind, SearchKind};
use crate::ui::table::{empty_state, page_header, sortable_header_cell, text_cell};

pub fn networks(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Networks).to_ascii_lowercase();
    let mut shown: Vec<&Network> = app
        .snapshot
        .networks
        .iter()
        .filter(|network| {
            query.is_empty()
                || network.name.to_ascii_lowercase().contains(&query)
                || network.driver.to_ascii_lowercase().contains(&query)
                || network.scope.to_ascii_lowercase().contains(&query)
        })
        .collect();
    let (sort_col, sort_asc) = app.sort_for(3);
    shown.sort_by(|a, b| {
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
    let sort = (sort_col, sort_asc);
    content = content.child(
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(
                div()
                    .id("networks-header")
                    .role(Role::Row)
                    .flex()
                    .flex_row()
                    .w_full()
                    .items_center()
                    .px(px(12.0))
                    .pb(px(6.0))
                    .gap(px(10.0))
                    .child(sortable_header_cell(
                        3, 0, "NAME", name, false, sort, tokens, cx,
                    ))
                    .child(sortable_header_cell(
                        3, 1, "DRIVER", driver, false, sort, tokens, cx,
                    ))
                    .child(sortable_header_cell(
                        3, 2, "SCOPE", scope, false, sort, tokens, cx,
                    ))
                    .child(sortable_header_cell(
                        3, 3, "ATTACHED", attached, true, sort, tokens, cx,
                    )),
            )
            .child(div().w_full().h(px(1.0)).bg(tokens.border)),
    );

    let rows = shown.into_iter().enumerate().map(|(index, network)| {
        let attached_count = app.snapshot.network_counts.get(&network.name);
        let click_id = if network.id.is_empty() {
            network.name.clone()
        } else {
            network.id.clone()
        };
        let selected = app.detail_kind == DetailKind::Network
            && app.selected_id.as_deref() == Some(click_id.as_str());
        let click_name = network.name.clone();
        let mut row = div()
            .id(SharedString::from(format!("network-{click_id}")))
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
            .aria_label(&network.name)
            .aria_selected(selected)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_network(&click_id, &click_name, cx);
            }));
        row = row
            .child(text_cell(network.name.clone(), name, false, tokens))
            .child(text_cell(
                if network.driver.is_empty() {
                    "—".to_owned()
                } else {
                    network.driver.clone()
                },
                driver,
                false,
                tokens,
            ))
            .child(text_cell(network.scope.clone(), scope, false, tokens))
            .child(text_cell(
                attached_count.to_string(),
                attached,
                true,
                tokens,
            ));
        row
    });
    content
        .child(
            div()
                .id("networks-table")
                .flex()
                .flex_col()
                .w_full()
                .role(Role::Table)
                .aria_label("Networks")
                .children(rows),
        )
        .into_any_element()
}
