//! A small single-line text field.
//!
//! GPUI ships the input-handler trait but no field widget. berth needs search,
//! socket and form fields, so this is the shared primitive: a byte buffer with
//! a cursor and anchor, real text input for IME, and the chrome the design
//! contract uses.

use std::ops::Range;

use berth_core::TextBuffer;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, IntoElement, KeyDownEvent, Pixels, Point, Render, SharedString,
    UTF16Selection, Window, canvas, div, px,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Changed,
    Submitted,
    Cancelled,
}

/// A single-line field. One entity per field; the root reads `text()` when it
/// renders or submits.
pub struct InputState {
    buffer: TextBuffer,
    placeholder: SharedString,
    focus_handle: FocusHandle,
    width: f32,
    height: f32,
    font_size: f32,
    marked: Option<Range<usize>>,
}

impl EventEmitter<InputEvent> for InputState {}

impl InputState {
    pub fn new(placeholder: impl Into<SharedString>, width: f32, cx: &mut Context<Self>) -> Self {
        Self {
            buffer: TextBuffer::default(),
            placeholder: placeholder.into(),
            focus_handle: cx.focus_handle(),
            width,
            height: 28.0,
            font_size: 12.5,
            marked: None,
        }
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[allow(dead_code)]
    pub fn set_value(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        self.buffer.set_value(value);
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.buffer.clear();
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    pub fn set_width(&mut self, width: f32) {
        if (self.width - width).abs() > f32::EPSILON {
            self.width = width;
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let command = modifiers.platform || modifiers.control;
        let extend = modifiers.shift;

        match key {
            "backspace" if command => {
                self.buffer.home(true);
                self.buffer.backspace();
                cx.emit(InputEvent::Changed);
            }
            "backspace" if modifiers.alt => {
                self.buffer.move_word_left(true);
                self.buffer.backspace();
                cx.emit(InputEvent::Changed);
            }
            "backspace" => {
                self.buffer.backspace();
                cx.emit(InputEvent::Changed);
            }
            "delete" => {
                self.buffer.delete();
                cx.emit(InputEvent::Changed);
            }
            "left" if command => self.buffer.home(extend),
            "right" if command => self.buffer.end(extend),
            "left" if modifiers.alt => self.buffer.move_word_left(extend),
            "right" if modifiers.alt => self.buffer.move_word_right(extend),
            "left" => self.buffer.move_left(extend),
            "right" => self.buffer.move_right(extend),
            "home" => self.buffer.home(extend),
            "end" => self.buffer.end(extend),
            "a" if command => self.buffer.select_all(),
            "c" if command => {
                if let Some(text) = self.buffer.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                }
                return;
            }
            "x" if command => {
                if let Some(text) = self.buffer.selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                    self.buffer.insert("");
                    cx.emit(InputEvent::Changed);
                }
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.buffer.insert(&text);
                    cx.emit(InputEvent::Changed);
                }
            }
            "enter" => {
                if self.marked.take().is_some() {
                    cx.notify();
                    return;
                }
                cx.emit(InputEvent::Submitted);
                return;
            }
            "escape" => {
                if self.marked.take().is_some() {
                    cx.notify();
                    return;
                }
                cx.emit(InputEvent::Cancelled);
                return;
            }
            _ => return,
        }

        self.marked = None;
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn focus_and_place_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.buffer.end(false);
        cx.notify();
    }

    fn visual_parts(&self) -> (&str, &str, &str) {
        let text = self.buffer.text();
        if let Some((start, end)) = self.buffer.selection() {
            (&text[..start], &text[start..end], &text[end..])
        } else {
            let cursor = self.buffer.cursor().min(text.len());
            (&text[..cursor], "", &text[cursor..])
        }
    }
}

impl Focusable for InputState {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for InputState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The tokens are resolved by the root; the field reads the current
        // window appearance so it stays correct without a second observer.
        let tokens = berth_ui::theme::for_appearance(window.appearance());
        let focused = self.focus_handle.is_focused(window);
        let empty = self.buffer.is_empty();
        let (before, selected, after) = self.visual_parts();
        let entity = cx.entity();
        let focus = self.focus_handle.clone();

        let mut field = div()
            .id(("input", cx.entity_id().as_u64()))
            .flex()
            .flex_row()
            .items_center()
            .w(px(self.width))
            .h(px(self.height))
            .px(px(9.0))
            .rounded(px(6.0))
            .bg(tokens.surface)
            .border_1()
            .border_color(if focused {
                tokens.accent
            } else {
                tokens.border
            })
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .key_context("Input")
            .role(gpui::Role::TextInput)
            .aria_label(self.placeholder.to_string())
            .on_key_down(cx.listener(Self::on_key_down))
            .on_click(cx.listener(|this, _, window, cx| {
                this.focus_and_place_caret(window, cx);
            }));

        if empty {
            if focused {
                field = field.child(
                    div()
                        .w(px(1.5))
                        .h(px(self.font_size + 2.0))
                        .bg(tokens.accent),
                );
            }
            field = field.child(
                div()
                    .text_size(px(self.font_size))
                    .text_color(tokens.muted)
                    .child(self.placeholder.clone()),
            );
        } else {
            if !before.is_empty() {
                field = field.child(
                    div()
                        .text_size(px(self.font_size))
                        .text_color(tokens.text)
                        .child(before.to_owned()),
                );
            }
            if !selected.is_empty() {
                field = field.child(
                    div().rounded(px(2.0)).bg(tokens.accent.alpha(0.24)).child(
                        div()
                            .text_size(px(self.font_size))
                            .text_color(tokens.text)
                            .child(selected.to_owned()),
                    ),
                );
            }
            if focused {
                field = field.child(
                    div()
                        .w(px(1.5))
                        .h(px(self.font_size + 2.0))
                        .bg(tokens.accent),
                );
            }
            if !after.is_empty() {
                field = field.child(
                    div()
                        .text_size(px(self.font_size))
                        .text_color(tokens.text)
                        .child(after.to_owned()),
                );
            }
        }

        // The canvas is an invisible overlay that installs the text-input
        // handler with the field's real bounds. Without it, typed characters
        // and IME never reach `EntityInputHandler`.
        field.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    window.handle_input(
                        &focus,
                        ElementInputHandler::new(bounds, entity.clone()),
                        cx,
                    );
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

impl EntityInputHandler for InputState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
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
    ) -> Option<Range<usize>> {
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
        range: Option<Range<usize>>,
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
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range, new_text, window, cx);
        self.marked =
            Some(self.buffer.cursor().saturating_sub(new_text.len())..self.buffer.cursor());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let text = self.buffer.text();
        let start = utf16_to_byte(text, range_utf16.start);
        let end = utf16_to_byte(text, range_utf16.end);
        let char_width = self.font_size * 0.58;
        let prefix_chars = text[..start].chars().count();
        let span_chars = text[start..end].chars().count().max(1);
        let x = element_bounds.left() + px(9.0) + px(prefix_chars as f32 * char_width);
        let width = px(span_chars as f32 * char_width);
        Some(Bounds::new(
            Point::new(x, element_bounds.top()),
            gpui::size(width, element_bounds.size.height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let _ = window;
        let text = self.buffer.text();
        let char_width = self.font_size * 0.58;
        let x = (f32::from(point.x) - 9.0).max(0.0);
        let index = ((x / char_width).round()).max(0.0) as usize;
        Some(byte_to_utf16(text, snap_to_char_boundary(text, index)))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.buffer.text().to_owned();
        let start = utf16_to_byte(&text, range_utf16.start);
        let end = utf16_to_byte(&text, range_utf16.end);
        self.buffer.set_cursor(start, false);
        self.buffer.set_cursor(end, true);
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.buffer.text().encode_utf16().count())
    }
}

fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    if utf16 == 0 {
        return 0;
    }
    let mut seen = 0;
    for (byte, ch) in text.char_indices() {
        if seen >= utf16 {
            return byte;
        }
        seen += ch.len_utf16();
    }
    text.len()
}

fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

fn snap_to_char_boundary(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte, _)| byte)
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

    #[test]
    fn snapping_uses_a_grapheme_boundary() {
        let text = "aにb";
        assert_eq!(snap_to_char_boundary(text, 0), 0);
        assert_eq!(snap_to_char_boundary(text, 1), 1);
        assert_eq!(snap_to_char_boundary(text, 2), 4);
        assert_eq!(snap_to_char_boundary(text, 99), text.len());
    }
}

#[cfg(test)]
mod gpui_tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn typed_characters_reach_the_buffer(cx: &mut TestAppContext) {
        let (input, cx) = cx.add_window_view(|window, cx| {
            let state = InputState::new("Search containers", 190.0, cx);
            state.focus_handle.focus(window, cx);
            state
        });
        cx.run_until_parked();
        cx.simulate_input("nginx");
        assert_eq!(
            input.read_with(&*cx, |input, _| input.text().to_owned()),
            "nginx"
        );
        cx.simulate_keystrokes("backspace");
        assert_eq!(
            input.read_with(&*cx, |input, _| input.text().to_owned()),
            "ngin"
        );
        cx.simulate_keystrokes("left left");
        cx.simulate_input("X");
        assert_eq!(
            input.read_with(&*cx, |input, _| input.text().to_owned()),
            "ngXin"
        );
    }

    #[gpui::test]
    fn command_a_selects_and_typing_replaces(cx: &mut TestAppContext) {
        let (input, cx) = cx.add_window_view(|window, cx| {
            let state = InputState::new("Search", 190.0, cx);
            state.focus_handle.focus(window, cx);
            state
        });
        cx.run_until_parked();
        cx.simulate_input("alpine");
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("nginx");
        assert_eq!(
            input.read_with(&*cx, |input, _| input.text().to_owned()),
            "nginx"
        );
    }
}
