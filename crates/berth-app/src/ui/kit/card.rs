//! One card, and the pieces a card is made of.

use gpui::prelude::*;
use gpui::{
    AnyElement, Div, FontWeight, Hsla, IntoElement, SharedString, StyleRefinement, Styled, div, px,
};

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography;

use crate::ui::icon::icon;

/// A card: the surface a panel of the page is made of.
///
/// It is a container, so a view adds to it what it likes. Siblings in a
/// [`card_row`] stretch to the tallest of them and share one bottom edge.
pub(crate) struct Card(Div);

pub(crate) fn card(tokens: Tokens) -> Card {
    Card(
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(8.0))
            .p(px(layout::CARD_PADDING))
            .rounded(px(layout::CARD_RADIUS))
            .bg(tokens.surface)
            .border_1()
            .border_color(tokens.border),
    )
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        self.0.style()
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.0.extend(elements);
    }
}

impl IntoElement for Card {
    type Element = Div;

    fn into_element(self) -> Div {
        self.0
    }
}

/// A row of cards. Each is as tall as the tallest, so the row has one bottom
/// edge.
pub(crate) fn card_row() -> Div {
    div()
        .flex()
        .flex_row()
        .w_full()
        .items_stretch()
        .gap(px(layout::CARD_GAP))
}

/// A card's title.
pub(crate) fn card_title(text: impl Into<SharedString>, tokens: Tokens) -> Div {
    div()
        .text_size(px(typography::TITLE_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.text)
        .child(text.into())
}

/// A card's first line: its title, then what belongs beside it, at the end.
pub(crate) fn card_header(
    title: impl Into<SharedString>,
    end: Option<AnyElement>,
    tokens: Tokens,
) -> Div {
    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .gap(px(8.0))
        .child(card_title(title, tokens))
        .child(div().flex_1());
    if let Some(end) = end {
        header = header.child(end);
    }
    header
}

/// Secondary text: the body of a card, a caption under a figure.
pub(crate) fn caption(text: impl Into<SharedString>, tokens: Tokens) -> Div {
    div()
        .text_size(px(typography::SECONDARY))
        .text_color(tokens.muted)
        .child(text.into())
}

/// The square of tint behind an icon, in a rail item or beside a card's title.
pub(crate) fn icon_chip(
    name: &'static str,
    glyph: f32,
    tint: Hsla,
    tokens: Tokens,
    size: f32,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .rounded(px(layout::RADIUS_CHIP))
        .bg(tokens.tint_wash(tint))
        .child(icon(name, glyph, tint))
}
