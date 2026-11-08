//! Shared inspector chrome: the panel shell, small buttons, facts and IDs.

use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{Context, Div, FontWeight, IntoElement, Role, Stateful, div, px};

use super::tabular;
use crate::app::BerthApp;

pub(crate) fn pane_tab(
    label: &'static str,
    active: bool,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(format!("inspector-tab-{label}")))
        .flex()
        .items_center()
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(12.0))
        .bg(if active {
            tokens.accent.alpha(0.16)
        } else {
            tokens.surface_hover
        })
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(active)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(if active {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if active {
                    tokens.accent_text_pill()
                } else {
                    tokens.muted
                })
                .child(label),
        )
}

pub(crate) fn small_action(
    id: &'static str,
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(22.0))
        .px(px(8.0))
        .rounded(px(11.0))
        .bg(tokens.surface_hover)
        .role(Role::Button)
        .aria_label(label)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(10.5))
                .text_color(tokens.muted)
                .child(label),
        )
}

pub(crate) fn toggle(
    label: &'static str,
    on: bool,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(format!(
            "inspector-toggle-{label}"
        )))
        .flex()
        .items_center()
        .h(px(22.0))
        .px(px(8.0))
        .rounded(px(11.0))
        .bg(if on {
            tokens.accent.alpha(0.16)
        } else {
            tokens.surface_hover
        })
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(on)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(10.5))
                .text_color(if on {
                    tokens.accent_text_pill()
                } else {
                    tokens.muted
                })
                .child(label),
        )
}

pub(crate) fn fact(label: &'static str, value: &str, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(10.0))
        .child(
            div()
                .w(px(64.0))
                .text_size(px(type_scale::FACT_LABEL))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(type_scale::FACT_VALUE))
                .font_features(tabular())
                .text_color(tokens.text)
                .truncate()
                .child(value.to_owned()),
        )
}

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
        .p(px(14.0))
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
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .truncate()
                        .child(name.to_owned()),
                )
                .child(
                    div()
                        .id("inspector-close")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(24.0))
                        .h(px(24.0))
                        .rounded(px(12.0))
                        .bg(tokens.surface_hover)
                        .role(Role::Button)
                        .aria_label("Close inspector")
                        .on_click(cx.listener(|this, _, _, cx| this.clear_selection(cx)))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(tokens.muted)
                                .child("×"),
                        ),
                ),
        )
        .child(div().w_full().h(px(1.0)).bg(tokens.border))
}

pub(crate) fn verb_button(
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(format!("inspector-verb-{label}")))
        .flex()
        .items_center()
        .h(px(26.0))
        .px(px(10.0))
        .rounded(px(13.0))
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

pub(crate) fn delete_button(
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(format!(
            "inspector-delete-{label}"
        )))
        .flex()
        .items_center()
        .h(px(26.0))
        .px(px(10.0))
        .rounded(px(13.0))
        .bg(tokens.danger_text().alpha(0.12))
        .role(Role::Button)
        .aria_label(label)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.danger_text())
                .child(label),
        )
}

pub(crate) fn short_id(id: &str) -> String {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    id.chars().take(12).collect()
}
