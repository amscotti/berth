//! Tests for the text field, in a window.

use gpui::{Modifiers, MouseButton, TestAppContext, point, px};

use super::InputState;
use crate::testkit;

/// A field on its own in a window, focused, holding `text`.
fn field<'a>(
    text: &str,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<InputState>, &'a mut gpui::VisualTestContext) {
    let (input, cx) = cx.add_window_view(|window, cx| {
        let state = InputState::new("Search containers", 190.0, cx);
        state.focus_handle.focus(window, cx);
        state
    });
    input.update(cx, |input, cx| input.set_value(text, cx));
    cx.run_until_parked();
    (input, cx)
}

/// Move focus on the way Tab does where nothing holds it, as the app's root
/// does. This window holds a field and nothing else, so there is nobody to
/// answer the key itself.
fn next_stop(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| window.focus_next(cx));
}

/// Where the field was drawn.
fn bounds(cx: &mut gpui::VisualTestContext) -> gpui::Bounds<gpui::Pixels> {
    cx.debug_bounds("input-Search containers")
        .expect("the field was laid out")
}

/// The x in the window where the text's byte `offset` is drawn: the field's
/// left edge, its border, its padding, then the shaped text.
fn x_of(cx: &mut gpui::VisualTestContext, text: &str, offset: usize) -> gpui::Pixels {
    let shaped =
        cx.update(|window, _| super::text::Line::new(text, gpui::black(), window).x_of(offset));
    bounds(cx).left() + px(1.0 + super::text::PAD + shaped)
}

fn text_of(input: &gpui::Entity<InputState>, cx: &mut gpui::VisualTestContext) -> String {
    input.read_with(cx, |input, _| input.text().to_owned())
}

fn has_focus(input: &gpui::Entity<InputState>, cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| input.read(cx).focus_handle().is_focused(window))
}

fn click(cx: &mut gpui::VisualTestContext, x: gpui::Pixels, y: gpui::Pixels) {
    let at = point(x, y);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
}

#[gpui::test]
fn a_click_puts_the_caret_where_the_character_is(cx: &mut TestAppContext) {
    let (input, cx) = field("hello world", cx);
    let y = bounds(cx).center().y;
    // A hair short of the end of the "o": the nearest boundary is after it.
    let x = x_of(cx, "hello world", 5);
    click(cx, x - px(1.0), y);
    assert_eq!(
        input.read_with(cx, |input, _| input.cursor()),
        5,
        "the caret goes after the fifth character"
    );
    // A hair past the start of the "w" is nearer than the "o" before the space.
    let x = x_of(cx, "hello world", 6);
    click(cx, x + px(3.0), y);
    assert_eq!(input.read_with(cx, |input, _| input.cursor()), 6);
}

#[gpui::test]
fn a_drag_selects_what_it_crosses(cx: &mut TestAppContext) {
    let (input, cx) = field("hello world", cx);
    let y = bounds(cx).center().y;
    let (start, end) = (x_of(cx, "hello world", 0), x_of(cx, "hello world", 5));
    cx.simulate_mouse_down(point(start, y), MouseButton::Left, Modifiers::default());
    assert_eq!(input.read_with(cx, |input, _| input.selection()), None);
    cx.simulate_mouse_move(point(end, y), None, Modifiers::default());
    assert_eq!(
        input.read_with(cx, |input, _| input.selection()),
        Some((0, 5)),
        "the selection follows the pointer"
    );
    // Letting go ends the drag: moving on changes nothing.
    cx.simulate_mouse_up(point(end, y), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(point(end + px(40.0), y), None, Modifiers::default());
    assert_eq!(
        input.read_with(cx, |input, _| input.selection()),
        Some((0, 5))
    );
}

#[gpui::test]
fn a_second_press_selects_the_word_under_it(cx: &mut TestAppContext) {
    let (input, cx) = field("one two three", cx);
    let (x, y) = (x_of(cx, "one two three", 6), bounds(cx).center().y);
    click(cx, x, y);
    // The platform counts the clicks; the test platform doesn't, so the
    // second press says so itself.
    cx.simulate_event(gpui::MouseDownEvent {
        position: point(x, y),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    assert_eq!(
        input.read_with(cx, |input, _| input.selection()),
        Some((4, 7))
    );
}

#[gpui::test]
fn the_caret_stays_in_view_however_long_the_text_is(cx: &mut TestAppContext) {
    let text = "a-very-long-container-name-nobody-will-read-to-the-end";
    let (input, cx) = field(text, cx);
    // The caret is at the end, further than a 190-point field shows.
    let scrolled = input.read_with(cx, |input, _| input.scroll());
    assert!(scrolled > 0.0, "the field scrolled to its end ({scrolled})");
    // Home brings the start back.
    cx.simulate_keystrokes("home");
    assert_eq!(input.read_with(cx, |input, _| input.scroll()), 0.0);
}

#[gpui::test]
fn undo_takes_back_a_word_and_redo_brings_it_back(cx: &mut TestAppContext) {
    let (input, cx) = field("", cx);
    cx.simulate_input("one two");
    assert_eq!(text_of(&input, cx), "one two");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text_of(&input, cx), "one ");
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(text_of(&input, cx), "one two");
}

#[gpui::test]
fn a_clear_button_comes_with_text_and_takes_it_away(cx: &mut TestAppContext) {
    let (input, cx) = field("", cx);
    // With nothing to clear the field is the only stop: moving on comes back.
    next_stop(cx);
    assert!(has_focus(&input, cx), "an empty field has no clear button");
    cx.simulate_input("nginx");
    // With text there is a stop after the field: its clear button.
    next_stop(cx);
    assert!(
        !has_focus(&input, cx),
        "Tab leaves the field for its clear button"
    );
    testkit::press(cx, "enter");
    assert_eq!(text_of(&input, cx), "");
    assert!(
        has_focus(&input, cx),
        "clearing leaves the field ready for the next thing typed"
    );
}

#[gpui::test]
fn the_platform_is_told_where_the_field_really_draws_a_character(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    // An IME candidate window is placed from these answers, so they have to
    // be where the text is drawn, to the pixel. The test platform has no
    // fonts and shapes every character to the same box, so this can't show a
    // wide "W" next to a narrow "i". It holds the field to reporting the text
    // system's own layout, which is what drawing uses.
    let (input, cx) = field("Wi", cx);
    let element = bounds(cx);
    cx.update(|window, cx| {
        let line = super::text::Line::new("Wi", gpui::black(), window);
        let mut reported = [None; 3];
        input.update(cx, |input, cx| {
            reported[0] = input.bounds_for_range(0..1, element, window, cx);
            reported[1] = input.bounds_for_range(1..2, element, window, cx);
            reported[2] = input.bounds_for_range(0..2, element, window, cx);
        });
        let [first, second, whole] = reported.map(|bounds| bounds.expect("the field answered"));
        assert_eq!(first.left(), element.left() + px(super::text::PAD));
        assert_eq!(second.left(), first.right());
        assert_eq!(whole.right() - whole.left(), px(line.width()));
        // Not the old guess, at so much per character.
        assert_ne!(f32::from(whole.size.width) / 2.0, InputState::FONT * 0.58);
    });
}

#[gpui::test]
fn typed_characters_reach_the_buffer(cx: &mut TestAppContext) {
    let (input, cx) = field("", cx);
    cx.simulate_input("nginx");
    assert_eq!(text_of(&input, cx), "nginx");
    cx.simulate_keystrokes("backspace");
    assert_eq!(text_of(&input, cx), "ngin");
    cx.simulate_keystrokes("left left");
    cx.simulate_input("X");
    assert_eq!(text_of(&input, cx), "ngXin");
}

#[gpui::test]
fn command_a_selects_and_typing_replaces(cx: &mut TestAppContext) {
    let (input, cx) = field("", cx);
    cx.simulate_input("alpine");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("nginx");
    assert_eq!(text_of(&input, cx), "nginx");
}

#[gpui::test]
fn a_field_is_painted_with_the_published_tokens(cx: &mut TestAppContext) {
    // An Appearance choice that differs from the system's is published…
    cx.update(|app| berth_ui::theme::ActiveTokens::set(berth_ui::theme::dark(), app));
    let (_input, _cx) = cx.add_window_view(|window, cx| {
        // …and the field follows it, not the window's own appearance.
        let tokens = berth_ui::theme::ActiveTokens::get(window, cx);
        assert_eq!(tokens, berth_ui::theme::dark());
        InputState::new("Search", 190.0, cx)
    });
}

#[gpui::test]
fn before_anything_is_published_a_field_follows_the_window(cx: &mut TestAppContext) {
    let (_input, _cx) = cx.add_window_view(|window, cx| {
        let tokens = berth_ui::theme::ActiveTokens::get(window, cx);
        assert_eq!(tokens, berth_ui::theme::for_appearance(window.appearance()));
        InputState::new("Search", 190.0, cx)
    });
}

#[gpui::test]
fn the_stages_of_a_composed_character_are_one_step_to_undo(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (input, cx) = field("", cx);
    // An IME writes a character a stroke at a time, rewriting as it goes.
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "n", None, window, cx);
            input.replace_and_mark_text_in_range(Some(0..1), "ni", None, window, cx);
            input.replace_and_mark_text_in_range(Some(0..2), "に", None, window, cx);
            input.unmark_text(window, cx);
        })
    });
    assert_eq!(text_of(&input, cx), "に");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        text_of(&input, cx),
        "",
        "the whole character came back out at once"
    );
}

#[gpui::test]
fn the_platform_reads_and_sets_the_selection_in_its_own_units(cx: &mut TestAppContext) {
    use gpui::{EntityInputHandler, UTF16Selection};
    let (input, cx) = field("aにb", cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            // UTF-16 counts the kana as one, where bytes count three.
            let mut adjusted = None;
            let read = input
                .text_for_range(1..2, &mut adjusted, window, cx)
                .expect("the field answered");
            assert_eq!(read, "に");
            assert_eq!(adjusted, Some(1..2));
            assert_eq!(input.text_length_utf16(window, cx), Some(3));
            assert_eq!(input.marked_text_range(window, cx), None);

            input.set_selected_text_range(1..2, window, cx);
            assert_eq!(
                input.selection(),
                Some((1, 4)),
                "in bytes, as the buffer keeps them"
            );
            let UTF16Selection { range, reversed } = input
                .selected_text_range(false, window, cx)
                .expect("the field answered");
            assert_eq!((range, reversed), (1..2, false));
            // A point over the "b" is the third character, in the platform's count.
            let from = input.drawn_from();
            let b_starts = super::text::Line::new("aにb", gpui::black(), window).x_of(4);
            let x = from + px(super::text::PAD) + px(b_starts + 1.0);
            assert_eq!(
                input.character_index_for_point(point(x, px(0.0)), window, cx),
                Some(3)
            );
        })
    });
}
