//! Preferences: the one place they are written, and the window they remember.

use std::time::Duration;

use gpui::{Bounds, Context, Pixels, Subscription, Window};

use crate::app::BerthApp;
use crate::settings;

/// How long the window has to be still before where it is gets written. A
/// resize is a stream of changes, and only the last one matters.
const SETTLE: Duration = Duration::from_millis(400);

impl BerthApp {
    /// The preferences as they stand.
    fn settings(&self) -> settings::Data {
        settings::Data {
            appearance: self.appearance,
            socket: self.engine.preferred_socket.clone().unwrap_or_default(),
            window: self.window,
        }
    }

    /// Write the preferences, and say so if they can't be. Where there is no
    /// file, as in tests, nothing is persisted and nothing is said.
    pub(crate) fn save_settings(&self, cx: &mut Context<Self>) {
        let Some(path) = self.settings_file.clone() else {
            return;
        };
        let data = self.settings();
        // Deliberately not held on to: a save must outlive the state that
        // asked for it, and nothing waits on it.
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_executor()
                .spawn(async move { settings::save_to(&path, &data) })
                .await;
            if let Err(error) = written {
                this.update(cx, |app, cx| {
                    app.notify(&format!("Preferences weren't saved: {error}"), true, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// The window moved, or was resized. Remember where, and write it once
    /// it has been still for a moment.
    pub(crate) fn remember_window(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        self.window = Some(settings::Window::of(bounds));
        // Starting the wait again cancels the one before it.
        self.settings_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |app, cx| app.save_settings(cx)).ok();
        }));
    }

    /// Tell the app whenever the window moves or is resized.
    pub(crate) fn watch_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.observe_window_bounds(window, |this, window, cx| {
            this.remember_window(window.bounds(), cx)
        })
    }
}
