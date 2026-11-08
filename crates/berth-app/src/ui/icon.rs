//! One SVG icon, tinted by the current text color.

use gpui::prelude::*;
use gpui::{Hsla, IntoElement, SharedString, px, svg};

pub fn icon(name: &'static str, size: f32, color: Hsla) -> impl IntoElement {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .text_color(color)
        .w(px(size))
        .h(px(size))
}
