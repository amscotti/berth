//! Where the characters of a field are, and how the line is drawn.
//!
//! The field used to guess, at so many pixels a character. That put the caret
//! in the wrong place in any text but the plainest, moved an IME candidate
//! window off its text, and left long text with its caret off the field.
//! Everything here comes from the text system's own layout of the line.

use gpui::{App, Font, Hsla, Pixels, Point, ShapedLine, TextAlign, TextRun, Window, px};

use super::InputState;

/// Room inside a field, between its border and its text.
pub(crate) const PAD: f32 = 9.0;

/// A field's line, shaped: where every character is.
pub(crate) struct Line {
    shaped: ShapedLine,
}

impl Line {
    /// Shape `text` as a field shows it: in the font the page hands down, at
    /// the size a field is set in.
    pub(crate) fn new(text: &str, color: Hsla, window: &Window) -> Self {
        let style = window.text_style();
        let font = Font {
            family: style.font_family.clone(),
            features: style.font_features.clone(),
            fallbacks: style.font_fallbacks.clone(),
            weight: style.font_weight,
            style: style.font_style,
        };
        let shaped = window.text_system().shape_line(
            text.to_owned().into(),
            px(InputState::FONT),
            &[TextRun {
                len: text.len(),
                font,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        Self { shaped }
    }

    /// How wide the line is.
    pub(crate) fn width(&self) -> f32 {
        self.shaped.width.into()
    }

    /// The x of the caret when it sits just after byte `offset`.
    pub(crate) fn x_of(&self, offset: usize) -> f32 {
        self.shaped.x_for_index(offset).into()
    }

    /// The byte offset whose caret is nearest to `x`, measured from the start
    /// of the text. Past either end is the end.
    pub(crate) fn offset_at(&self, x: f32) -> usize {
        self.shaped.closest_index_for_x(px(x))
    }

    /// Draw the line, vertically centred in a box `height` tall whose corner
    /// is `origin`.
    pub(crate) fn paint(
        &self,
        origin: Point<Pixels>,
        height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.shaped.len() > 0 {
            let _ = self
                .shaped
                .paint(origin, height, TextAlign::Left, None, window, cx);
        }
    }
}

/// How far the text is pushed left so that the caret stays in view.
///
/// `scrolled` is where it was. A caret already in view doesn't move the text:
/// walking the cursor back through what is on screen shouldn't make it jump.
pub(crate) fn reveal(scrolled: f32, caret_x: f32, text_w: f32, visible_w: f32) -> f32 {
    // Text shorter than the field isn't scrolled, however far it was before.
    let furthest = (text_w - visible_w).max(0.0);
    let scrolled = scrolled.clamp(0.0, furthest);
    if caret_x < scrolled {
        caret_x
    } else if caret_x > scrolled + visible_w {
        (caret_x - visible_w).min(furthest)
    } else {
        scrolled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caret_in_view_leaves_the_text_alone() {
        assert_eq!(reveal(0.0, 30.0, 400.0, 150.0), 0.0);
        assert_eq!(reveal(120.0, 200.0, 400.0, 150.0), 120.0);
    }

    #[test]
    fn a_caret_past_the_edge_brings_its_edge_along() {
        // Typing on past the right edge scrolls by exactly as much as it takes.
        assert_eq!(reveal(0.0, 160.0, 400.0, 150.0), 10.0);
        // Walking back past the left edge brings the start back.
        assert_eq!(reveal(120.0, 100.0, 400.0, 150.0), 100.0);
    }

    #[test]
    fn text_that_shrank_stops_being_scrolled() {
        assert_eq!(reveal(120.0, 0.0, 100.0, 150.0), 0.0);
        // And never further than its own end.
        assert_eq!(reveal(500.0, 400.0, 400.0, 150.0), 250.0);
    }
}
