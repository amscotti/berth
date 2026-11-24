//! Tests for how the app behaves as a whole, driven in process against the
//! mock engine. Each one is a bug that was on screen first.

use std::cell::Cell;

use berth_engine::dto::Port;
use berth_engine::test_support::MockEngine;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::actions::{ContainerAction, Pending, Target};
use crate::app::{BerthApp, CreateKind, Filter, SearchKind};
use crate::jobs::mutation::{Outcome, PruneOptions, Request};
use crate::slot::{MutationKind, MutationSlot};
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
        target: Target::Container {
            action: ContainerAction::Stop,
            id: "demo-1".into(),
            name: "demo-1".into(),
        },
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
        assert!(!prune_running(app));
    });
    assert_eq!(engine.count("/prune"), 0);

    // The stop finishing must not close the sheet as though the prune had run.
    cx.run_until_parked();
    app.read_with(&*cx, |app, _| {
        assert!(cleanup(app));
        assert!(!prune_running(app));
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
        assert!(prune_running(app));
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
        assert!(!prune_running(app));
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
        app.set_snapshot(snapshot);
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
        app.set_snapshot(snapshot);
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
        app.set_snapshot(snapshot);
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

/// Whether the cleanup's own request is the one running.
fn prune_running(app: &BerthApp) -> bool {
    matches!(
        app.mutation,
        MutationSlot::Running {
            kind: MutationKind::Prune,
            ..
        }
    )
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

/// A second request is refused while one is running, and the refusal says
/// what it is waiting for.
#[gpui::test]
fn a_second_request_is_refused_with_the_running_one_named(cx: &mut TestAppContext) {
    let (app, engine, cx) = wired(cx);
    let socket = engine.socket();

    // A cleanup is running: a stop waits, and is not sent.
    app.update(cx, |app, cx| {
        app.open_prune(cx);
        app.run_prune(cx);
        assert!(!app.start_mutation(stop(&socket), cx), "refused");
        assert_eq!(
            app.view
                .toast
                .as_ref()
                .map(|toast| (toast.text.as_str(), toast.bad)),
            Some(("still cleaning up; wait for it to finish", true))
        );
    });
    cx.run_until_parked();
    assert!(engine.count("/prune") >= 1, "the cleanup ran");
    assert_eq!(
        engine.count("/containers/demo-1/stop"),
        0,
        "the stop never did"
    );

    // A stop is running: the cleanup waits, says what for, and its sheet
    // stays open because nothing of it was sent.
    app.update(cx, |app, cx| {
        assert!(app.start_mutation(stop(&socket), cx));
        app.open_prune(cx);
        app.run_prune(cx);
        assert_eq!(
            app.view
                .toast
                .as_ref()
                .map(|toast| (toast.text.as_str(), toast.bad)),
            Some(("still stopping demo-1; wait for it to finish", true))
        );
        assert!(!prune_running(app));
        assert!(cleanup(app));
    });
    cx.run_until_parked();
    assert_eq!(engine.count("/containers/demo-1/stop"), 1);
}

/// A table's rows are worked out once per snapshot, query, sort and filter,
/// and kept from one frame to the next while those hold.
#[gpui::test]
fn a_tables_rows_are_worked_out_once_per_snapshot_and_settings(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    app.update(cx, |app, _| {
        // Behind a reference, so that every closure counts on the same one.
        let figured = &Cell::new(0);
        let containers = |app: &BerthApp| {
            app.rows_of(SearchKind::Containers, || {
                figured.set(figured.get() + 1);
                (0..app.snapshot.containers.len()).collect()
            })
        };
        let drawn = |app: &BerthApp| {
            for _ in 0..3 {
                containers(app);
            }
        };

        // The window was drawn once to open, with nothing typed: start from
        // a question it hasn't answered.
        app.view.lists[0].query = "web".into();
        drawn(app);
        assert_eq!(figured.get(), 1, "three frames, one answer");

        // The same query in another case is the same question.
        app.view.lists[0].query = "WEB".into();
        drawn(app);
        assert_eq!(figured.get(), 1, "but not its case");

        app.view.lists[0].sort = (2, false);
        drawn(app);
        assert_eq!(figured.get(), 2, "a sort is a new question");
        app.view.filter = Filter::Stopped;
        drawn(app);
        assert_eq!(figured.get(), 3, "and so is a filter");

        app.set_snapshot((*app.snapshot).clone());
        drawn(app);
        assert_eq!(
            figured.get(),
            4,
            "and a commit, even one where nothing moved: a new snapshot is a new answer"
        );

        // Each table keeps its own.
        for _ in 0..2 {
            app.rows_of(SearchKind::Images, || {
                figured.set(figured.get() + 1);
                Vec::new()
            });
        }
        assert_eq!(
            figured.get(),
            5,
            "the images' answer is not the containers'"
        );

        // And the answer is the table's own.
        app.view.filter = Filter::All;
        app.view.lists[0].query = "web".into();
        assert_eq!(app.visible_rows(), [0], "web, among the fixture's two");
    });
}

#[gpui::test]
fn confirming_a_delete_closes_the_sheet_and_repaints_immediately(cx: &mut TestAppContext) {
    let (app, engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        app.confirm(
            Pending::One(Target::Container {
                action: ContainerAction::Delete,
                id: "demo-1".into(),
                name: "demo-1".into(),
            }),
            cx,
        );
    });
    testkit::redraw(cx);
    let draws_before = app.read_with(&*cx, |app, cx| app.draws(None, cx));

    app.update(cx, |app, cx| {
        app.resolve_pending(true, cx);
        assert!(app.view.sheet.is_none(), "confirm closes the sheet at once");
        assert!(app.mutating(), "and starts the mutation");
    });
    testkit::redraw(cx);

    app.read_with(&*cx, |app, cx| {
        assert!(app.view.sheet.is_none());
        assert!(
            app.draws(None, cx) > draws_before,
            "closing and starting the mutation repainted the shell immediately"
        );
    });
    cx.run_until_parked();
    assert_eq!(engine.count("/containers/demo-1?force=1"), 1);
}

#[gpui::test]
fn a_prune_resets_df_at_so_disk_figures_refresh_immediately(cx: &mut TestAppContext) {
    let (app, engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        app.engine.df_at = berth_core::format::now();
        app.open_prune(cx);
        app.run_prune(cx);
    });
    cx.run_until_parked();
    assert!(engine.count("/prune") >= 1);
    assert!(
        engine.count("/system/df") >= 1,
        "df_at was reset so the follow-up refresh re-fetched /system/df"
    );
}

#[gpui::test]
fn a_stale_engine_mutation_cannot_close_the_new_engines_sheet(cx: &mut TestAppContext) {
    let (app, _engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        let old_gen = app.engine.generation;
        app.mutation = MutationSlot::Running {
            kind: MutationKind::Prune,
            since: std::time::Instant::now(),
            task: cx.spawn(async move |_, _| ()),
        };
        // Engine switched while the prune was in flight, and a new Prune sheet
        // was opened on the new engine.
        app.engine.generation = old_gen.wrapping_add(1);
        app.open_prune(cx);

        app.finish_mutation(
            Outcome {
                message: "Reclaimed 0 B".into(),
                bad: false,
            },
            old_gen,
            false,
            cx,
        );
        assert!(
            cleanup(app),
            "stale mutation must not close the new engine's sheet"
        );
        assert!(!app.mutating(), "the mutation slot was still released");
    });
}

#[gpui::test]
fn bulk_selection_and_compose_stack_actions_dispatch_batches(cx: &mut TestAppContext) {
    let (app, engine, cx) = wired(cx);
    app.update(cx, |app, cx| {
        app.toggle_bulk(cx);
        assert!(app.view.bulk_enabled);
        app.bulk_select_all(cx);
        assert_eq!(app.view.bulk_selected.len(), 2);
        app.bulk_clear(cx);
        assert!(app.view.bulk_selected.is_empty());

        app.bulk_toggle("demo-1", cx);
        assert!(app.view.bulk_selected.contains("demo-1"));
        // Destructive bulk opens a confirmation sheet instead of sending.
        app.bulk_request(ContainerAction::Delete, cx);
        assert!(matches!(
            app.view.sheet,
            Some(Sheet::Confirm(Pending::Batch {
                action: ContainerAction::Delete,
                count: 1
            }))
        ));
        app.resolve_pending(false, cx);
        // Non-destructive bulk runs immediately and clears selection.
        app.bulk_request(ContainerAction::Stop, cx);
        assert!(app.view.bulk_selected.is_empty());
    });
    cx.run_until_parked();
    assert_eq!(engine.count("/containers/demo-1/stop"), 1);

    // Compose project actions target matching project members.
    app.update(cx, |app, cx| {
        app.set_snapshot(crate::fixture::fixture_snapshot());
        app.project_restart("shop", cx);
    });
    cx.run_until_parked();
    assert_eq!(engine.count("/restart"), 2);

    app.update(cx, |app, cx| {
        app.set_snapshot(crate::fixture::fixture_snapshot());
        app.project_action("shop", false, cx);
    });
    cx.run_until_parked();
    assert_eq!(engine.count("/stop"), 3);
}
