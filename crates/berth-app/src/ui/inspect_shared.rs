//! Shared inspector chrome: the panel shell, its tabs and toggles, facts and
//! IDs.
//!
//! The inspector sits on `surface`, not on the page, so its chips are washed
//! over that: a chip washed over the page would read as a patch of something
//! else on the panel.

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Context, Div, FontWeight, IntoElement, Role, SharedString, Stateful, Window,
    div, px,
};

use super::kit::{Button, button, chip};
use super::tabular;
use crate::app::BerthApp;

/// A page of the inspector: Facts, Config, Runtime, Inspect. One is selected.
pub(crate) fn inspector_tab(
    label: &'static str,
    active: bool,
    tokens: Tokens,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    chip(
        SharedString::from(format!("inspector-tab-{label}")),
        label,
        active,
        tokens,
    )
    .tab()
    .on_surface(tokens.surface)
    .on_click(on_click)
}

/// A switch in the logs header: Follow, Times. On or off on its own.
pub(crate) fn inspector_toggle(
    label: &'static str,
    on: bool,
    tokens: Tokens,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    chip(
        SharedString::from(format!("inspector-toggle-{label}")),
        label,
        on,
        tokens,
    )
    .toggle()
    .on_surface(tokens.surface)
    .on_click(on_click)
}

/// A quiet button of the inspector: Refresh, Copy. Here when wanted, and
/// nothing in the way otherwise.
pub(crate) fn inspector_quiet(
    id: &'static str,
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    button(id, label, tokens).small().ghost().on_click(on_click)
}

/// A button of an inspector's action row.
pub(crate) fn inspector_button(
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    button(
        SharedString::from(format!("inspector-verb-{label}")),
        label,
        tokens,
    )
    .on_click(on_click)
}

/// The one destructive action of an inspector: Delete, Remove. It destroys,
/// so it is drawn in the danger wash.
pub(crate) fn inspector_delete(
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    inspector_button(label, tokens, on_click).danger_wash()
}

pub(crate) fn fact(label: &'static str, value: &str, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(layout::ROW_GAP))
        .child(
            div()
                .w(px(layout::FACT_LABEL))
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(type_scale::BODY))
                .font_features(tabular())
                .text_color(tokens.text)
                .truncate()
                .child(value.to_owned()),
        )
}

/// A fact whose value is an identifier or path, set in the monospace family.
pub(crate) fn fact_mono(label: &'static str, value: &str, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(layout::ROW_GAP))
        .child(
            div()
                .w(px(layout::FACT_LABEL))
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(type_scale::MONO))
                .font_family(type_scale::MONO_FAMILY)
                .font_features(tabular())
                .text_color(tokens.text)
                .truncate()
                .child(value.to_owned()),
        )
}

/// The panel: its title, a close, a rule, and then whatever page is showing.
pub(crate) fn pane_shell(
    name: &str,
    label: &'static str,
    width: f32,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Stateful<Div> {
    div()
        .id("inspector")
        .flex()
        .flex_col()
        .w(px(width))
        .flex_shrink_0()
        .h_full()
        .p(px(layout::INSPECTOR_PADDING))
        .gap(px(8.0))
        .bg(tokens.surface)
        .border_l_1()
        .border_color(tokens.border)
        .role(Role::Complementary)
        .aria_label(label)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(type_scale::TITLE_SMALL))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .truncate()
                        .child(name.to_owned()),
                )
                .child(
                    button("inspector-close", "Close inspector", tokens)
                        .small()
                        .icon("xmark")
                        .icon_only()
                        .on_click(cx.listener(|this, _, _, cx| this.clear_selection(cx))),
                ),
        )
        .child(div().w_full().h(px(1.0)).bg(tokens.border))
}

pub(crate) fn short_id(id: &str) -> String {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    id.chars().take(12).collect()
}
