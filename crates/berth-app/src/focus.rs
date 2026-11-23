//! Keyboard focus the app keeps itself: the page's, and the hold a sheet
//! takes on it.
//!
//! A sheet is modal. While one is open it owns the keyboard: focus moves into
//! it, Tab crosses only its controls, and the page behind gets no keystroke
//! but the ones the shell itself answers, such as Escape. When the sheet
//! closes, focus goes back to where it was.

use gpui::{App, Context, FocusHandle, Window};

use crate::app::BerthApp;
use crate::app_forms::PRUNE_OPTIONS;

/// The controls a sheet is made of, each with a focus handle of its own.
///
/// The handles live here rather than on the elements because Tab inside a
/// sheet has to be answered from a list, not from the window's: the page
/// behind the sheet is still on screen, and still full of tab stops.
#[derive(Clone)]
pub(crate) struct SheetFocus {
    pub(crate) cancel: FocusHandle,
    pub(crate) confirm: FocusHandle,
    pub(crate) create: FocusHandle,
    pub(crate) clean_up: FocusHandle,
    /// The kinds of data a cleanup can remove, in the order the sheet lists
    /// them.
    pub(crate) options: [FocusHandle; PRUNE_OPTIONS],
    /// Where focus was when the sheet opened.
    before: Option<FocusHandle>,
}

impl SheetFocus {
    pub(crate) fn new(cx: &mut Context<BerthApp>) -> Self {
        Self {
            cancel: cx.focus_handle().tab_stop(true),
            confirm: cx.focus_handle().tab_stop(true),
            create: cx.focus_handle().tab_stop(true),
            clean_up: cx.focus_handle().tab_stop(true),
            options: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            before: None,
        }
    }

    /// The open sheet's controls, in the order Tab crosses them and only
    /// those that can be used. Empty when no sheet is open.
    pub(crate) fn stops(&self, app: &BerthApp, cx: &App) -> Vec<FocusHandle> {
        if app.pending.is_some() {
            // Confirm waits for a running mutation; while it does, Tab
            // passes it by.
            let mut stops = vec![self.cancel.clone()];
            if !app.mutating() {
                stops.push(self.confirm.clone());
            }
            return stops;
        }
        if app.create_kind.is_some() {
            return vec![
                app.create_name.read(cx).focus_handle(),
                self.cancel.clone(),
                self.create.clone(),
            ];
        }
        if app.prune_open {
            let offered = if app.prune_build_supported() {
                PRUNE_OPTIONS
            } else {
                PRUNE_OPTIONS - 1
            };
            let mut stops: Vec<FocusHandle> = self.options.iter().take(offered).cloned().collect();
            stops.push(self.cancel.clone());
            // While berth can see there is nothing to clean up, the button
            // can't be pressed, and Tab passes it by.
            if app.prune_has_work() != Some(false) {
                stops.push(self.clean_up.clone());
            }
            return stops;
        }
        Vec::new()
    }

    /// Whether focus is on one of the open sheet's controls.
    #[cfg(test)]
    pub(crate) fn holds_focus(&self, app: &BerthApp, cx: &App, window: &Window) -> bool {
        self.stops(app, cx)
            .iter()
            .any(|handle| handle.is_focused(window))
    }

    /// Whether focus is on one of these controls.
    fn holds(&self, stops: &[FocusHandle], window: &Window) -> bool {
        stops.iter().any(|handle| handle.is_focused(window))
    }
}

impl BerthApp {
    /// Put keyboard focus where it belongs, before the frame is drawn: in the
    /// sheet while one is open, back on the page when it has closed.
    pub(crate) fn place_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let stops = self.sheet_focus.stops(self, cx);
        if !stops.is_empty() {
            if !self.sheet_focus.holds(&stops, window) {
                // The sheet has just opened. Remember where focus was, then
                // move it to the sheet's first control, which for a
                // confirmation is Cancel: the way out, not the way forward.
                if self.sheet_focus.before.is_none() {
                    self.sheet_focus.before = Some(
                        window
                            .focused(cx)
                            .unwrap_or_else(|| self.focus_handle.clone()),
                    );
                }
                if let Some(first) = stops.first() {
                    first.focus(window, cx);
                }
            }
        } else if let Some(before) = self.sheet_focus.before.take() {
            // Back to where it was. If that control is gone the window drops
            // the focus to what contains it, and the line below catches a
            // window left with nothing focused.
            before.focus(window, cx);
        }
        if window.focused(cx).is_none() {
            self.focus_handle.focus(window, cx);
        }
    }

    /// Tab, or Shift+Tab when `forward` is false. Inside a sheet this crosses
    /// only the sheet's controls, wrapping at each end; elsewhere it is the
    /// window's own order.
    pub(crate) fn move_focus(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stops = self.sheet_focus.stops(self, cx);
        let Some(last) = stops.len().checked_sub(1) else {
            if forward {
                window.focus_next(cx);
            } else {
                window.focus_prev(cx);
            }
            return;
        };
        let at = stops.iter().position(|handle| handle.is_focused(window));
        let next = match (at, forward) {
            (None, true) => 0,
            (None, false) => last,
            (Some(index), true) => (index + 1) % stops.len(),
            (Some(index), false) => (index + last) % stops.len(),
        };
        if let Some(handle) = stops.get(next) {
            handle.focus(window, cx);
        }
    }
}
