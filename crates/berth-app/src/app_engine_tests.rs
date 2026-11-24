//! Tests for engine discovery, refresh commit, switching, and error recovery.

use berth_core::history::Sample;
use berth_engine::df::Df;
use berth_engine::dto::{UsageData, Volume};
use berth_engine::events::Event;
use gpui::TestAppContext;

use crate::jobs::refresh;
use crate::state::{HostSample, Snapshot};

/// The fixture engine, on a socket that exists nowhere: the tasks a scan
/// or a refresh spawns must never reach this machine's real engine.
fn runtime() -> berth_engine::discovery::Runtime {
    let mut runtime = crate::fixture::fixture_runtime();
    runtime.socket = "/nowhere/berth-test.sock".into();
    runtime
}

fn outcome(
    snapshot: Snapshot,
    df: Option<Result<Df, String>>,
    generation: u64,
) -> refresh::Outcome {
    refresh::Outcome {
        snapshot,
        host: HostSample::default(),
        df,
        generation,
    }
}

/// The machine's own figures aren't the engine's. A new sample lands
/// beside the snapshot and leaves the snapshot on screen exactly as it is.
#[gpui::test]
fn a_host_sample_does_not_rebuild_the_snapshot(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    let on_screen = app.read_with(&*cx, |app, _| app.snapshot.clone());
    app.update(cx, |app, cx| app.sample_host_async(cx));
    cx.run_until_parked();
    app.read_with(&*cx, |app, _| {
        assert!(app.host_task.is_none(), "the sample finished");
        assert!(
            std::sync::Arc::ptr_eq(&app.snapshot, &on_screen),
            "the snapshot was not rebuilt for the host's figures"
        );
        // The fixture's 64 GiB gave way to what this machine really has.
        assert!(app.host.mem_total > 0, "the sample landed");
    });
}

#[gpui::test]
fn finding_the_same_engine_again_keeps_what_was_learned_from_it(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        let runtime = runtime();
        app.adopt_runtimes(vec![runtime.clone()], app.engine.generation, cx);
        let generation = app.engine.generation;
        app.engine.history.push(Sample::default());
        app.engine.df = Some(Df::default());
        app.engine.events_since = 42;

        // The same daemon, found again by a rescan, now with a silent
        // neighbour: nothing starts over.
        let silent = berth_engine::discovery::Runtime {
            socket: "/nowhere/other.sock".into(),
            ..Default::default()
        };
        app.adopt_runtimes(vec![silent, runtime], generation, cx);
        assert_eq!(app.engine.generation, generation);
        assert_eq!(app.engine.history.len(), 1);
        assert!(app.engine.df.is_some());
        assert_eq!(app.engine.events_since, 42);
        assert_eq!(
            app.engine.active_index(),
            Some(1),
            "the list may reorder; the engine stays"
        );
    });
}

/// The engine being read is known by identity. One that has left the list
/// is no longer the engine, whatever has taken its place in the list.
#[gpui::test]
fn an_engine_that_left_the_list_is_no_longer_the_active_one(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        let runtime = runtime();
        app.adopt_runtimes(vec![runtime.clone()], app.engine.generation, cx);
        assert_eq!(
            app.engine.active_runtime().map(|it| &it.socket),
            Some(&runtime.socket)
        );

        // A scan that finds nothing answering, and not the engine either.
        let stranger = berth_engine::discovery::Runtime {
            socket: "/nowhere/stranger.sock".into(),
            ..Default::default()
        };
        app.adopt_runtimes(vec![stranger], app.engine.generation, cx);
        assert!(app.engine.client.is_none(), "nothing is being read");
        assert_eq!(
            app.engine.active_index(),
            None,
            "the stranger that took its place is not the engine"
        );
        assert!(app.engine.active_runtime().is_none());
        assert_eq!(app.tray_title(), "— up · — · —");
    });
}

#[gpui::test]
fn a_different_engine_starts_over_and_a_returning_one_reconnects(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        let runtime = runtime();
        app.adopt_runtimes(vec![runtime.clone()], app.engine.generation, cx);
        app.engine.history.push(Sample::default());

        // Another daemon behind the same socket is a new engine.
        let mut other = runtime.clone();
        other.info.id = "daemon-2".into();
        app.adopt_runtimes(vec![other], app.engine.generation, cx);
        assert!(app.engine.history.is_empty());
        assert!(app.snapshot.containers.is_empty());
        assert!(app.snapshot.events.is_empty());
        assert!(app.view.selected_id.is_none());

        // The same daemon, after it had gone away and the client was
        // dropped: what was learned before it went is not trusted.
        app.engine.history.push(Sample::default());
        app.engine.client = None;
        app.adopt_runtimes(vec![runtime], app.engine.generation, cx);
        assert!(app.engine.client.is_some(), "it reconnects");
        assert!(app.engine.history.is_empty(), "and starts over");
    });
}

#[gpui::test]
fn a_rescan_asked_for_during_a_scan_runs_after_it(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.start_discovery(cx);
        assert!(app.discovery_task.is_some());
        app.start_discovery(cx);
        app.rescan(cx);
        assert!(
            app.rescan_wanted,
            "the later requests are kept, not dropped"
        );
        // The engine being read is not put on hold for the scan.
        assert_eq!(
            app.snapshot.containers.len(),
            2,
            "the snapshot on screen is untouched"
        );
    });
}

#[gpui::test]
fn a_quiet_refresh_preserves_the_events_ring(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.set_snapshot(Snapshot::empty());
        let generation = app.engine.generation;
        let first = Snapshot {
            events: vec![Event {
                id: "c1".into(),
                kind: "container".into(),
                action: "start".into(),
                time: 100,
                time_nano: 100_000_000_000,
                ..Default::default()
            }],
            last_update: 100,
            ..Default::default()
        };
        app.finish_refresh(Ok(outcome(first, None, generation)), cx);
        assert_eq!(app.snapshot.events.len(), 1);

        // Next pass returns no new events and no error: the ring must stay.
        let quiet = Snapshot {
            events: Vec::new(),
            events_error: false,
            last_update: 105,
            ..Default::default()
        };
        app.finish_refresh(Ok(outcome(quiet, None, generation)), cx);
        assert_eq!(
            app.snapshot.events.len(),
            1,
            "a quiet pass must not wipe accumulated events"
        );
        assert_eq!(app.snapshot.events[0].id, "c1");
    });
}

#[gpui::test]
fn commit_merges_df_volume_usage_into_snapshot_volumes(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        let generation = app.engine.generation;
        let bare = || Volume {
            name: "shop-data".into(),
            driver: "local".into(),
            usage_data: None,
            ..Default::default()
        };
        let df = Df {
            volumes: Some(vec![Volume {
                name: "shop-data".into(),
                usage_data: Some(UsageData {
                    size: 4096,
                    ref_count: 0,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let pass1 = Snapshot {
            volumes: vec![bare()],
            last_update: 100,
            ..Default::default()
        };
        app.finish_refresh(Ok(outcome(pass1, Some(Ok(df)), generation)), cx);
        assert_eq!(
            app.snapshot.volumes[0].usage_data,
            Some(UsageData {
                size: 4096,
                ref_count: 0
            }),
            "fresh /system/df fills in Volume usage_data"
        );
        assert_eq!(app.prune_count(2), Some(1), "unused volume is countable");

        // Subsequent pass without new df retains prior df and still merges it.
        let pass2 = Snapshot {
            volumes: vec![bare()],
            last_update: 105,
            ..Default::default()
        };
        app.finish_refresh(Ok(outcome(pass2, None, generation)), cx);
        assert_eq!(
            app.snapshot.volumes[0].usage_data,
            Some(UsageData {
                size: 4096,
                ref_count: 0
            }),
            "cached df continues to populate Volume usage_data"
        );
    });
}

#[gpui::test]
fn use_socket_switches_to_a_different_reachable_engine_while_connected(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        let engine_a = runtime();
        let mut engine_b = runtime();
        engine_b.socket = "/nowhere/engine-b.sock".into();
        engine_b.info.id = "daemon-b".into();

        app.adopt_runtimes(vec![engine_a.clone()], app.engine.generation, cx);
        assert_eq!(
            app.engine.socket.as_deref(),
            Some("/nowhere/berth-test.sock")
        );

        // User requests engine B while engine A is still alive and reachable.
        app.adopt_runtimes_for(
            vec![engine_b.clone(), engine_a.clone()],
            app.engine.generation,
            Some("/nowhere/engine-b.sock".into()),
            cx,
        );
        assert_eq!(
            app.engine.socket.as_deref(),
            Some("/nowhere/engine-b.sock"),
            "explicit custom socket switches even when previous engine is still listed"
        );

        // Requesting an unreachable socket keeps the current engine and toasts.
        app.adopt_runtimes_for(
            vec![engine_b],
            app.engine.generation,
            Some("/nowhere/missing.sock".into()),
            cx,
        );
        assert_eq!(app.engine.socket.as_deref(), Some("/nowhere/engine-b.sock"));
        assert!(
            app.view
                .toast
                .as_ref()
                .is_some_and(|toast| toast.bad && toast.text.contains("/nowhere/missing.sock"))
        );
    });
}

#[gpui::test]
fn offline_background_retry_does_not_clear_engine_error(cx: &mut TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.engine.client = None;
        app.engine.error = "no engine answering".into();
        app.start_discovery_inner(false, cx);
        assert_eq!(
            app.engine.error, "no engine answering",
            "background retry must not wipe the offline error and flicker"
        );
        assert_eq!(app.tray_title(), "— up · — · —");
    });
}

#[gpui::test]
fn finish_refresh_handles_stale_generation_transient_error_dead_engine_and_refresh_wanted(
    cx: &mut TestAppContext,
) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        let engine_a = runtime();
        app.adopt_runtimes(vec![engine_a], app.engine.generation, cx);
        let gen_a = app.engine.generation;
        app.refresh_task = None;

        // Stale generation error is discarded without touching engine.error.
        app.finish_refresh(
            Err(refresh::RefreshError {
                message: "old error".into(),
                engine_dead: true,
                generation: gen_a.wrapping_sub(1),
            }),
            cx,
        );
        assert!(app.engine.error.is_empty());
        assert!(app.engine.client.is_some());

        // Transient error keeps client alive and records error.
        app.finish_refresh(
            Err(refresh::RefreshError {
                message: "timeout".into(),
                engine_dead: false,
                generation: gen_a,
            }),
            cx,
        );
        assert_eq!(app.engine.error, "timeout");
        assert!(app.engine.client.is_some());
        assert_eq!(app.tray_title(), "— up · — · —");

        // refresh_wanted triggers an immediate follow-up refresh on completion.
        app.refresh_wanted = true;
        app.finish_refresh(
            Ok(outcome(
                Snapshot {
                    last_update: 200,
                    ..Default::default()
                },
                None,
                gen_a,
            )),
            cx,
        );
        assert!(!app.refresh_wanted);
        assert!(app.refresh_task.is_some(), "queued refresh started at once");

        // engine_dead drops client, keeps error message, and triggers discovery.
        app.refresh_task = None;
        app.discovery_task = None;
        app.finish_refresh(
            Err(refresh::RefreshError {
                message: "socket refused".into(),
                engine_dead: true,
                generation: gen_a,
            }),
            cx,
        );
        assert!(app.engine.client.is_none());
        assert_eq!(
            app.engine.error, "socket refused",
            "starting discovery after engine_dead must not clear the error"
        );
        assert!(app.discovery_task.is_some());
    });
}
