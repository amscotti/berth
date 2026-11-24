//! Helpers for tests that drive the app in process. Compiled only for tests.

use berth_engine::test_support::MockEngine;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::app::BerthApp;

/// A mock with one running container and every route a pass or an action
/// visits: the list calls, stats, events, df and the lifecycle routes. Each
/// route repeats its answer, so a test can run as many passes as it likes.
pub(crate) fn scripted_engine() -> MockEngine {
    scripted_engine_with(berth_engine::test_support::Reply::ok(""))
}

/// `scripted_engine`, with `/events` answering `events`.
pub(crate) fn scripted_engine_with(events: berth_engine::test_support::Reply) -> MockEngine {
    const CONTAINERS: &str = r#"[{"Id":"demo-1","Names":["/web"],"Image":"nginx:alpine","State":"running","Status":"Up 1 hour"}]"#;
    const STATS: &str = r#"{"cpu_stats":{"cpu_usage":{"total_usage":3000000},"system_cpu_usage":40000000,"online_cpus":4},"precpu_stats":{"cpu_usage":{"total_usage":1000000},"system_cpu_usage":20000000},"memory_stats":{"usage":1000,"limit":2000,"stats":{"inactive_file":200}},"pids_stats":{"current":5}}"#;
    const DF: &str = r#"{"LayersSize":0,"ImageUsage":{"TotalSize":100,"Reclaimable":40}}"#;
    let engine = MockEngine::start();
    engine
        .on("GET /containers/json")
        .ok(CONTAINERS)
        .on("/stats")
        .ok(STATS)
        .on("GET /images/json")
        .ok("[]")
        .on("GET /volumes")
        .ok(r#"{"Volumes":[]}"#)
        .on("GET /networks")
        .ok("[]")
        .on("GET /events")
        .reply(events)
        .on("GET /system/df")
        .ok(DF)
        .on("POST /containers/")
        .ok("")
        .on("DELETE /containers/")
        .ok("")
        .on("POST /images/prune")
        .ok(r#"{"SpaceReclaimed":0}"#)
        .on("POST /volumes/prune")
        .ok(r#"{"SpaceReclaimed":0}"#)
        .on("POST /networks/prune")
        .ok(r#"{"SpaceReclaimed":0}"#)
        .on("POST /build/prune")
        .ok(r#"{"SpaceReclaimed":0}"#)
        .on("/logs")
        .reply(berth_engine::test_support::Reply::framed("hello\n"))
        .on("/json")
        .ok(r#"{"Id":"demo-1","Name":"/web","State":{"Running":true}}"#);
    engine
}

/// An app on the fixture, in a window. No engine is wired in, so nothing
/// talks to a socket unless a test says so.
pub(crate) fn fixture(cx: &mut TestAppContext) -> (Entity<BerthApp>, &mut VisualTestContext) {
    // The keys the app binds answer in tests as they do in the window.
    cx.update(|cx| berth_commands::install_keymap(cx, berth_commands::Platform::current()));
    cx.add_window_view(|_, cx| {
        berth_ui::theme::ActiveTokens::set(berth_ui::theme::light(), cx);
        BerthApp::new_fixture(berth_ui::theme::light(), cx)
    })
}

/// An app on the fixture, wired to read `engine`. Nothing starts by itself:
/// a test drives a pass or an action when it wants one.
pub(crate) fn app_on<'a>(
    engine: &MockEngine,
    cx: &'a mut TestAppContext,
) -> (Entity<BerthApp>, &'a mut VisualTestContext) {
    cx.update(|cx| berth_commands::install_keymap(cx, berth_commands::Platform::current()));
    let socket = engine.socket();
    let (app, cx) = cx.add_window_view(|_, cx| {
        berth_ui::theme::ActiveTokens::set(berth_ui::theme::light(), cx);
        BerthApp::new_fixture(berth_ui::theme::light(), cx)
    });
    app.update(cx, |app, _| {
        app.engine.client = Some(berth_engine::client::Client::new(socket).with_api("v1.43"));
    });
    (app, cx)
}

/// Draw the window, so layout-dependent state (scroll offsets, element
/// bounds, hitboxes) is current. A harmless mouse move does it.
pub(crate) fn draw(cx: &mut VisualTestContext) {
    cx.simulate_event(gpui::MouseMoveEvent::default());
}

/// Draw the window without touching the mouse or the keyboard, for tests
/// that count what a change redraws. A mouse move (`draw`) counts as
/// switching to the mouse, and GPUI then redraws the whole window once,
/// views it was keeping included; this changes nothing on screen, so only
/// what was notified is drawn.
pub(crate) fn redraw(cx: &mut VisualTestContext) {
    cx.simulate_modifiers_change(gpui::Modifiers::default());
}

/// Press a key as a person would: down, then up. GPUI's own helper sends the
/// down stroke only, and a control is pressed by the release.
pub(crate) fn press(cx: &mut VisualTestContext, key: &'static str) {
    cx.simulate_keystrokes(key);
    if let Ok(keystroke) = gpui::Keystroke::parse(key) {
        cx.update(|window, cx| {
            window.dispatch_event(
                gpui::PlatformInput::KeyUp(gpui::KeyUpEvent { keystroke }),
                cx,
            );
        });
    }
}

/// Press Tab until `reached` holds, and say how many presses it took. Stops
/// after `limit` presses, so a broken Tab order fails instead of hanging.
pub(crate) fn tab_until(
    cx: &mut VisualTestContext,
    mut reached: impl FnMut(&mut VisualTestContext) -> bool,
    limit: usize,
) -> usize {
    for presses in 0..=limit {
        if reached(cx) {
            return presses;
        }
        press(cx, "tab");
    }
    limit
}
