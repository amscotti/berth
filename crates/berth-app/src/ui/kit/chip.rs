//! One chip: a filter, one segment of a choice, a tab, or a toggle.

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, Hsla, IntoElement, Role, SharedString, Stateful,
    Toggled, Window, div, px,
};

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography;

use super::Click;

/// What a chip is to assistive technology.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kind {
    /// One of a set where one is on at a time: a filter, an appearance.
    #[default]
    OneOf,
    /// A page of a tab strip.
    Tab,
    /// On or off on its own: Follow, Times.
    Toggle,
}

/// A chip. On or off, never in between.
///
/// The chip that is on is the one that looks switched on: a wash of the accent
/// with the accent's own text colour, and its label a little heavier. The
/// others are plain surfaces with a hairline, quieter than the one that is on
/// and brighter than nothing.
#[derive(Clone)]
pub(crate) struct Chip {
    id: ElementId,
    label: SharedString,
    tokens: Tokens,
    on: bool,
    kind: Kind,
    /// What the chip sits on, when it isn't the page: a card's `surface`.
    over: Option<Hsla>,
    on_click: Option<Click>,
}

pub(crate) fn chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    tokens: Tokens,
) -> Chip {
    Chip {
        id: id.into(),
        label: label.into(),
        tokens,
        on,
        kind: Kind::OneOf,
        over: None,
        on_click: None,
    }
}

impl Chip {
    pub(crate) fn on_click(
        mut self,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(std::rc::Rc::new(on_click));
        self
    }

    /// A tab: one page of a tab strip, announced as selected.
    pub(crate) fn tab(mut self) -> Self {
        self.kind = Kind::Tab;
        self
    }

    /// A toggle, such as Follow or Times: on or off, announced as such.
    pub(crate) fn toggle(mut self) -> Self {
        self.kind = Kind::Toggle;
        self
    }

    /// The chip sits on `over` rather than on the page.
    pub(crate) fn on_surface(mut self, over: Hsla) -> Self {
        self.over = Some(over);
        self
    }
}

impl IntoElement for Chip {
    type Element = Stateful<Div>;

    fn into_element(self) -> Stateful<Div> {
        let tokens = self.tokens;
        let (fill, ink) = if self.on {
            let wash = match self.over {
                Some(over) => tokens.accent_wash_on(over),
                None => tokens.accent_wash(),
            };
            (wash, tokens.accent_text_pill())
        } else {
            (tokens.surface, tokens.text)
        };

        let mut chip = div()
            .id(self.id)
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .h(px(layout::CONTROL_SMALL))
            .px(px(10.0))
            .rounded(px(layout::CONTROL_SMALL / 2.0))
            .border_1()
            .border_color(if self.on {
                // The hairline that the focus ring is drawn in.
                tokens.invisible()
            } else {
                tokens.border
            })
            .bg(fill)
            .text_size(px(typography::LABEL))
            .font_weight(if self.on {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .text_color(ink)
            .role(match self.kind {
                Kind::Tab => Role::Tab,
                Kind::OneOf => Role::RadioButton,
                Kind::Toggle => Role::Button,
            })
            .aria_label(self.label.clone())
            .child(div().child(self.label.clone()));
        chip = match self.kind {
            Kind::Tab => chip.aria_selected(self.on),
            Kind::OneOf | Kind::Toggle => chip.aria_toggled(if self.on {
                Toggled::True
            } else {
                Toggled::False
            }),
        };

        chip = if self.on {
            // An ink on its wash: hover is an outline, so the fill holds
            // still and so does the text's contrast.
            let outline = tokens.hover_outline(ink);
            chip.hover(move |style| style.border_color(outline))
                .active(move |style| style.border_color(ink))
        } else {
            let hovered = tokens.surface_hover;
            let pressed = tokens.pressed(tokens.surface);
            chip.hover(move |style| style.bg(hovered))
                .active(move |style| style.bg(pressed))
        };
        if let Some(on_click) = self.on_click {
            chip = chip.on_click(move |event, window, cx| on_click(event, window, cx));
        }
        super::focusable(chip, tokens.focus_ring())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_chip_is_legible_on_and_off_in_both_themes() {
        // 4.5:1 is the WCAG floor for text this size.
        for tokens in [
            berth_ui::theme::for_appearance(gpui::WindowAppearance::Light),
            berth_ui::theme::for_appearance(gpui::WindowAppearance::Dark),
        ] {
            let cases = [
                (
                    "on, on the page",
                    tokens.accent_wash(),
                    tokens.accent_text_pill(),
                ),
                (
                    "on, on a card",
                    tokens.accent_wash_on(tokens.surface),
                    tokens.accent_text_pill(),
                ),
                ("off", tokens.surface, tokens.text),
                ("off, hovered", tokens.surface_hover, tokens.text),
                ("off, pressed", tokens.pressed(tokens.surface), tokens.text),
            ];
            for (what, over, ink) in cases {
                let ratio = berth_ui::theme::contrast_ratio(over, ink);
                assert!(ratio >= 4.5, "{:?} {what}: {ratio:.2}:1", tokens.theme);
            }
        }
    }
}
