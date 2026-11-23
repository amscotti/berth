//! A single-line text field.
//!
//! GPUI ships the input-handler trait but no field widget. berth needs search,
//! socket and form fields, so this is the shared primitive: a byte buffer with
//! a cursor and anchor, real text input for IME, and the chrome the design
//! contract uses.
//!
//! Where the characters are comes from the text system (`input_text.rs`). So a
//! click puts the caret where it looks like it should, a drag selects what it
//! looks like it should, the caret stays in view however long the text is, and
//! an IME candidate window sits on its own text.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use berth_core::TextBuffer;
use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, DispatchPhase, ElementInputHandler, EventEmitter, FocusHandle, Focusable,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Render, SharedString, Window, canvas, div, fill, point, px, size,
};

use self::text::{Line, PAD, reveal};
use super::kit::button;

#[path = "input_handler.rs"]
mod handler;
#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
#[path = "input_text.rs"]
mod text;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// The text changed, whether by the user or the app.
    Changed,
    /// The user changed the text themselves. Sent together with `Changed`,
    /// so a subscriber can tell an edit from the app filling a field in.
    Edited,
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
    marked: Option<Range<usize>>,
    /// How far the text is pushed left, so the caret stays in view.
    scroll: f32,
    /// Where the field was last drawn from. The platform asks where a point
    /// in the window falls in the text, and only the frame knows that.
    drawn_from: Rc<Cell<Pixels>>,
    /// The button is held on the field, and the selection follows the pointer.
    dragging: bool,
}

impl InputState {
    /// A field is a control of the usual height, set in the secondary size.
    const HEIGHT: f32 = layout::CONTROL;
    const FONT: f32 = type_scale::SECONDARY;
    /// What the clear button takes off the end of a field that has text: its
    /// own width, and room either side.
    const CLEAR_ROOM: f32 = layout::CONTROL_SMALL + 2.0 * 6.0;
}

impl EventEmitter<InputEvent> for InputState {}

impl InputState {
    pub fn new(placeholder: impl Into<SharedString>, width: f32, cx: &mut Context<Self>) -> Self {
        Self {
            buffer: TextBuffer::default(),
            placeholder: placeholder.into(),
            focus_handle: cx.focus_handle().tab_stop(true),
            width,
            marked: None,
            scroll: 0.0,
            drawn_from: Rc::new(Cell::new(px(0.0))),
            dragging: false,
        }
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// The app filling the field in. There is nothing to undo back to.
    pub fn set_value(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        self.buffer.set_value(value);
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.buffer.clear();
        cx.emit(InputEvent::Changed);
        cx.notify();
    }

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

    /// Where the caret is, in bytes into the text.
    #[cfg(test)]
    pub(crate) fn cursor(&self) -> usize {
        self.buffer.cursor()
    }

    /// What is selected, in bytes, or `None`.
    #[cfg(test)]
    pub(crate) fn selection(&self) -> Option<(usize, usize)> {
        self.buffer.selection()
    }

    /// How far the text is pushed left.
    #[cfg(test)]
    pub(crate) fn scroll(&self) -> f32 {
        self.scroll
    }

    /// Tell the platform and the page that the text is different.
    fn text_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        window.notify_text_input_changed(&self.focus_handle);
        self.edited(cx);
    }

    /// Tell the platform and the page that the selection is different.
    fn selection_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    /// Announce an edit made by the user.
    fn edited(&mut self, cx: &mut Context<Self>) {
        cx.emit(InputEvent::Edited);
        cx.emit(InputEvent::Changed);
        cx.notify();
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
            }
            "backspace" if modifiers.alt => {
                self.buffer.move_word_left(true);
                self.buffer.backspace();
            }
            "backspace" => {
                self.buffer.backspace();
            }
            "delete" => {
                self.buffer.delete();
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
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                }
                return;
            }
            "x" if command => {
                if let Some(text) = self.buffer.selected_text() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                    self.buffer.insert("");
                    self.text_changed(window, cx);
                }
                return;
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.buffer.insert(&text);
                    self.text_changed(window, cx);
                }
                return;
            }
            // Back through what was typed, and forward again. Cmd+Z and
            // Cmd+Shift+Z on a Mac; Ctrl+Z and Ctrl+Y elsewhere.
            "z" if command => {
                self.step_back(modifiers.shift, window, cx);
                return;
            }
            "y" if modifiers.control && !modifiers.platform => {
                self.step_back(true, window, cx);
                return;
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

        self.text_changed(window, cx);
    }

    /// Undo the last step, or redo one with `forward`.
    fn step_back(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let stepped = if forward {
            self.buffer.redo()
        } else {
            self.buffer.undo()
        };
        if !stepped {
            return;
        }
        self.marked = None;
        window.notify_text_input_changed(&self.focus_handle);
        self.edited(cx);
    }

    /// The byte offset whose caret is nearest to `x`, a point in the window
    /// over a field drawn inside `bounds`.
    fn offset_at(&self, x: Pixels, bounds: Bounds<Pixels>, window: &Window) -> usize {
        let line = Line::new(self.buffer.text(), gpui::black(), window);
        let into_text = f32::from(x) - f32::from(bounds.left()) - PAD + self.scroll;
        line.offset_at(into_text)
    }

    /// A press on the field. The caret goes where the pointer is; a second
    /// press in the same place selects the word under it.
    fn press(
        &mut self,
        x: Pixels,
        extend: bool,
        bounds: Bounds<Pixels>,
        clicks: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.buffer
            .set_cursor(self.offset_at(x, bounds, window), extend);
        if clicks == 2 {
            self.select_word();
        }
        self.dragging = true;
        self.selection_changed(window, cx);
    }

    /// The pointer moved while the button is held: the selection follows it.
    fn drag_to(
        &mut self,
        x: Pixels,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dragging {
            return;
        }
        let offset = self.offset_at(x, bounds, window);
        if offset != self.buffer.cursor() {
            self.buffer.set_cursor(offset, true);
            self.selection_changed(window, cx);
        }
    }

    /// Select the word the caret is in. A word is letters, digits and `_`.
    fn select_word(&mut self) {
        let text = self.buffer.text();
        let word = |character: char| character.is_alphanumeric() || character == '_';
        let at = self.buffer.cursor();
        let start = text[..at]
            .char_indices()
            .rev()
            .take_while(|(_, character)| word(*character))
            .last()
            .map_or(at, |(byte, _)| byte);
        let end = at
            + text[at..]
                .char_indices()
                .take_while(|(_, character)| word(*character))
                .last()
                .map_or(0, |(byte, character)| byte + character.len_utf8());
        self.buffer.set_cursor(start, false);
        self.buffer.set_cursor(end, true);
    }
}

impl Focusable for InputState {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for InputState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The root publishes its tokens for entities that render on their
        // own. Reading the window's appearance here would ignore an
        // Appearance choice that differs from the system's.
        let tokens = berth_ui::theme::ActiveTokens::get(window, cx);
        let focused = self.focus_handle.is_focused(window);
        let text = self.buffer.text().to_owned();
        let empty = text.is_empty();
        let line = if empty {
            Line::new(&self.placeholder, tokens.muted, window)
        } else {
            Line::new(&text, tokens.text, window)
        };
        // The text has the field to itself, less its padding, and less the
        // clear button while there is something to clear.
        let clear_room = if empty { 0.0 } else { Self::CLEAR_ROOM };
        let visible = (self.width - 2.0 - 2.0 * PAD - clear_room).max(0.0);
        let caret_x = line.x_of(self.buffer.cursor());
        self.scroll = reveal(self.scroll, caret_x, line.width(), visible);

        let selection = self.buffer.selection();
        let scroll = self.scroll;
        let entity = cx.entity();
        let focus = self.focus_handle.clone();
        let drawn_from = self.drawn_from.clone();

        let field = div()
            .id(("input", cx.entity_id().as_u64()))
            .debug_selector({
                let placeholder = self.placeholder.clone();
                move || format!("input-{placeholder}")
            })
            .relative()
            .flex()
            .items_center()
            .w(px(self.width))
            .h(px(Self::HEIGHT))
            .rounded(px(layout::RADIUS_NESTED))
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
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        // The canvas covers the field, so its bounds are the
                        // field's: the handler answers for the whole of it.
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        drawn_from.set(bounds.left());
                        paint_text(
                            bounds, &line, selection, caret_x, scroll, focused, tokens, window, cx,
                        );
                        listen_to_the_pointer(bounds, entity.clone(), window);
                    },
                )
                .absolute()
                .inset_0(),
            );

        if empty {
            field
        } else {
            // The clear button is a control of its own: Tab reaches it, Enter
            // or Space presses it, and it gives the field focus back.
            field.child(
                div()
                    .absolute()
                    .right(px(6.0))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(
                        button(
                            ("input-clear", cx.entity_id().as_u64()),
                            format!("Clear {}", lower_first(&self.placeholder)),
                            tokens,
                        )
                        .small()
                        .icon("xmark")
                        .icon_only()
                        .on_click(cx.listener(|this, _, window, cx| {
                            // The press is for the button, not the field.
                            cx.stop_propagation();
                            this.clear(cx);
                            this.focus(window, cx);
                            window.notify_text_input_changed(&this.focus_handle);
                        })),
                    ),
            )
        }
    }
}

/// `"Search containers"` as it would read after "Clear": `"search containers"`.
fn lower_first(words: &str) -> String {
    let mut words = words.to_owned();
    if let Some(first) = words.get_mut(0..1) {
        // Only an ASCII letter is safe to lower one byte at a time.
        if first.is_ascii() {
            first.make_ascii_lowercase();
        }
    }
    words
}

/// Draw a field's line: the selection, then the caret, then the text over both.
#[allow(clippy::too_many_arguments)]
fn paint_text(
    bounds: Bounds<Pixels>,
    line: &Line,
    selection: Option<(usize, usize)>,
    caret_x: f32,
    scroll: f32,
    focused: bool,
    tokens: Tokens,
    window: &mut Window,
    cx: &mut App,
) {
    let from = bounds.left() + px(PAD) - px(scroll);
    if let Some((start, end)) = selection {
        let x = from + px(line.x_of(start));
        let width = px(line.x_of(end) - line.x_of(start));
        window.paint_quad(fill(
            Bounds::new(
                point(x, bounds.top() + px(5.0)),
                size(width, bounds.size.height - px(10.0)),
            ),
            tokens.text_selection(),
        ));
    }
    if focused {
        window.paint_quad(fill(
            Bounds::new(
                point(from + px(caret_x), bounds.top() + px(6.0)),
                size(px(1.5), bounds.size.height - px(12.0)),
            ),
            tokens.accent,
        ));
    }
    line.paint(bounds.origin, bounds.size.height, window, cx);
}

/// Answer the pointer over a field drawn inside `bounds`: a press puts the
/// caret down, a held button drags the selection, letting go ends the drag.
fn listen_to_the_pointer(
    bounds: Bounds<Pixels>,
    entity: gpui::Entity<InputState>,
    window: &mut Window,
) {
    window.on_mouse_event({
        let entity = entity.clone();
        move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && bounds.contains(&event.position)
                // Something drawn over the field, such as its clear button,
                // has taken the press.
                && !window.default_prevented()
            {
                entity.update(cx, |this, cx| {
                    this.press(
                        event.position.x,
                        event.modifiers.shift,
                        bounds,
                        event.click_count,
                        window,
                        cx,
                    )
                });
            }
        }
    });
    // The drag is followed wherever the pointer goes, on the field or off it.
    window.on_mouse_event({
        let entity = entity.clone();
        move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                entity.update(cx, |this, cx| {
                    this.drag_to(event.position.x, bounds, window, cx)
                });
            }
        }
    });
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Bubble {
            entity.update(cx, |this, _| this.dragging = false);
        }
    });
}
