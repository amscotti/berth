//! The navigation rail: brand, pages, live figures and the status line.

use berth_core::format;
use berth_ui::layout::Widths;
use berth_ui::theme::{self, Tokens};
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{Context, FontWeight, IntoElement, Orientation, Role, SharedString, div, px};

use super::kit::nav_item;
use crate::app::{BerthApp, Tab};

pub(crate) fn rail(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> impl IntoElement {
    let tokens = app.tokens;
    let narrow = widths.narrow_rail;
    let reachable = app.engine.reachable();

    let mut rail = div()
        .id("rail")
        .flex()
        .flex_col()
        .w(px(widths.rail))
        .flex_shrink_0()
        .h_full()
        .py(px(14.0))
        .px(px(if narrow { 8.0 } else { 10.0 }))
        .gap(px(if narrow { 3.0 } else { 2.0 }))
        .bg(tokens.surface)
        .border_r_1()
        .border_color(tokens.border)
        .role(Role::Navigation)
        .aria_label("berth navigation");

    // Brand.
    let mut brand = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .w_full()
        .gap(px(8.0))
        .pb(px(if narrow { 12.0 } else { 14.0 }));
    brand = brand.child(
        div()
            .w(px(18.0))
            .h(px(18.0))
            .rounded(px(4.0))
            .bg(tokens.accent)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(px(2.0))
                    .bg(tokens.accent_text),
            ),
    );
    if !narrow {
        brand = brand.child(
            div()
                .text_size(px(type_scale::TITLE_SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.text)
                .child("berth"),
        );
    }
    rail = rail.child(brand);

    // The pages are a tab strip: each item is a tab, and one is selected.
    let mut pages = div()
        .id("rail-pages")
        .role(Role::TabList)
        .aria_orientation(Orientation::Vertical)
        .aria_label("Pages")
        .flex()
        .flex_col()
        .gap(px(if narrow { 3.0 } else { 2.0 }));
    for tab in Tab::ALL {
        if tab == Tab::Containers {
            pages = if narrow {
                pages.child(divider(tokens))
            } else {
                pages.child(section_label("Resources", tokens))
            };
        }
        if tab == Tab::Disk {
            pages = if narrow {
                pages.child(divider(tokens))
            } else {
                pages.child(section_label("Engine", tokens))
            };
        }
        pages = pages.child(rail_item(app, tab, reachable, narrow, cx));
    }
    rail = rail.child(pages);
    rail = rail.child(div().flex_1());
    rail.child(rail_status(app, narrow))
}

fn rail_item(
    app: &BerthApp,
    tab: Tab,
    reachable: usize,
    narrow: bool,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let tokens = app.tokens;
    let figure = tab.figure(&app.snapshot, reachable);
    let mut item = nav_item(
        ("rail", tab as usize),
        tab.label(),
        tab.icon_name(),
        theme::tint(tokens, tab.tint_kind()),
        app.view.tab == tab,
        tokens,
    );
    if narrow {
        item = item.compact();
    }
    if !figure.is_empty() {
        item = item.figure(SharedString::from(figure));
    }
    item.on_click(cx.listener(move |this, _, _, cx| this.switch_to(tab, cx)))
}

fn rail_status(app: &BerthApp, narrow: bool) -> impl IntoElement {
    let tokens = app.tokens;
    let error = !app.engine.error.is_empty();
    let loading = app.engine.loading || app.engine.last_update == 0;
    let label = if error {
        if narrow { "Warning" } else { "Engine warning" }
    } else if loading {
        if narrow {
            "Reading"
        } else {
            "Reading engine…"
        }
    } else {
        if narrow {
            "Refresh"
        } else {
            "Refresh · every 5 s"
        }
    };
    let engine_line = if error {
        app.engine.error.clone()
    } else {
        app.engine.engine_label()
    };
    let clock = if app.engine.last_update == 0 {
        "updated —".to_owned()
    } else {
        format!("updated {}", format::clock(app.engine.last_update))
    };

    let mut status = div().flex().flex_col().w_full().gap(px(3.0)).pb(px(6.0));
    if !narrow {
        status = status.px(px(8.0));
    }
    status = status.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(7.0))
            .child(div().w(px(7.0)).h(px(7.0)).rounded(px(4.0)).bg(if error {
                tokens.danger_text()
            } else if loading {
                tokens.warning_fill
            } else {
                tokens.success
            }))
            .child(
                div()
                    .text_size(px(if narrow {
                        type_scale::META
                    } else {
                        type_scale::LABEL
                    }))
                    .text_color(if error {
                        tokens.warning_text()
                    } else {
                        tokens.muted
                    })
                    .child(label),
            ),
    );
    status = status.child(
        div()
            .text_size(px(if narrow {
                type_scale::META
            } else {
                type_scale::LABEL
            }))
            .text_color(if error {
                tokens.warning_text()
            } else {
                tokens.muted
            })
            .truncate()
            .child(engine_line),
    );
    status.child(
        div()
            .text_size(px(if narrow {
                type_scale::META
            } else {
                type_scale::LABEL
            }))
            .font_features(super::tabular())
            .text_color(tokens.muted)
            .child(clock),
    )
}

fn section_label(name: &'static str, tokens: Tokens) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .w_full()
        .px(px(8.0))
        .pt(px(12.0))
        .pb(px(3.0))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.faint())
                .child(name),
        )
}

fn divider(tokens: Tokens) -> impl IntoElement {
    div()
        .w_full()
        .py(px(6.0))
        .child(div().w_full().h(px(1.0)).bg(tokens.border))
}
