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

#[gpui::test]
fn unchanged_launch_bounds_do_not_schedule_a_redundant_write(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-unchanged");
    let path = dir.path().join("settings.json");
    let (app, cx) = testkit::fixture(cx);
    app.update_in(cx, |app, window, cx| {
        let initial = settings::Window::of(window.bounds(), window.viewport_size());
        app.settings_file = Some(path.clone());
        app.window = Some(initial);
        app.remember_window(window.bounds(), window.viewport_size(), cx);
    });
    settle(cx);
    assert!(
        !path.exists(),
        "matching bounds at launch must not trigger a disk write"
    );
}

#[gpui::test]
fn overlapping_saves_do_not_race_or_lose_the_latest_preference(cx: &mut TestAppContext) {
    let dir = TempDir::new("settings-concurrent");
    let path = dir.path().join("settings.json");
    let (app, cx) = testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.settings_file = Some(path.clone());
        for i in 0..8 {
            app.appearance = if i % 2 == 0 {
                Appearance::Light
            } else {
                Appearance::Dark
            };
            app.engine.preferred_socket = Some(format!("/tmp/sock-{i}.sock"));
            app.save_settings(cx);
        }
    });
    cx.run_until_parked();
    assert!(
        app.read_with(&*cx, |app, _| app.view.toast.is_none()),
        "overlapping saves must not produce a failure toast"
    );
    let saved = settings::load_from(&path);
    assert_eq!(saved.appearance, Appearance::Dark);
    assert_eq!(saved.socket, "/tmp/sock-7.sock");
}
