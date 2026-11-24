//! What the platform asks a text field, and the field's answers.
//!
//! The platform speaks UTF-16 offsets; the buffer speaks bytes. Where a
//! character is on screen comes from the text system's layout of the line, so
//! an IME candidate window opens over the text it belongs to, wherever the
//! field has scrolled to.

use gpui::{Bounds, Context, UTF16Selection, Window, point, px, size};

use super::InputState;
use super::text::{Line, PAD};

impl gpui::EntityInputHandler for InputState {
    fn text_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.buffer.text();
        let start = utf16_to_byte(text, range_utf16.start);
        let end = utf16_to_byte(text, range_utf16.end);
        adjusted_range.replace(byte_to_utf16(text, start)..byte_to_utf16(text, end));
        Some(text[start..end].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.buffer.text();
        let (start, end) = self
            .buffer
            .selection()
            .unwrap_or((self.buffer.cursor(), self.buffer.cursor()));
        Some(UTF16Selection {
            range: byte_to_utf16(text, start)..byte_to_utf16(text, end),
            reversed: self
                .buffer
                .anchor()
                .is_some_and(|anchor| anchor > self.buffer.cursor()),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        self.marked.as_ref().map(|range| {
            let text = self.buffer.text();
            byte_to_utf16(text, range.start)..byte_to_utf16(text, range.end)
        })
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer_text = self.buffer.text().to_owned();
        if let Some(range) = range {
            let start = utf16_to_byte(&buffer_text, range.start);
            let end = utf16_to_byte(&buffer_text, range.end);
            self.buffer.set_cursor(start, false);
            self.buffer.set_cursor(end, true);
        }
        self.buffer.insert(text);
        self.marked = None;
        window.notify_text_input_changed(&self.focus_handle);
        self.edited(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<std::ops::Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A character being composed is rewritten as it is typed: "n", "ni",
        // "に". The first stroke opens a step to undo; the rest belong to it.
        if self.marked.is_some() {
            self.buffer.join_step();
        }
        self.replace_text_in_range(range, new_text, window, cx);
        self.marked =
            Some(self.buffer.cursor().saturating_sub(new_text.len())..self.buffer.cursor());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: Bounds<gpui::Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<gpui::Pixels>> {
        let text = self.buffer.text().to_owned();
        let start = utf16_to_byte(&text, range_utf16.start);
        let end = utf16_to_byte(&text, range_utf16.end);
        let line = Line::new(&text, gpui::black(), window);
        let x = element_bounds.left() + px(PAD + line.x_of(start) - self.scroll);
        let width = (line.x_of(end) - line.x_of(start)).max(1.0);
        Some(Bounds::new(
            point(x, element_bounds.top()),
            size(px(width), element_bounds.size.height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let text = self.buffer.text().to_owned();
        let line = Line::new(&text, gpui::black(), window);
        let into_text = f32::from(point.x) - f32::from(self.drawn_from.get()) - PAD + self.scroll;
        Some(byte_to_utf16(&text, line.offset_at(into_text)))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.buffer.text().to_owned();
        let start = utf16_to_byte(&text, range_utf16.start);
        let end = utf16_to_byte(&text, range_utf16.end);
        self.buffer.set_cursor(start, false);
        self.buffer.set_cursor(end, true);
        self.selection_changed(window, cx);
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.buffer.text().encode_utf16().count())
    }
}

/// The byte offset of a UTF-16 offset into `text`, snapped to a boundary.
pub(crate) fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    if utf16 == 0 {
        return 0;
    }
    let mut seen = 0;
    for (byte, character) in text.char_indices() {
        if seen >= utf16 {
            return byte;
        }
        seen += character.len_utf16();
    }
    text.len()
}

/// The UTF-16 offset of a byte offset into `text`.
pub(crate) fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_round_trips_over_multibyte_text() {
        let text = "aに😀b";
        for (byte, _) in text.char_indices() {
            let utf16 = byte_to_utf16(text, byte);
            assert_eq!(utf16_to_byte(text, utf16), byte);
        }
        assert_eq!(utf16_to_byte(text, 999), text.len());
    }
}
