//! One button.

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, FocusHandle, FontWeight, IntoElement, Pixels, Role,
    SharedString, Stateful, Window, div, px,
};

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography;

use super::Click;
use crate::ui::icon::icon;
use crate::ui::tabular;

/// What a button says about itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    /// The one action a person is most likely to take here.
    Primary,
    /// An ordinary action.
    Secondary,
    /// There when wanted and quiet otherwise: Refresh, Copy.
    Ghost,
    /// An action that destroys, filled so that it can't be taken for the way
    /// forward.
    Danger,
    /// An action that destroys, drawn quiet because it sits somewhere crowded,
    /// such as a table row.
    DangerWash,
    /// A link in the colours of what it opens: a published port.
    Port,
}

impl Variant {
    /// The fill, the ink and the focus ring.
    fn palette(self, tokens: Tokens) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
        match self {
            Variant::Primary => (
                tokens.accent,
                tokens.accent_text,
                tokens.focus_ring_on_fill(),
            ),
            Variant::Danger => (
                tokens.danger,
                tokens.accent_text,
                tokens.focus_ring_on_fill(),
            ),
            Variant::Secondary => (tokens.surface_hover, tokens.text, tokens.focus_ring()),
            Variant::Ghost => (tokens.invisible(), tokens.muted, tokens.focus_ring()),
            Variant::DangerWash => (
                tokens.danger_wash(),
                tokens.danger_text(),
                tokens.focus_ring(),
            ),
            Variant::Port => (
                tokens.pill_wash(tokens.warning_fill),
                tokens.warning_text(),
                tokens.focus_ring(),
            ),
        }
    }
}

/// A button. Every button in the app is one of these.
#[derive(Clone)]
pub(crate) struct Button {
    id: ElementId,
    label: SharedString,
    /// What assistive technology calls it. The label, unless set.
    name: Option<SharedString>,
    tokens: Tokens,
    variant: Variant,
    small: bool,
    width: Option<Pixels>,
    icon: Option<&'static str>,
    /// The icon is the whole button, and the label is only its name: a
    /// close, in a crowded header.
    icon_only: bool,
    /// Figures that line up in a column: a port number.
    tabular: bool,
    enabled: bool,
    tab_stop: bool,
    focus: Option<FocusHandle>,
    description: Option<SharedString>,
    on_click: Option<Click>,
}

/// A button of the usual size that does an ordinary thing.
pub(crate) fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tokens: Tokens,
) -> Button {
    Button {
        id: id.into(),
        label: label.into(),
        name: None,
        tokens,
        variant: Variant::Secondary,
        small: false,
        width: None,
        icon: None,
        icon_only: false,
        tabular: false,
        enabled: true,
        tab_stop: true,
        focus: None,
        description: None,
        on_click: None,
    }
}

impl Button {
    /// The one action a person is most likely to take here.
    pub(crate) fn primary(mut self) -> Self {
        self.variant = Variant::Primary;
        self
    }

    /// There when wanted and quiet otherwise.
    pub(crate) fn ghost(mut self) -> Self {
        self.variant = Variant::Ghost;
        self
    }

    /// An action that destroys.
    pub(crate) fn danger(mut self) -> Self {
        self.variant = Variant::Danger;
        self
    }

    /// An action that destroys, drawn quiet.
    pub(crate) fn danger_wash(mut self) -> Self {
        self.variant = Variant::DangerWash;
        self
    }

    /// A link to a published port, in the port colours.
    pub(crate) fn port(mut self) -> Self {
        self.variant = Variant::Port;
        self
    }

    /// The 24-point size, for somewhere crowded.
    pub(crate) fn small(mut self) -> Self {
        self.small = true;
        self
    }

    /// A fixed width, so that a dialog's buttons share one shape.
    pub(crate) fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// An icon before the label.
    pub(crate) fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    /// The icon alone, in a square button. The label then isn't shown, so it
    /// exists for assistive technology: say what the button does.
    pub(crate) fn icon_only(mut self) -> Self {
        self.icon_only = true;
        self
    }

    /// Set the label in tabular figures, so numbers line up.
    pub(crate) fn tabular(mut self) -> Self {
        self.tabular = true;
        self
    }

    /// What assistive technology calls the button, when its label alone
    /// doesn't say what pressing it does: the label is "8080", the name is
    /// "Open port 8080".
    pub(crate) fn name(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// A disabled button can't be pressed, isn't reached by Tab, and says why.
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.enabled = !disabled;
        self
    }

    /// Why the button can't be used, for assistive technology.
    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Whether Tab stops here. Off for a control that repeats in every row of
    /// a table, where the table itself is the stop.
    pub(crate) fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Focus through a handle the view keeps, as a sheet does to hold Tab
    /// inside itself.
    pub(crate) fn focus(mut self, focus: FocusHandle) -> Self {
        self.focus = Some(focus);
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

impl IntoElement for Button {
    type Element = Stateful<Div>;

    fn into_element(self) -> Stateful<Div> {
        let tokens = self.tokens;
        let variant = self.variant;
        let (fill, ink, ring) = variant.palette(tokens);
        let height = if self.small {
            layout::CONTROL_SMALL
        } else {
            layout::CONTROL
        };

        // Every button keeps a hairline, so that the focus ring, or a wash's
        // hover, can be drawn in it without moving anything. A secondary
        // button keeps a visible one: its fill is close to the tinted surfaces
        // it can sit on, such as an active engine's card, and the edge is
        // what keeps it a button there.
        let edge = if variant == Variant::Secondary {
            tokens.border
        } else {
            tokens.invisible()
        };

        let mut button = div()
            .id(self.id)
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .h(px(height))
            .rounded(px(height / 2.0))
            .border_1()
            .border_color(edge)
            .bg(fill)
            .text_size(px(if self.small {
                typography::LABEL
            } else {
                typography::BODY
            }))
            .font_weight(FontWeight::MEDIUM)
            .when(self.tabular, |this| this.font_features(tabular()))
            .text_color(ink)
            .role(Role::Button)
            .aria_label(self.name.clone().unwrap_or_else(|| self.label.clone()));

        // What the button shows: an icon alone in a square, or a label with
        // an icon before it.
        button = if self.icon_only {
            button.w(px(height)).flex_shrink_0().child(icon(
                self.icon.unwrap_or("xmark"),
                height - 10.0,
                ink,
            ))
        } else {
            button = match self.width {
                Some(width) => button.w(width),
                None => button.px(px(if self.small { 9.0 } else { 13.0 })),
            };
            if let Some(name) = self.icon {
                button = button.child(icon(name, 12.0, ink));
            }
            button.child(div().child(self.label.clone()))
        };

        if self.enabled {
            // How hover and being pressed show. A filled button moves its
            // fill toward the text colour, which is what a person expects and
            // which its text has contrast to spare for. An ink on a wash shows
            // an outline in the ink instead: its fill doesn't move, so its
            // contrast doesn't either. A ghost has nothing to outline, so its
            // label strengthens, and a press shades it.
            button = match variant {
                Variant::Primary => {
                    // The palette names the primary button's hover itself.
                    let hovered = tokens.accent_hover;
                    let pressed = tokens.pressed(fill);
                    button
                        .hover(move |style| style.bg(hovered))
                        .active(move |style| style.bg(pressed))
                }
                Variant::Secondary | Variant::Danger => {
                    let hovered = tokens.hovered(fill);
                    let pressed = tokens.pressed(fill);
                    button
                        .hover(move |style| style.bg(hovered))
                        .active(move |style| style.bg(pressed))
                }
                Variant::Ghost => {
                    let text = tokens.text;
                    let pressed = tokens.pressed(tokens.surface);
                    button
                        .hover(move |style| style.text_color(text))
                        .active(move |style| style.bg(pressed).text_color(text))
                }
                Variant::DangerWash | Variant::Port => {
                    let outline = tokens.hover_outline(ink);
                    button
                        .hover(move |style| style.border_color(outline))
                        .active(move |style| style.border_color(ink))
                }
            };
            if let Some(on_click) = self.on_click {
                button = button.on_click(move |event, window, cx| on_click(event, window, cx));
            }
        } else {
            button = button.opacity(tokens.disabled_opacity()).aria_description(
                self.description
                    .unwrap_or_else(|| "Unavailable".to_owned().into()),
            );
        }
        super::focused(
            button,
            self.focus.as_ref(),
            ring,
            self.enabled && self.tab_stop,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a button's ink is read: on its own fill, or, with no fill of
    /// its own, on the page and on a card.
    fn grounds(variant: Variant, tokens: Tokens) -> Vec<(&'static str, gpui::Hsla)> {
        let (fill, _, _) = variant.palette(tokens);
        match variant {
            Variant::Ghost => vec![
                ("on the page", tokens.background),
                ("on a card", tokens.surface),
            ],
            _ => vec![("at rest", fill)],
        }
    }

    /// What a person reads on a button, in each of its states, in both themes.
    #[test]
    fn every_button_stays_legible_in_every_state_and_theme() {
        // 4.5:1 is the WCAG floor for text this size.
        for appearance in [gpui::WindowAppearance::Light, gpui::WindowAppearance::Dark] {
            let tokens = berth_ui::theme::for_appearance(appearance);
            for variant in [
                Variant::Primary,
                Variant::Secondary,
                Variant::Ghost,
                Variant::Danger,
                Variant::DangerWash,
                Variant::Port,
            ] {
                let (_, ink, _) = variant.palette(tokens);
                let (fill, _, _) = variant.palette(tokens);
                let mut states = grounds(variant, tokens);
                // Only a filled button moves its fill under the pointer.
                match variant {
                    Variant::Primary => {
                        states.push(("hovered", tokens.accent_hover));
                        states.push(("pressed", tokens.pressed(fill)));
                    }
                    Variant::Secondary | Variant::Danger => {
                        states.push(("hovered", tokens.hovered(fill)));
                        states.push(("pressed", tokens.pressed(fill)));
                    }
                    _ => {}
                }
                for (state, over) in states {
                    let ratio = berth_ui::theme::contrast_ratio(over, ink);
                    assert!(
                        ratio >= 4.5,
                        "{variant:?} {state}, {:?}: {ratio:.2}:1",
                        tokens.theme
                    );
                }
            }
        }
    }

    /// A focus ring has to be seen: 3:1 against whatever it surrounds.
    #[test]
    fn the_ring_shows_against_what_it_rings() {
        for appearance in [gpui::WindowAppearance::Light, gpui::WindowAppearance::Dark] {
            let tokens = berth_ui::theme::for_appearance(appearance);
            for variant in [
                Variant::Primary,
                Variant::Secondary,
                Variant::Ghost,
                Variant::Danger,
                Variant::DangerWash,
                Variant::Port,
            ] {
                let (_, _, ring) = variant.palette(tokens);
                for (what, over) in grounds(variant, tokens) {
                    let ratio = berth_ui::theme::contrast_ratio(ring, over);
                    assert!(
                        ratio >= 3.0,
                        "{variant:?} ring {what}, {:?}: {ratio:.2}:1",
                        tokens.theme
                    );
                }
            }
        }
    }
}
