//! One checkbox.

use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Div, ElementId, FocusHandle, FontWeight, IntoElement, Role, SharedString,
    Stateful, Toggled, Window, div, px,
};

use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography;

use super::Click;
use crate::ui::icon::icon;
use crate::ui::tabular;

/// A checkbox: one option of a set, with its label and room for a figure.
///
/// The whole row is the control. There is far more to aim at than a 14-point
/// box, and the label is part of what assistive technology reads out.
#[derive(Clone)]
pub(crate) struct Checkbox {
    id: ElementId,
    label: SharedString,
    tokens: Tokens,
    checked: bool,
    /// What this option amounts to, such as "13.0 MiB", at the end of the row.
    figure: Option<SharedString>,
    /// Only the box is drawn: for a column of a table, where the row names it.
    box_only: bool,
    /// Whether Tab stops here.
    tab_stop: bool,
    focus: Option<FocusHandle>,
    on_click: Option<Click>,
}

pub(crate) fn checkbox(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    tokens: Tokens,
) -> Checkbox {
    Checkbox {
        id: id.into(),
        label: label.into(),
        tokens,
        checked,
        figure: None,
        box_only: false,
        tab_stop: true,
        focus: None,
        on_click: None,
    }
}

impl Checkbox {
    /// What this option amounts to, shown at the end of the row.
    pub(crate) fn figure(mut self, figure: impl Into<SharedString>) -> Self {
        self.figure = Some(figure.into());
        self
    }

    /// Draw only the box.
    pub(crate) fn box_only(mut self) -> Self {
        self.box_only = true;
        self
    }

    /// Whether Tab stops here.
    pub(crate) fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Focus through a handle the view keeps.
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

impl IntoElement for Checkbox {
    type Element = Stateful<Div>;

    fn into_element(self) -> Stateful<Div> {
        let tokens = self.tokens;
        let height = if self.box_only {
            // A column of the table: the whole row's height is the target.
            layout::ROW_CONTAINER
        } else {
            layout::CONTROL
        };
        let mut row = div()
            .id(self.id)
            .flex()
            .flex_row()
            .items_center()
            .h(px(height))
            .gap(px(8.0))
            // A column of the table centres its box; an option on its own
            // takes the line and leaves room for its label.
            .when(self.box_only, |this| {
                this.w(px(layout::BULK_COLUMN)).justify_center()
            })
            .when(!self.box_only, |this| this.w_full().px(px(4.0)))
            .rounded(px(layout::RADIUS_ROW))
            // The hairline that the focus ring is drawn in.
            .border_1()
            .border_color(tokens.invisible())
            .role(Role::CheckBox)
            .aria_toggled(if self.checked {
                Toggled::True
            } else {
                Toggled::False
            })
            // What the option amounts to is part of the control, so it is
            // part of what the control is called.
            .aria_label(match &self.figure {
                Some(figure) if !self.box_only => format!("{}, {figure}", self.label).into(),
                _ => self.label.clone(),
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(if self.checked {
                        tokens.accent
                    } else {
                        tokens.checkbox_border()
                    })
                    .bg(if self.checked {
                        tokens.accent
                    } else {
                        tokens.track
                    })
                    // A drawn check, not a text glyph: the glyph is whatever
                    // the font has, at whatever weight, and in no colour of
                    // its own.
                    .when(self.checked, |this| {
                        this.child(icon("check", 11.0, tokens.accent_text))
                    }),
            );
        if !self.box_only {
            row = row
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(typography::BODY))
                        .text_color(tokens.text)
                        .child(self.label.clone()),
                )
                .when_some(self.figure, |this, figure| {
                    this.child(
                        div()
                            .text_size(px(typography::LABEL))
                            .font_weight(FontWeight::MEDIUM)
                            .font_features(tabular())
                            .text_color(tokens.muted)
                            .child(figure),
                    )
                });
        }
        if let Some(on_click) = self.on_click {
            row = row.on_click(move |event, window, cx| on_click(event, window, cx));
        }
        let hovered = tokens.surface_hover;
        super::focused(
            row.hover(move |style| style.bg(hovered)),
            self.focus.as_ref(),
            tokens.focus_ring(),
            self.tab_stop,
        )
    }
}
