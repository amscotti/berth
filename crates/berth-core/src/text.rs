//! A single-line text buffer: a value, a byte cursor, an optional anchor, and
//! the steps back to what was typed before. Every offset is kept on a UTF-8
//! boundary; externally supplied offsets are snapped before use.

/// How far back an edit can be undone. A field holds one line; nobody types
/// two hundred words and then wants the first one back.
const MAX_UNDO: usize = 200;

/// The editing state behind one text field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextBuffer {
    value: String,
    cursor: usize,
    anchor: Option<usize>,
    /// The field as it was before each step, oldest first.
    undo: Vec<Memory>,
    /// The steps undone, newest last, ready to be redone.
    redo: Vec<Memory>,
    /// Where a word being typed ends, so that typing on is still one step.
    typing: Option<usize>,
    /// The next edit undoes together with the step before it, whatever it is.
    joining: bool,
}

/// The field as it was, to go back to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Memory {
    value: String,
    cursor: usize,
    anchor: Option<usize>,
}

impl TextBuffer {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();
        Self {
            value,
            cursor,
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.value
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// The app filling the field in. There is nothing to undo back to.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.value.len();
        self.anchor = None;
        self.forget_history();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.anchor = None;
        self.forget_history();
    }

    /// Take back the last step. False when there is none.
    pub fn undo(&mut self) -> bool {
        let Some(before) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.remember());
        self.restore(before);
        true
    }

    /// Bring back a step that was taken back. False when there is none.
    pub fn redo(&mut self) -> bool {
        let Some(after) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.remember());
        self.restore(after);
        true
    }

    /// Let the next edit undo together with the step before it, whatever the
    /// edit is: the stages of one composed character are one step, not five.
    pub fn join_step(&mut self) {
        self.joining = true;
    }

    fn remember(&self) -> Memory {
        Memory {
            value: self.value.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    fn restore(&mut self, memory: Memory) {
        self.value = memory.value;
        self.cursor = memory.cursor;
        self.anchor = memory.anchor;
        self.break_run();
    }

    fn forget_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.break_run();
    }

    /// Start a new step, unless this edit runs on from the one before it.
    fn begin_step(&mut self, runs_on: bool) {
        if self.joining || (runs_on && self.typing == Some(self.cursor)) {
            self.joining = false;
        } else {
            if self.undo.len() >= MAX_UNDO {
                self.undo.remove(0);
            }
            self.undo.push(self.remember());
            self.typing = None;
        }
        // An edit after an undo leaves nowhere to redo to.
        self.redo.clear();
    }

    /// Whatever was being typed is over: the next character starts a step.
    fn break_run(&mut self) {
        self.typing = None;
        self.joining = false;
    }

    /// The selected byte range, always ordered.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        let start = anchor.min(self.cursor).min(self.value.len());
        let end = anchor.max(self.cursor).min(self.value.len());
        (start < end).then_some((start, end))
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(start, end)| &self.value[start..end])
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.value.len();
        self.break_run();
    }

    pub fn set_cursor(&mut self, offset: usize, extend: bool) {
        let offset = self.snap(offset);
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = offset;
    }

    /// Type `text` over the selection. A word typed character by character
    /// is one step to undo; a space starts the next.
    pub fn insert(&mut self, text: &str) {
        let word = !text.is_empty() && !text.chars().any(char::is_whitespace);
        self.begin_step(word);
        self.delete_selection();
        let at = self.cursor.min(self.value.len());
        self.value.insert_str(at, text);
        self.cursor = at + text.len();
        self.anchor = None;
        self.typing = word.then_some(self.cursor);
    }

    pub fn backspace(&mut self) {
        self.begin_step(false);
        if self.delete_selection() {
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let start = self.prev_boundary(self.cursor);
        self.value.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn delete(&mut self) {
        self.begin_step(false);
        if self.delete_selection() {
            return;
        }
        if self.cursor >= self.value.len() {
            return;
        }
        let end = self.next_boundary(self.cursor);
        self.value.replace_range(self.cursor..end, "");
    }

    pub fn move_left(&mut self, extend: bool) {
        self.break_run();
        if !extend && let Some((start, _)) = self.selection() {
            self.set_cursor(start, false);
            return;
        }
        self.set_cursor(self.prev_boundary(self.cursor), extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        self.break_run();
        if !extend && let Some((_, end)) = self.selection() {
            self.set_cursor(end, false);
            return;
        }
        self.set_cursor(self.next_boundary(self.cursor), extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
        self.break_run();
        let mut at = self.cursor;
        while at > 0
            && self.value[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            at = self.prev_boundary(at);
        }
        while at > 0
            && !self.value[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            at = self.prev_boundary(at);
        }
        self.set_cursor(at, extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        self.break_run();
        let mut at = self.cursor;
        let len = self.value.len();
        while at < len
            && !self.value[at..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
        {
            at = self.next_boundary(at);
        }
        while at < len
            && self.value[at..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
        {
            at = self.next_boundary(at);
        }
        self.set_cursor(at, extend);
    }

    pub fn home(&mut self, extend: bool) {
        self.break_run();
        self.set_cursor(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.break_run();
        self.set_cursor(self.value.len(), extend);
    }

    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            return false;
        };
        self.value.replace_range(start..end, "");
        self.cursor = start;
        self.anchor = None;
        true
    }

    fn snap(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.value.len());
        while offset > 0 && !self.value.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn prev_boundary(&self, offset: usize) -> usize {
        let mut offset = self.snap(offset);
        while offset > 0 {
            let prev = self.value[..offset]
                .char_indices()
                .next_back()
                .map_or(0, |(idx, _)| idx);
            let ch = self.value[prev..offset].chars().next().unwrap_or('\0');
            offset = prev;
            if !is_cluster_continuation(ch) && !self.value[..offset].ends_with('\u{200D}') {
                break;
            }
        }
        offset
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let mut offset = self.snap(offset);
        if offset >= self.value.len() {
            return self.value.len();
        }
        let mut iter = self.value[offset..].char_indices().peekable();
        let Some((_, first)) = iter.next() else {
            return self.value.len();
        };
        let mut prev_was_zwj = first == '\u{200D}';
        offset += first.len_utf8();
        while let Some(&(_, next_ch)) = iter.peek() {
            if is_cluster_continuation(next_ch) || prev_was_zwj {
                prev_was_zwj = next_ch == '\u{200D}';
                offset += next_ch.len_utf8();
                iter.next();
            } else {
                break;
            }
        }
        offset
    }
}

fn is_cluster_continuation(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036F
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x200D
            | 0x20D0..=0x20FF
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0x1F3FB..=0x1F3FF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> TextBuffer {
        TextBuffer::new(text)
    }

    #[test]
    fn typing_inserts_at_the_cursor_and_replaces_a_selection() {
        let mut b = buffer("hello");
        b.set_cursor(5, false);
        b.insert("!");
        assert_eq!(b.text(), "hello!");
        b.select_all();
        b.insert("bye");
        assert_eq!(b.text(), "bye");
        assert_eq!(b.cursor(), 3);
    }

    #[test]
    fn backspace_and_delete_are_grapheme_boundary_safe() {
        let mut b = buffer("aに😀");
        b.set_cursor(b.text().len(), false);
        b.backspace();
        assert_eq!(b.text(), "aに");
        b.set_cursor(0, false);
        b.delete();
        assert_eq!(b.text(), "に");
        b.set_cursor(0, false);
        b.delete();
        assert_eq!(b.text(), "");
    }

    #[test]
    fn movement_collapses_then_extends_a_selection() {
        let mut b = buffer("abcd");
        b.set_cursor(1, false);
        b.move_right(true);
        b.move_right(true);
        assert_eq!(b.selection(), Some((1, 3)));
        b.move_left(false);
        assert_eq!(b.selection(), None);
        assert_eq!(b.cursor(), 1);
        b.move_right(false);
        assert_eq!(b.cursor(), 2);
    }

    #[test]
    fn word_movement_skips_whitespace() {
        let mut b = buffer("one two  three");
        b.set_cursor(0, false);
        b.move_word_right(false);
        assert_eq!(b.cursor(), 4);
        b.move_word_right(false);
        assert_eq!(b.cursor(), 9);
        b.move_word_left(false);
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn external_offsets_are_snapped() {
        let mut b = buffer("aにb");
        b.set_cursor(2, false); // inside the 3-byte に
        assert_eq!(b.cursor(), 1);
        b.set_cursor(999, false);
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn a_word_typed_is_one_step_back_and_a_space_starts_the_next() {
        let mut b = buffer("");
        for key in ["one", " ", "two"] {
            for ch in key.chars() {
                b.insert(&ch.to_string());
            }
        }
        assert_eq!(b.text(), "one two");
        assert!(b.undo());
        assert_eq!(b.text(), "one ");
        assert!(b.undo());
        assert_eq!(b.text(), "one");
        assert!(b.undo());
        assert_eq!(b.text(), "");
        // Nothing before the beginning.
        assert!(!b.undo());
    }

    #[test]
    fn redo_bring_back_a_step_until_something_new_is_typed() {
        let mut b = buffer("");
        b.insert("hello");
        assert!(b.undo());
        assert_eq!(b.text(), "");
        assert!(b.redo());
        assert_eq!(b.text(), "hello");
        // A new edit leaves nowhere to redo to.
        assert!(b.undo());
        b.insert("bye");
        assert!(!b.redo());
        assert_eq!(b.text(), "bye");
    }

    #[test]
    fn deleting_and_moving_the_cursor_are_steps_of_their_own() {
        let mut b = buffer("");
        b.insert("ab");
        b.backspace();
        assert_eq!(b.text(), "a");
        b.insert("c");
        assert_eq!(b.text(), "ac");
        // Moving the cursor ends the run of typing, so "c" is its own step.
        assert!(b.undo());
        assert_eq!(b.text(), "a");
        assert!(b.undo());
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn the_stages_of_a_composed_character_are_one_step() {
        let mut b = buffer("x ");
        // A composition is written, then rewritten, then left alone.
        b.insert("n");
        b.join_step();
        b.set_cursor(2, false);
        b.set_cursor(3, true);
        b.insert("ni");
        assert!(b.undo());
        assert_eq!(b.text(), "x ");
    }

    #[test]
    fn undo_goes_back_to_where_the_cursor_and_selection_were() {
        // Typing over a selection, then taking it back, selects the same text
        // again: the field is exactly as it was before the step.
        let mut b = buffer("ab");
        b.set_cursor(1, false);
        b.set_cursor(2, true);
        b.insert("-");
        assert_eq!(b.text(), "a-");
        assert!(b.undo());
        assert_eq!(b.text(), "ab");
        assert_eq!((b.cursor(), b.anchor()), (2, Some(1)));
    }

    #[test]
    fn the_app_filling_a_field_in_leaves_nothing_to_undo() {
        let mut b = buffer("");
        b.insert("typed");
        b.set_value("filled");
        assert!(!b.undo());
        b.insert("!");
        b.clear();
        assert!(!b.undo());
    }

    #[test]
    fn history_is_bounded() {
        let mut b = buffer("");
        for step in 0..(MAX_UNDO + 50) {
            b.insert(&format!("{step} "));
        }
        let mut undone = 0;
        while b.undo() {
            undone += 1;
        }
        assert_eq!(undone, MAX_UNDO);
    }
}
