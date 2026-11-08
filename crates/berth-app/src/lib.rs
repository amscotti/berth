//! berth's application shell.
//!
//! M0 opens one window with the design tokens, the rail, the page frame and a
//! static containers table. The engine worker, refresh commit and inspector
//! jobs land in later milestones; the shape of the shell is already the final
//! one.

mod app;
mod app_actions;
mod app_bulk;
mod app_detail;
mod app_engine;
mod app_forms;
mod app_render;
mod assets;
mod host;
mod jobs;
mod settings;
mod state;
mod terminal;
mod tray;
mod ui;

use berth_commands::app_actions::Quit;
use berth_commands::{Platform, install_keymap, menus};
use berth_ui::theme;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Focusable, QuitMode, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

pub use app::{BerthApp, CreateKind, Filter, InspectorTab, Tab};
pub use assets::Assets;
pub use state::{Snapshot, fixture_snapshot, fixture_snapshot_with};

pub fn state_for_perf(count: usize) -> Snapshot {
    fixture_snapshot_with(count)
}

/// Launch the application.
pub fn run() {
    let quit_mode = if cfg!(target_os = "macos") {
        QuitMode::Explicit
    } else {
        QuitMode::LastWindowClosed
    };
    gpui_platform::application()
        .with_assets(crate::assets::Assets)
        .with_quit_mode(quit_mode)
        .run(|cx: &mut App| {
            install(cx);
            open_main_window(cx);
            cx.activate(true);
        });
}

fn install(cx: &mut App) {
    install_keymap(cx, Platform::current());
    cx.set_menus(menus());
    cx.on_action(|_: &Quit, cx| cx.quit());
}

fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1280.0), px(760.0)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("berth".into()),
                    ..Default::default()
                }),
                app_id: Some("dev.berth.app".to_owned()),
                window_min_size: Some(size(px(960.0), px(600.0))),
                focus: true,
                ..Default::default()
            },
            |window, cx| {
                let tokens = theme::for_appearance(window.appearance());
                cx.new(|cx| BerthApp::new(tokens, cx))
            },
        )
        .expect("open berth window");

    handle
        .update(cx, |app, window, cx| {
            let weak = cx.entity().downgrade();
            let subscription = window.observe_window_appearance(move |window, cx| {
                let tokens = theme::for_appearance(window.appearance());
                weak.update(cx, |app, cx| app.set_tokens(tokens, cx)).ok();
            });
            app.set_appearance_subscription(subscription);
            app.window_handle = Some(handle.into());
            let focus = app.focus_handle(cx);
            window.focus(&focus, cx);
            app.start(cx);
            app.start_tray(cx);
            let weak = cx.entity().downgrade();
            window.on_window_should_close(cx, move |window, cx| {
                let has_tray = weak
                    .update(cx, |app, _| app.tray.is_some())
                    .unwrap_or(false);
                if has_tray {
                    // Keep the state alive: minimize instead of destroying the
                    // window. The tray's Open item activates it again.
                    window.minimize_window();
                    false
                } else {
                    cx.quit();
                    true
                }
            });
        })
        .expect("initialize berth window");
}
