//! The two corners of the window, each a view of its own: the page (the rail
//! and whatever page is showing) and the inspector.
//!
//! GPUI redraws a view that was notified, and its ancestors, but not its
//! siblings. So an edit in a search field can redraw the page without
//! shaping the inspector's logs again, and a log poll can redraw the
//! inspector without laying the table out again.
//!
//! Both ask the shell to draw them (`BerthApp::render_page` and
//! `BerthApp::render_inspector`), so everything that draws a page or a pane
//! still has the shell in hand. Both also watch the shell: a change the
//! shell announces with `cx.notify()` reaches both, exactly as when the
//! shell drew everything itself. A change only one corner shows is announced
//! with [`BerthApp::redraw_page`] or [`BerthApp::redraw_inspector`] instead,
//! and the other corner is left as it is.

use gpui::{App, Context, Entity, IntoElement, Render, Subscription, WeakEntity, Window, div};

use crate::app::BerthApp;

/// Which corner of the window a [`Subview`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Corner {
    /// The rail and the main column.
    Page,
    /// What is open in the inspector.
    Inspector,
}

pub(crate) struct Subview {
    shell: WeakEntity<BerthApp>,
    corner: Corner,
    /// The shell's own changes, which this view must redraw for.
    _watch: Option<Subscription>,
    /// How many times this view has been drawn. Tests read it, to hold each
    /// change to redrawing only what shows it.
    #[cfg(test)]
    pub(crate) draws: u32,
}

impl Subview {
    fn new(shell: WeakEntity<BerthApp>, corner: Corner) -> Self {
        Self {
            shell,
            corner,
            _watch: None,
            #[cfg(test)]
            draws: 0,
        }
    }

    /// The page.
    pub(crate) fn page(shell: WeakEntity<BerthApp>) -> Self {
        Self::new(shell, Corner::Page)
    }

    /// The inspector.
    pub(crate) fn inspector(shell: WeakEntity<BerthApp>) -> Self {
        Self::new(shell, Corner::Inspector)
    }

    /// Redraw whenever the shell does. Called once the shell exists, which
    /// is after this view was made.
    pub(crate) fn watch(&mut self, shell: &Entity<BerthApp>, cx: &mut Context<Self>) {
        self._watch = Some(cx.observe(shell, |_, _, cx| cx.notify()));
    }
}

impl Render for Subview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.draws += 1;
        }
        let viewport = f32::from(window.viewport_size().width);
        let Some(shell) = self.shell.upgrade() else {
            return div().into_any_element();
        };
        let corner = self.corner;
        let mut drawn = div().into_any_element();
        shell.update(cx, |app, cx| {
            drawn = match corner {
                Corner::Page => app.render_page(viewport, cx),
                // Nothing selected: nothing to show, and no room taken.
                Corner::Inspector => app
                    .render_inspector(viewport, cx)
                    .unwrap_or_else(|| div().into_any_element()),
            };
        });
        drawn
    }
}

impl BerthApp {
    /// Redraw the page alone, for a change that only the page shows: what
    /// was typed into a search field, the sort, the bulk selection. The
    /// inspector draws none of it, so its logs are not shaped again.
    ///
    /// Anything else is `cx.notify()`, which reaches both corners.
    pub(crate) fn redraw_page(&self, cx: &mut App) {
        self.page.update(cx, |_, cx| cx.notify());
    }

    /// Redraw the inspector alone, for a change that only it shows: its
    /// logs, its inspect document, its facts and layers, its own tab and
    /// its own search. The page draws none of it, so its table is not laid
    /// out again.
    ///
    /// Nothing is selected: there is no inspector on screen to redraw, and
    /// the next selection draws it as it is.
    pub(crate) fn redraw_inspector(&self, cx: &mut App) {
        if self.view.selected_id.is_some() {
            self.pane.update(cx, |_, cx| cx.notify());
        }
    }

    /// How many times a corner of the window, or the shell itself, has been
    /// drawn.
    #[cfg(test)]
    pub(crate) fn draws(&self, corner: Option<Corner>, cx: &App) -> u32 {
        match corner {
            Some(Corner::Page) => self.page.read(cx).draws,
            Some(Corner::Inspector) => self.pane.read(cx).draws,
            None => self.draws,
        }
    }
}
