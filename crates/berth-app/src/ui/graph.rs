//! Cell graphs: a grid of dots where each column is a slice of time and each
//! dot is one step of height. Drawn directly on a canvas so a few hundred dots
//! do not become a few hundred scene nodes.

use gpui::prelude::*;
use gpui::{Bounds, Hsla, IntoElement, Point, canvas, div, fill, px, size};

const DOT: f32 = 4.0;
const PITCH: f32 = 6.0;
const INSET: f32 = 8.0;

/// A graph well: surface, hairline, fixed height, dots newest on the right.
#[allow(dead_code)]
pub fn cell_graph(
    values: Vec<f32>,
    color: Hsla,
    grid_color: Hsla,
    scale: f32,
    grid: bool,
) -> impl IntoElement {
    cell_graph_with_height(values, color, grid_color, scale, grid, 70.0)
}

pub fn cell_graph_with_height(
    values: Vec<f32>,
    color: Hsla,
    grid_color: Hsla,
    scale: f32,
    grid: bool,
    height: f32,
) -> impl IntoElement {
    div()
        .w_full()
        .h(px(height))
        .rounded(px(8.0))
        .bg(grid_color.alpha(0.28))
        .border_1()
        .border_color(grid_color.alpha(0.55))
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
                    let value_span = if scale > 0.0 { scale } else { 1.0 };

                    if grid {
                        for row in (0..rows).step_by(3) {
                            for col in 0..cols {
                                let x = f32::from(bounds.left()) + INSET + col as f32 * PITCH;
                                let y = f32::from(bounds.top()) + INSET + row as f32 * PITCH;
                                window.paint_quad(fill(
                                    Bounds::new(Point::new(px(x), px(y)), size(px(1.0), px(1.0))),
                                    grid_color.alpha(0.35),
                                ));
                            }
                        }
                    }

                    let start = values.len().saturating_sub(cols);
                    for (offset, value) in values[start..].iter().enumerate() {
                        let col = offset;
                        let height =
                            ((*value / value_span).clamp(0.0, 1.0) * rows as f32).ceil() as usize;
                        for step in 0..height.min(rows) {
                            let x = f32::from(bounds.left()) + INSET + col as f32 * PITCH;
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

/// A count graph's top: the whole fleet, so one container is one unit.
#[allow(dead_code)]
pub fn count_scale(total: usize) -> f32 {
    total.max(1) as f32
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
