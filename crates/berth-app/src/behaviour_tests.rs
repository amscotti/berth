//! Tests for how the app behaves as a whole, driven in process against the
//! mock engine. Each one is a bug that was on screen first.

use berth_engine::dto::Port;
use berth_engine::test_support::MockEngine;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::actions::{ContainerAction, Pending, Target};
use crate::app::{BerthApp, CreateKind, Filter};
use crate::jobs::mutation::{PruneOptions, Request};
use crate::testkit;
use crate::view::Sheet;

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
        method: "POST",
        path: "/containers/demo-1/stop".into(),
        verb: "Stopped",
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
        assert!(cleanup(app));
        assert!(!app.view.prune_in_flight);
    });
    assert_eq!(engine.count("/prune"), 0);

    // The stop finishing must not close the sheet as though the prune had run.
    cx.run_until_parked();
    app.read_with(&*cx, |app, _| {
        assert!(cleanup(app));
        assert!(!app.view.prune_in_flight);
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
        assert!(app.view.prune_in_flight);
        assert!(cleanup(app));
    });
    assert_eq!(
        engine.count("/prune"),
        0,
        "the request is queued, not sent yet"
    );
    cx.run_until_parked();
    app.read_with(&*cx, |app, _| {
        assert!(!cleanup(app));
        assert!(!app.view.prune_in_flight);
        assert_eq!(app.view.toast.as_ref().map(|toast| toast.bad), Some(false));
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
        assert!(app.view.selected_id.is_some());
        // The fixture's container is running, so it isn't among the stopped.
        app.set_filter(Filter::Stopped, cx);
        assert!(app.view.selected_id.is_none());
        // Cleared the whole way: no detail job is left running against nothing.
        assert!(app.inspector.detail_task.is_none());
        assert!(app.inspector.inspect_facts.is_none());
        assert!(app.inspector.logs_text.is_empty());
    });
}

#[gpui::test]
fn a_selection_that_still_fits_survives_a_filter_change(cx: &mut TestAppContext) {
    let (app, _engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        app.select(0, cx);
        app.set_filter(Filter::Running, cx);
        assert_eq!(app.view.selected_id.as_deref(), Some("demo-1"));
        // Choosing the filter that is already on changes nothing.
        app.set_filter(Filter::Running, cx);
        assert_eq!(app.view.selected_id.as_deref(), Some("demo-1"));
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
        app.view.lists[0].sort = (8, true);
        assert_eq!(
            app.visible_rows(),
            [1, 0],
            "ascending: the lower port first"
        );
        app.view.lists[0].sort = (8, false);
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
        app.view.lists[0].sort = (8, true);
        assert_eq!(app.visible_rows(), [0, 1]);
    });
}

/// What a cleanup would remove is counted from the lists on screen, not just
/// weighed; and where it can't be counted, that is unknown, not zero.
#[gpui::test]
fn a_cleanup_counts_what_it_would_remove_and_says_when_it_cannot(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    app.read_with(&*cx, |app, _| {
        // The fixture: nothing stopped, one image no container uses, a
        // volume in use, and no network that could go.
        assert_eq!(app.prune_count(0), Some(0));
        assert_eq!(app.prune_count(1), Some(1));
        assert_eq!(app.prune_count(2), Some(0));
        assert_eq!(app.prune_count(3), Some(0));
        assert_eq!(app.prune_count(4), None, "build cache isn't things");
        let options = PruneOptions::default();
        assert_eq!(app.prune_selected_bytes(&options), Some(13 * 1024 * 1024));
        assert_eq!(app.prune_has_work(&options), Some(true));
    });

    // With only the options that have nothing left ticked, the button knows
    // there is nothing to do, and Tab passes it by.
    app.update(cx, |app, cx| {
        app.open_prune(cx);
        app.toggle_prune_option(1, cx);
    });
    let (work, stops) = app.read_with(&*cx, |app, cx| {
        (
            app.prune_has_work(&app.prune_options().expect("the cleanup is open")),
            app.view.sheet_focus.stops(app, cx).len(),
        )
    });
    assert_eq!(work, Some(false));
    assert_eq!(
        stops, 6,
        "five options and Cancel; Clean up is out of reach"
    );

    // A stopped container goes even where it takes no space.
    app.update(cx, |app, cx| {
        let mut snapshot = (*app.snapshot).clone();
        snapshot.containers[0].state = "exited".into();
        app.snapshot = std::sync::Arc::new(snapshot);
        cx.notify();
    });
    assert_eq!(
        app.read_with(&*cx, |app, _| {
            (
                app.prune_count(0),
                app.prune_has_work(&PruneOptions::default()),
            )
        }),
        (Some(1), Some(true))
    );

    // Without the engine's disk figures nothing can be counted or weighed.
    // That is unknown, and the engine decides: the button isn't refused.
    app.update(cx, |app, cx| {
        let mut snapshot = (*app.snapshot).clone();
        snapshot.df = None;
        app.snapshot = std::sync::Arc::new(snapshot);
        cx.notify();
    });
    app.read_with(&*cx, |app, _| {
        assert_eq!(app.prune_count(0), None);
        assert_eq!(app.prune_count(1), None);
        let options = PruneOptions::default();
        assert_eq!(app.prune_selected_bytes(&options), None);
        assert_eq!(app.prune_has_work(&options), None);
    });
}

/// Whether the cleanup sheet is the one that is open.
fn cleanup(app: &BerthApp) -> bool {
    matches!(app.view.sheet, Some(Sheet::Prune(_)))
}

/// A create is refused by the one rule for names, the same the sheet
/// explains, and nothing is sent for a refused one.
#[gpui::test]
fn a_bad_name_is_refused_before_anything_is_sent(cx: &mut TestAppContext) {
    let (app, engine, cx) = wired(cx);
    // The last is the fixture's own volume.
    for name in ["", "a", "-bad", "bad name", "shop-data"] {
        app.update(cx, |app, cx| {
            app.open_create(CreateKind::Volume, cx);
            app.view
                .create_name
                .update(cx, |input, cx| input.set_value(name, cx));
            app.submit_create(cx);
            assert!(
                matches!(
                    app.view.sheet,
                    Some(Sheet::Create {
                        attempted: true,
                        ..
                    })
                ),
                "{name:?} was refused"
            );
            assert!(
                matches!(app.view.sheet, Some(Sheet::Create { .. })),
                "and the sheet stays to say why"
            );
        });
        cx.run_until_parked();
    }
    assert_eq!(engine.count("/volumes/create"), 0, "nothing was sent");

    // A name that passes the rule is sent, once.
    app.update(cx, |app, cx| {
        app.view
            .create_name
            .update(cx, |input, cx| input.set_value("brand-new", cx));
        app.submit_create(cx);
        assert!(matches!(
            app.view.sheet,
            Some(Sheet::Create {
                attempted: false,
                ..
            })
        ));
    });
    cx.run_until_parked();
    assert_eq!(engine.count("/volumes/create"), 1);
}

/// There is one slot for a sheet, so a sheet opened over another replaces
/// it whole: the first is gone, and nothing of it is left to be found when
/// it opens again.
#[gpui::test]
fn a_sheet_opened_over_another_replaces_it_whole(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.open_prune(cx);
        app.toggle_prune_option(1, cx);
        assert!(cleanup(app));

        app.open_create(CreateKind::Volume, cx);
        assert!(
            matches!(
                app.view.sheet,
                Some(Sheet::Create {
                    kind: CreateKind::Volume,
                    attempted: false
                })
            ),
            "the create sheet takes the slot"
        );

        app.confirm(
            Pending::One(Target::Container {
                action: ContainerAction::Delete,
                id: "demo-1".into(),
                name: "demo-1".into(),
            }),
            cx,
        );
        assert!(
            matches!(app.view.sheet, Some(Sheet::Confirm(_))),
            "and a confirmation takes it in turn"
        );

        // Cancel closes it, and nothing is open behind it.
        app.resolve_pending(false, cx);
        assert!(
            app.view.sheet.is_none(),
            "one sheet was open, so one closed"
        );

        // The cleanup opens as it always does. What was ticked for it before
        // went with the sheet it was ticked in.
        app.open_prune(cx);
        assert_eq!(app.prune_options(), Some(PruneOptions::default()));
    });
}
