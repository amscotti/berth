//! Tests for what berth remembers between launches: that the fixture keeps
//! its hands off the preferences of whoever runs it, that the window is
//! remembered once it is still, and that a save that fails is said.

use berth_engine::test_support::TempDir;
use gpui::{TestAppContext, px, size};
use std::time::Duration;

use crate::settings::{self, Appearance};
use crate::testkit;

/// Let the window settle, and let the save that follows run.
fn settle(cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
}

#[gpui::test]
fn the_fixture_neither_reads_nor_writes_anyones_preferences(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    app.read_with(&*cx, |app, _| {
        assert_eq!(app.settings_file, None, "nothing is persisted");
        assert_eq!(app.appearance, Appearance::System, "not the saved choice");
        assert_eq!(app.engine.preferred_socket, None, "not the saved socket");
        assert_eq!(app.window, None, "no window was left");
    });
    // Changing the appearance and the engine saves nothing anywhere.
    app.update(cx, |app, cx| app.save_settings(cx));
    settle(cx);
    assert!(app.read_with(&*cx, |app, _| app.view.toast.is_none()));
}

#[gpui::test]
fn the_window_is_remembered_once_it_is_still(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-window");
    let path = dir.path().join("settings.json");
    let (app, cx) = testkit::fixture(cx);
    // Wired as the running app is: a file to write, and an eye on the window.
    app.update_in(cx, |app, window, cx| {
        app.settings_file = Some(path.clone());
        let subscription = app.watch_window(window, cx);
        app.remember_bounds(subscription);
    });

    cx.simulate_resize(size(px(1100.0), px(700.0)));
    cx.run_until_parked();
    assert!(
        !path.exists(),
        "nothing is written while the window is still moving"
    );
    // Moving again before the wait is up starts it over rather than writing
    // the size in between.
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.simulate_resize(size(px(1200.0), px(710.0)));
    settle(cx);

    let saved = settings::load_from(&path);
    assert_eq!(
        saved.window.map(|window| (window.width, window.height)),
        Some((1200, 710)),
        "the last size is what was written"
    );
    // It comes back as the size and place to open at.
    app.read_with(&*cx, |app, _| {
        let window = app.window.expect("remembered");
        let anywhere = gpui::Bounds::new(
            gpui::point(px(-10_000.0), px(-10_000.0)),
            size(px(20_000.0), px(20_000.0)),
        );
        let minimum = size(px(960.0), px(600.0));
        assert_eq!(
            window.bounds(minimum, &[anywhere]).map(|it| it.size),
            Some(size(px(1200.0), px(710.0)))
        );
    });
}

#[gpui::test]
fn a_save_that_fails_is_said(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-refused");
    // A file where a directory would have to be: no write can start, whoever
    // is running.
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, b"").expect("make the blocker");
    let (app, cx) = testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.settings_file = Some(blocker.join("settings.json"));
        app.save_settings(cx);
    });
    cx.run_until_parked();
    let toast = app.read_with(&*cx, |app, _| app.view.toast.clone());
    assert!(
        toast
            .as_ref()
            .is_some_and(|toast| toast.bad && toast.text.starts_with("Preferences weren't saved")),
        "the failure reached the person: {toast:?}"
    );
}
