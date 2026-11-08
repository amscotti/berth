//! The navigation rail: brand, pages, live figures and the status line.

use berth_core::format;
use berth_ui::layout::Widths;
use berth_ui::theme::{self, Tokens};
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{Context, FontWeight, IntoElement, Role, div, px};

use super::tabular;
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
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.text)
                .child("berth"),
        );
    }
    rail = rail.child(brand);

    for tab in Tab::ALL {
        if tab == Tab::Containers {
            rail = if narrow {
                rail.child(divider(tokens))
            } else {
                rail.child(section_label("Resources", tokens))
            };
        }
        if tab == Tab::Disk {
            rail = if narrow {
                rail.child(divider(tokens))
            } else {
                rail.child(section_label("Engine", tokens))
            };
        }
        rail = rail.child(rail_item(app, tab, reachable, narrow, cx));
    }

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
    let selected = app.tab == tab;
    let tint = theme::tint(tokens, tab.tint_kind());
    let figure = tab.figure(&app.snapshot, reachable);

    let mut item = div()
        .id(("rail", tab as usize))
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .h(px(if narrow { 36.0 } else { 32.0 }))
        .gap(px(if narrow { 0.0 } else { 8.0 }))
        .rounded(px(7.0))
        .bg(if selected {
            tokens.accent
        } else {
            tokens.surface
        })
        .when(!selected, |this| {
            this.hover(move |style| style.bg(tokens.surface_hover))
        })
        .role(Role::Button)
        .aria_label(if figure.is_empty() {
            tab.label().to_owned()
        } else {
            format!("{} · {figure}", tab.label())
        })
        .aria_selected(selected)
        .on_click(cx.listener(move |this, _, _, cx| this.switch_to(tab, cx)));
    if narrow {
        item = item.justify_center();
    } else {
        item = item.px(px(6.0));
    }

    let chip_bg = if selected {
        tokens.accent_text.alpha(0.16)
    } else {
        tint.alpha(0.14)
    };
    let glyph = if selected { tokens.accent_text } else { tint };
    let chip_size = if narrow { 24.0 } else { 20.0 };
    item = item.child(
        div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(chip_size))
            .h(px(chip_size))
            .rounded(px(5.0))
            .bg(chip_bg)
            .child(crate::ui::icon::icon(
                tab.icon_name(),
                if narrow { 13.0 } else { 12.0 },
                glyph,
            )),
    );
    if narrow {
        return item;
    }

    item = item.child(
        div()
            .text_size(px(type_scale::BODY))
            .font_weight(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if selected {
                tokens.accent_text
            } else {
                tokens.text
            })
            .child(tab.label()),
    );
    item = item.child(div().flex_1());
    if !figure.is_empty() {
        item = item.child(
            div()
                .text_size(px(10.5))
                .font_features(tabular())
                .text_color(if selected {
                    tokens.accent_text
                } else {
                    tokens.muted
                })
                .child(figure),
        );
    }
    item
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
                    .text_size(px(if narrow { 9.5 } else { 11.0 }))
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
            .text_size(px(if narrow { 9.0 } else { 10.5 }))
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
            .text_size(px(if narrow { 9.0 } else { 10.0 }))
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
                .text_size(px(type_scale::SECTION))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.muted.alpha(0.75))
                .child(name),
        )
}

fn divider(tokens: Tokens) -> impl IntoElement {
    div()
        .w_full()
        .py(px(6.0))
        .child(div().w_full().h(px(1.0)).bg(tokens.border))
}
