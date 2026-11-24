//! One item of the rail.

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, Hsla, IntoElement, Role, SharedString, Stateful,
    Window, div, px,
};

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography;

use super::Click;
use crate::ui::kit::icon_chip;
use crate::ui::tabular;

/// An item of the rail: where it leads, and what is there.
///
/// The item that is selected is filled with the accent: of everything on
/// screen it is the one thing that must be unmistakable. The others are
/// plain, and quiet enough to leave the page alone.
///
/// To assistive technology the rail is a tab strip: the items are the app's
/// pages, and exactly one is selected.
#[derive(Clone)]
pub(crate) struct NavItem {
    id: ElementId,
    label: SharedString,
    icon: &'static str,
    tint: Hsla,
    tokens: Tokens,
    selected: bool,
    /// An icon only, for a window too narrow for labels.
    compact: bool,
    /// What is there, such as "2/2" or "103 MiB".
    figure: Option<SharedString>,
    on_click: Option<Click>,
}

pub(crate) fn nav_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: &'static str,
    tint: Hsla,
    selected: bool,
    tokens: Tokens,
) -> NavItem {
    NavItem {
        id: id.into(),
        label: label.into(),
        icon,
        tint,
        tokens,
        selected,
        compact: false,
        figure: None,
        on_click: None,
    }
}

impl NavItem {
    /// What is there.
    pub(crate) fn figure(mut self, figure: impl Into<SharedString>) -> Self {
        self.figure = Some(figure.into());
        self
    }

    /// An icon only. The label and the figure are still announced.
    pub(crate) fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub(crate) fn on_click(
        mut self,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(std::rc::Rc::new(on_click));
        self
    }
}

impl IntoElement for NavItem {
    type Element = Stateful<Div>;

    fn into_element(self) -> Stateful<Div> {
        let tokens = self.tokens;
        let selected = self.selected;
        let (fill, ink, glyph) = if selected {
            (tokens.accent, tokens.accent_text, tokens.accent_text)
        } else {
            (tokens.surface, tokens.text, self.tint)
        };
        let (height, chip_size, glyph_size) = if self.compact {
            (layout::RAIL_ITEM_COMPACT, layout::CONTROL_SMALL, 13.0)
        } else {
            (layout::RAIL_ITEM, layout::CHIP, 12.0)
        };
        let ring = if selected {
            tokens.focus_ring_on_fill()
        } else {
            tokens.focus_ring()
        };

        let name = match &self.figure {
            Some(figure) => format!("{} · {figure}", self.label).into(),
            None => self.label.clone(),
        };
        // With the icon only, the figure would be invisible: it is in the
        // name instead, where it is still read out.
        let figure = self.figure.filter(|_| !self.compact);

        let mut item = div()
            .id(self.id)
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(height))
            .rounded(px(layout::RADIUS_ITEM))
            // The hairline that the focus ring is drawn in.
            .border_1()
            .border_color(tokens.invisible())
            .bg(fill)
            .role(Role::Tab)
            .aria_selected(selected)
            .aria_label(name)
            .child(icon_chip(self.icon, glyph_size, glyph, tokens, chip_size));
        if self.compact {
            item = item.justify_center();
        } else {
            item = item
                .gap(px(8.0))
                .px(px(6.0))
                .child(
                    div()
                        .text_size(px(typography::BODY))
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(ink)
                        .child(self.label.clone()),
                )
                .child(div().flex_1())
                .when_some(figure, |this, figure| {
                    this.child(
                        div()
                            .text_size(px(typography::LABEL))
                            .font_features(tabular())
                            .text_color(if selected {
                                tokens.accent_text
                            } else {
                                tokens.muted
                            })
                            .child(figure),
                    )
                });
        }
        if !selected {
            // The selected item doesn't change under the pointer: there is
            // nothing left to tell the person about it.
            let hovered = tokens.surface_hover;
            let pressed = tokens.pressed(tokens.surface);
            item = item
                .hover(move |style| style.bg(hovered))
                .active(move |style| style.bg(pressed));
        }
        if let Some(on_click) = self.on_click {
            item = item.on_click(move |event, window, cx| on_click(event, window, cx));
        }
        super::focused(item, None, ring, true)
    }
}
