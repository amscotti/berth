//! Transient notices. They belong to the top layer, not the navigation rail:
//! status text in the rail is a monitor reading, not a message channel.

use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{AnyElement, div, px};

use crate::app::BerthApp;

pub fn overlay(
    app: &BerthApp,
    tokens: Tokens,
    cx: &mut gpui::Context<BerthApp>,
) -> Option<AnyElement> {
    let toast = app.toast.as_ref()?;
    let color = if toast.bad {
        tokens.danger_text()
    } else {
        tokens.success_text()
    };
    Some(
        div()
            .id("toast")
            .absolute()
            .right(px(20.0))
            .bottom(px(18.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(9.0))
            .max_w(px(380.0))
            .px(px(12.0))
            .py(px(9.0))
            .rounded(px(9.0))
            .bg(tokens.surface)
            .border_1()
            .border_color(tokens.border)
            .shadow_lg()
            .role(gpui::Role::Alert)
            .aria_label(toast.text.clone())
            .on_click(cx.listener(|this, _, _, cx| {
                this.toast = None;
                this.toast_task = None;
                cx.notify();
            }))
            .child(div().w(px(7.0)).h(px(7.0)).rounded(px(4.0)).bg(color))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(tokens.text)
                    .child(toast.text.clone()),
            )
            .child(div().flex_1())
            .into_any_element(),
    )
}
