//! Cell graphs: a grid of dots where each column is a slice of time and each
//! dot is one step of height. Drawn directly on a canvas so a few hundred dots
//! do not become a few hundred scene nodes.

use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{Bounds, Hsla, IntoElement, Point, canvas, div, fill, px, size};

const DOT: f32 = 4.0;
const PITCH: f32 = 6.0;
const INSET: f32 = 8.0;

/// A graph well: surface, hairline, fixed height, dots newest on the right.
///
/// `ceiling` is the value the top of the well stands for. It is fixed, never
/// fitted to the data, so a flat series stays at one height while its oldest
/// samples leave the ring. `None` means the ceiling isn't known, and nothing
/// is drawn: a graph against an unknown ceiling would lie.
pub fn cell_graph(
    values: Vec<f32>,
    color: Hsla,
    tokens: Tokens,
    ceiling: Option<f32>,
    height: f32,
) -> impl IntoElement {
    let span = value_span(ceiling);
    div()
        .w_full()
        .h(px(height))
        .rounded(px(8.0))
        .bg(tokens.well())
        .border_1()
        .border_color(tokens.well_border())
        .p(px(INSET))
        .overflow_hidden()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let inner_w = f32::from(bounds.size.width) - 2.0 * INSET;
                    let inner_h = f32::from(bounds.size.height) - 2.0 * INSET;
                    if inner_w <= 0.0 || inner_h <= 0.0 {
                        return;
                    }
                    let cols = (((inner_w + PITCH - DOT) / PITCH).floor() as usize).max(1);
                    let rows = (((inner_h + PITCH - DOT) / PITCH).floor() as usize).max(1);
                    // An unknown ceiling: the well stays empty.
                    let Some(value_span) = span else {
                        return;
                    };

                    let start = values.len().saturating_sub(cols);
                    for (offset, value) in values[start..].iter().enumerate() {
                        // Anything above zero shows at least one step.
                        let height =
                            ((*value / value_span).clamp(0.0, 1.0) * rows as f32).ceil() as usize;
                        for step in 0..height.min(rows) {
                            let x = f32::from(bounds.left()) + INSET + offset as f32 * PITCH;
                            let y =
                                f32::from(bounds.top()) + INSET + (rows - 1 - step) as f32 * PITCH;
                            window.paint_quad(fill(
                                Bounds::new(Point::new(px(x), px(y)), size(px(DOT), px(DOT))),
                                color,
                            ));
                        }
                    }
                },
            )
            .size_full(),
        )
}

/// The span a ceiling gives a graph. Only a positive, finite ceiling is one.
pub fn value_span(ceiling: Option<f32>) -> Option<f32> {
    ceiling.filter(|ceiling| ceiling.is_finite() && *ceiling > 0.0)
}

/// A proportional horizontal bar. Segments under one point are skipped.
pub fn stacked_bar(segments: Vec<(f32, Hsla)>, track: Hsla) -> impl IntoElement {
    let total: f32 = segments.iter().map(|(fraction, _)| fraction.max(0.0)).sum();
    let mut bar = div()
        .w_full()
        .h(px(8.0))
        .rounded(px(4.0))
        .bg(track)
        .flex()
        .flex_row();
    for (fraction, color) in segments.iter().copied() {
        if fraction <= 0.0 {
            continue;
        }
        bar = bar.child(div().h(px(8.0)).flex_grow(fraction).bg(color));
    }
    if total < 1.0 {
        bar = bar.child(div().h(px(8.0)).flex_grow(1.0 - total));
    }
    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_real_ceiling_gives_a_span() {
        assert_eq!(value_span(Some(100.0)), Some(100.0));
        // An engine that reported no total, a zero one, or a nonsensical one.
        assert_eq!(value_span(None), None);
        assert_eq!(value_span(Some(0.0)), None);
        assert_eq!(value_span(Some(-1.0)), None);
        assert_eq!(value_span(Some(f32::NAN)), None);
        assert_eq!(value_span(Some(f32::INFINITY)), None);
    }
}
