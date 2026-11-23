//! The component kit: the parts every view is assembled from.
//!
//! A button, a chip, a checkbox, a card and a rail item each exist once, here.
//! They share hover, pressed, focus and disabled states, take their sizes and
//! washes from the tokens, and are keyboard-focusable, so a view can't
//! reintroduce a one-off control that behaves a little differently from every
//! other.
//!
//! None of them know about the app. They take a click listener in the form
//! `cx.listener` produces and tokens from the view.
//!
//! Every control can take focus and answers Enter and Space. Most are also
//! stops in the Tab order. The exception is a control that sits in every row
//! of a table: there a table is one stop, the arrow keys move inside it, and
//! a hundred rows must not mean three hundred presses.

use gpui::prelude::*;
use gpui::{
    ClickEvent, Div, ElementId, FocusHandle, Hsla, Role, SharedString, Stateful, Window, div, px,
};

use berth_ui::layout;
use berth_ui::theme::Tokens;

mod button;
mod card;
mod checkbox;
mod chip;
mod nav;

pub(crate) use button::{Button, button};
pub(crate) use card::{Card, caption, card, card_header, card_row, card_title, icon_chip};
pub(crate) use checkbox::{Checkbox, checkbox};
pub(crate) use chip::chip;
pub(crate) use nav::nav_item;

/// The click listener every control takes: what `cx.listener` returns.
pub(crate) type Click = std::rc::Rc<dyn Fn(&ClickEvent, &mut Window, &mut gpui::App)>;

/// A link: a row that leads to something else, such as a container or a
/// volume. It takes focus as a control does and is announced as what it is.
/// A view adds what the row shows, and where following it goes.
pub(crate) fn link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tokens: Tokens,
) -> Stateful<Div> {
    focusable(div().id(id), tokens.focus_ring())
        .role(Role::Link)
        .aria_label(label)
}

/// A table. It is one tab stop for all of its rows: a hundred rows must not
/// mean a hundred presses. The arrow keys move inside it, and the selected
/// row is what assistive technology reads out.
pub(crate) fn table(id: &'static str, label: &'static str, tokens: Tokens) -> Stateful<Div> {
    focusable(div().id(id), tokens.focus_ring())
        .rounded(px(layout::RADIUS_ROW))
        // The hairline that the focus ring is drawn in.
        .border_1()
        .border_color(tokens.invisible())
        .flex()
        .flex_col()
        .w_full()
        .role(Role::Table)
        .aria_label(label)
}

/// Give an element its keyboard focus ring and its place in the Tab order.
///
/// For the rare control that isn't one of the kit's, such as a sortable column
/// header. It keeps a hairline border for the ring to be drawn in.
pub(crate) fn focusable(element: Stateful<Div>, ring: Hsla) -> Stateful<Div> {
    focused(element, None, ring, true)
}

/// Give a control its focus ring, and say whether Tab reaches it.
///
/// `focus` is a handle the view keeps itself, as a sheet does to hold Tab
/// inside it; without one the element's id stands in, and the handle lives as
/// long as frames keep containing an element with that id.
///
/// A control that Tab skips keeps its focus handle all the same: a click or a
/// screen reader can still focus it, Enter and Space still press it, and Tab
/// still leaves from where it sits instead of from the top of the window.
pub(crate) fn focused(
    element: Stateful<Div>,
    focus: Option<&FocusHandle>,
    ring: Hsla,
    tab_stop: bool,
) -> Stateful<Div> {
    let element = match focus {
        Some(handle) => element.track_focus(&handle.clone().tab_stop(tab_stop)),
        None => element.tab_index(0).tab_stop(tab_stop),
    };
    element.focus_visible(move |style| style.border_color(ring))
}
