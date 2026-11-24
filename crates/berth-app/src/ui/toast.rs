//! Transient notices. They belong to the top layer, not the navigation rail:
//! status text in the rail is a monitor reading, not a message channel.

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, div, px};

use crate::app::BerthApp;

/// Horizontal distance from the right edge of the window so a toast sits at
/// the foot of the main column and never covers an open inspector.
pub(crate) fn right_offset(pane_open: bool, window_width: Option<f32>) -> f32 {
    if !pane_open {
        return 20.0;
    }
    let width = window_width.unwrap_or(layout::WINDOW_DEFAULT.0);
    20.0 + layout::for_window(width, true).pane
}

pub fn overlay(
    app: &BerthApp,
    tokens: Tokens,
    cx: &mut gpui::Context<BerthApp>,
) -> Option<AnyElement> {
    let toast = app.view.toast.as_ref()?;
    let color = if toast.bad {
        tokens.danger_text()
    } else {
        tokens.success_text()
    };
    let right = right_offset(
        app.view.selected_id.is_some(),
        app.window.map(|window| window.width as f32),
    );
    Some(
        div()
            .id("toast")
            .absolute()
            .right(px(right))
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
            // Good news is announced when the reader is free; only bad news
            // interrupts.
            .role(if toast.bad {
                gpui::Role::Alert
            } else {
                gpui::Role::Status
            })
            .aria_label(toast.text.clone())
            .on_click(cx.listener(|this, _, _, cx| {
                this.view.toast = None;
                this.view.toast_task = None;
                cx.notify();
            }))
            .child(div().w(px(7.0)).h(px(7.0)).rounded(px(4.0)).bg(color))
            .child(
                div()
                    .text_size(px(type_scale::SECONDARY))
                    .text_color(tokens.text)
                    .child(toast.text.clone()),
            )
            .child(div().flex_1())
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_clears_the_inspector_when_open() {
        assert_eq!(right_offset(false, Some(1280.0)), 20.0);
        assert_eq!(right_offset(true, Some(1280.0)), 20.0 + layout::PANE_WIDE);
        assert_eq!(right_offset(true, Some(960.0)), 20.0 + layout::PANE_NARROW);
        assert_eq!(right_offset(true, None), 20.0 + layout::PANE_WIDE);
    }
}
