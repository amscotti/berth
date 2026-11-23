//! Tests for how the app behaves as a whole, driven in process against the
//! mock engine. Each one is a bug that was on screen first.

use berth_engine::dto::Port;
use berth_engine::test_support::MockEngine;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::app::{BerthApp, Filter};
use crate::jobs::mutation::Request;
use crate::testkit;

/// An app wired to a scripted engine.
fn wired(cx: &mut TestAppContext) -> (Entity<BerthApp>, MockEngine, &mut VisualTestContext) {
    let engine = testkit::scripted_engine();
    let (app, cx) = testkit::app_on(&engine, cx);
    (app, engine, cx)
}

fn stop(socket: &str) -> Request {
    Request::Action {
        socket: socket.to_owned(),
        api: "v1.43".into(),
        method: "POST".into(),
        path: "/containers/demo-1/stop".into(),
        verb: "Stopped".into(),
    }
}

#[gpui::test]
fn a_refused_prune_leaves_its_sheet_open_and_no_unrelated_success_closes_it(
    cx: &mut TestAppContext,
) {
    let (app, engine, cx) = wired(cx);
    let socket = engine.socket();
    app.update(cx, |app, cx| {
        // A stop is in flight: its task is queued and hasn't run yet.
        assert!(app.start_mutation(stop(&socket), cx));
        app.open_prune(cx);
        app.run_prune(cx);
        // Refused, so the sheet is not waiting on anything, and nothing was sent.
        assert!(app.prune_open);
        assert!(!app.prune_in_flight);
    });
    assert_eq!(engine.count("/prune"), 0);

    // The stop finishing must not close the sheet as though the prune had run.
    cx.run_until_parked();
    app.read_with(&*cx, |app, _| {
        assert!(app.prune_open);
        assert!(!app.prune_in_flight);
    });
    assert_eq!(engine.count("/prune"), 0, "a refused prune is never sent");
    assert_eq!(engine.count("/containers/demo-1/stop"), 1);
}

#[gpui::test]
fn an_accepted_prune_closes_its_sheet_only_once_it_has_run(cx: &mut TestAppContext) {
    let (app, engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        app.open_prune(cx);
        app.run_prune(cx);
        assert!(app.prune_in_flight);
        assert!(app.prune_open);
    });
    assert_eq!(
        engine.count("/prune"),
        0,
        "the request is queued, not sent yet"
    );
    cx.run_until_parked();
    app.read_with(&*cx, |app, _| {
        assert!(!app.prune_open);
        assert!(!app.prune_in_flight);
        assert_eq!(app.toast.as_ref().map(|toast| toast.bad), Some(false));
    });
    assert!(engine.count("/prune") >= 1);
}

#[gpui::test]
fn a_refresh_in_flight_is_not_a_mutation(cx: &mut TestAppContext) {
    let (app, engine, cx) = wired(cx);
    let socket = engine.socket();
    app.update(cx, |app, cx| {
        app.refresh_task = Some(cx.spawn(async move |_, _| ()));
        assert!(
            !app.mutating(),
            "a pass reads; it doesn't change anything, and must not disable Confirm"
        );
        // A real mutation does.
        app.refresh_task = None;
        assert!(app.start_mutation(stop(&socket), cx));
        assert!(app.mutating());
        // And a second one is refused.
        assert!(!app.start_mutation(stop(&socket), cx));
    });
}

#[gpui::test]
fn changing_the_filter_clears_a_selection_that_no_longer_fits(cx: &mut TestAppContext) {
    let (app, _engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        app.select(0, cx);
        assert!(app.selected_id.is_some());
        // The fixture's container is running, so it isn't among the stopped.
        app.set_filter(Filter::Stopped, cx);
        assert!(app.selected_id.is_none());
        // Cleared the whole way: no detail job is left running against nothing.
        assert!(app.detail_task.is_none());
        assert!(app.detail_lifetime.is_none());
        assert!(app.logs_text.is_empty());
    });
}

#[gpui::test]
fn a_selection_that_still_fits_survives_a_filter_change(cx: &mut TestAppContext) {
    let (app, _engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        app.select(0, cx);
        app.set_filter(Filter::Running, cx);
        assert_eq!(app.selected_id.as_deref(), Some("demo-1"));
        // Choosing the filter that is already on changes nothing.
        app.set_filter(Filter::Running, cx);
        assert_eq!(app.selected_id.as_deref(), Some("demo-1"));
    });
}

#[gpui::test]
fn ports_sort_as_numbers_so_8080_comes_before_10000(cx: &mut TestAppContext) {
    let (app, _engine, cx) = wired(cx);
    app.update(cx, |app, _| {
        let mut snapshot = (*app.snapshot).clone();
        // One container publishes 10000, the other 8080. As text, "10000"
        // would sort first.
        for (index, port) in [(0usize, 10000u16), (1, 8080)] {
            snapshot.containers[index].ports = vec![Port {
                private_port: 80,
                public_port: Some(port),
                ..Default::default()
            }];
        }
        app.snapshot = std::sync::Arc::new(snapshot);
        app.sorts[0] = (8, true);
        assert_eq!(
            app.visible_rows(),
            [1, 0],
            "ascending: the lower port first"
        );
        app.sorts[0] = (8, false);
        assert_eq!(
            app.visible_rows(),
            [0, 1],
            "descending: the higher port first"
        );
    });
}

#[gpui::test]
fn a_container_with_no_port_sorts_after_those_with_one(cx: &mut TestAppContext) {
    let (app, _engine, cx) = wired(cx);
    app.update(cx, |app, _| {
        // In the fixture only the first container publishes a port.
        app.sorts[0] = (8, true);
        assert_eq!(app.visible_rows(), [0, 1]);
    });
}
