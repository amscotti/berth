//! Cell graphs: a grid of dots where each column is a slice of time and each
//! dot is one step of height. Drawn directly on a canvas so a few hundred dots
//! do not become a few hundred scene nodes.

use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{BorderStyle, Bounds, Hsla, IntoElement, Point, canvas, div, fill, outline, px, size};

const DOT: f32 = 4.0;
const PITCH: f32 = 6.0;
const INSET: f32 = 8.0;

/// A graph well: surface, hairline, fixed height, dots newest on the right.
///
/// The lattice is drawn first, a dot at every place one could be, so that a
/// low reading reads as low and not as absent. `ceiling` is the value the top
/// of the well stands for, `label` what it is called ("100%", "3.8 GiB").
/// The ceiling is fixed, never fitted to the data, so a flat series stays at
/// one height while its oldest samples leave the ring. `None` means the
/// ceiling isn't known, and nothing is drawn: a graph against an unknown
/// ceiling would lie, and a lattice would pretend to a scale. The highest
/// column is marked with a ring.
pub fn cell_graph(
    values: Vec<f32>,
    color: Hsla,
    tokens: Tokens,
    ceiling: Option<f32>,
    height: f32,
    label: Option<String>,
) -> impl IntoElement {
    let span = value_span(ceiling);
    let mut well = div()
        .w_full()
        .h(px(height))
        .rounded(px(8.0))
        .bg(tokens.well())
        .border_1()
        .border_color(tokens.well_border())
        .overflow_hidden()
        .relative()
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
                    let dot = |col: usize, step: usize| {
                        let x = f32::from(bounds.left()) + INSET + col as f32 * PITCH;
                        let y = f32::from(bounds.top()) + INSET + (rows - 1 - step) as f32 * PITCH;
                        Bounds::new(Point::new(px(x), px(y)), size(px(DOT), px(DOT)))
                    };
                    // An unknown ceiling: the well stays empty.
                    let Some(value_span) = span else {
                        return;
                    };

                    for col in 0..cols {
                        for step in 0..rows {
                            window.paint_quad(fill(dot(col, step), tokens.track));
                        }
                    }

                    let start = values.len().saturating_sub(cols);
                    let filled: Vec<usize> = values[start..]
                        .iter()
                        .map(|value| steps(*value, value_span, rows))
                        .collect();
                    // Right-align partial histories so newest samples sit at the
                    // right edge of the lattice.
                    let offset = column_offset(cols, filled.len());
                    for (i, &height) in filled.iter().enumerate() {
                        for step in 0..height.min(rows) {
                            window.paint_quad(fill(dot(offset + i, step), color));
                        }
                    }
                    // The highest column, and the newest where several are as
                    // high: the one to read the peak off.
                    if let Some(col) = peak(&filled)
                        && filled[col] > 0
                    {
                        let bounds = dot(offset + col, filled[col] - 1);
                        window.paint_quad(fill(bounds, color));
                        window.paint_quad(outline(bounds, tokens.text, BorderStyle::Solid));
                    }
                },
            )
            .size_full(),
        );
    if let (Some(label), Some(_)) = (label, span) {
        well = well.child(
            // Mounted after the canvas so it paints over the lattice rather
            // than behind its dots.
            div()
                .absolute()
                .top(px(2.0))
                .right(px(5.0))
                .px(px(3.0))
                .rounded(px(2.0))
                .bg(tokens.well())
                .text_size(px(type_scale::META))
                .font_features(crate::ui::tabular())
                .text_color(tokens.faint())
                .child(label),
        );
    }
    well
}

/// First lattice column for a history of `samples` points inside `cols` slots,
/// pinning the newest reading to the right edge.
fn column_offset(cols: usize, samples: usize) -> usize {
    cols.saturating_sub(samples)
}

/// How many steps of the lattice a value fills. Anything above zero shows at
/// least one step, so that a small reading is low and not missing.
fn steps(value: f32, span: f32, rows: usize) -> usize {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    ((value / span).clamp(0.0, 1.0) * rows as f32).ceil() as usize
}

/// The column holding the peak: the newest of the highest.
fn peak(filled: &[usize]) -> Option<usize> {
    filled
        .iter()
        .enumerate()
        .max_by_key(|&(col, &height)| (height, col))
        .map(|(col, _)| col)
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
    fn a_small_reading_is_low_and_not_missing() {
        // 5 MiB on a 3.8 GiB ceiling, in a well of nine rows.
        let span = 3.8 * 1024.0 * 1024.0 * 1024.0;
        assert_eq!(steps(5.0 * 1024.0 * 1024.0, span, 9), 1);
        assert_eq!(steps(span, span, 9), 9);
        assert_eq!(steps(span / 2.0, span, 9), 5);
        // Nothing, or less than nothing, or a value nobody measured: no dot.
        assert_eq!(steps(0.0, span, 9), 0);
        assert_eq!(steps(-1.0, span, 9), 0);
        assert_eq!(steps(f32::NAN, span, 9), 0);
    }

    #[test]
    fn the_peak_is_the_newest_of_the_highest_columns() {
        assert_eq!(peak(&[1, 3, 2, 3]), Some(3));
        assert_eq!(peak(&[4, 1, 2]), Some(0));
        // A flat line has its peak at its newest end; an empty one has none.
        assert_eq!(peak(&[2, 2, 2]), Some(2));
        assert_eq!(peak(&[0, 0]), Some(1));
        assert_eq!(peak(&[]), None);
    }

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

    #[test]
    fn partial_histories_align_newest_sample_to_the_right() {
        assert_eq!(column_offset(40, 24), 16);
        assert_eq!(column_offset(40, 40), 0);
        assert_eq!(column_offset(40, 60), 0);
        assert_eq!(column_offset(40, 1), 39);
    }
}
