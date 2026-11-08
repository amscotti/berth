//! A single-line text buffer: a value, a byte cursor, and an optional anchor.
//! Every offset is kept on a UTF-8 boundary; externally supplied offsets are
//! snapped before use.

/// The editing state behind one text field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextBuffer {
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
            anchor: None,
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

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.value.len();
        self.anchor = None;
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.anchor = None;
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

    pub fn insert(&mut self, text: &str) {
        self.delete_selection();
        let at = self.cursor.min(self.value.len());
        self.value.insert_str(at, text);
        self.cursor = at + text.len();
        self.anchor = None;
    }

    pub fn backspace(&mut self) {
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
        if !extend && let Some((start, _)) = self.selection() {
            self.set_cursor(start, false);
            return;
        }
        self.set_cursor(self.prev_boundary(self.cursor), extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend && let Some((_, end)) = self.selection() {
            self.set_cursor(end, false);
            return;
        }
        self.set_cursor(self.next_boundary(self.cursor), extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
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
        self.set_cursor(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.set_cursor(self.value.len(), extend);
    }

    /// Cursor offset for a click at `x` within a field whose text starts at
    /// `origin_x` and uses `approx_char_width`.
    pub fn offset_for_x(&self, x: f32, origin_x: f32, approx_char_width: f32) -> usize {
        if approx_char_width <= 0.0 {
            return self.cursor;
        }
        let index = (((x - origin_x) / approx_char_width).round()).max(0.0) as usize;
        let mut offset = 0;
        for (count, (byte, _)) in self.value.char_indices().enumerate() {
            if count == index {
                offset = byte;
                break;
            }
            offset = byte + self.value[byte..].chars().next().map_or(0, char::len_utf8);
        }
        offset.min(self.value.len())
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
    fn click_offset_maps_to_a_boundary() {
        let b = buffer("abcd");
        assert_eq!(b.offset_for_x(0.0, 0.0, 8.0), 0);
        assert_eq!(b.offset_for_x(17.0, 0.0, 8.0), 2);
        assert_eq!(b.offset_for_x(999.0, 0.0, 8.0), 4);
    }
}
