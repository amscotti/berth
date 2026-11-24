//! Tests that render every page, inspector tab, object inspector, sheet, and
//! page-level shortcut in process so no view path or error state can panic or
//! go dark unnoticed.

use std::sync::Arc;

use berth_engine::discovery::{Dialect, Kind, Runtime};
use gpui::{FocusHandle, Focusable, TestAppContext, VisualTestContext, px, size};

use crate::actions::{ContainerAction, Pending};
use crate::app::{CreateKind, DetailKind, InspectorTab, SearchKind, Tab};
use crate::jobs::engine_ops::EngineOp;
use crate::settings::Appearance;
use crate::testkit;
use crate::view::Sheet;

fn shortcut(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

fn focused(cx: &mut VisualTestContext, handle: &FocusHandle) -> bool {
    cx.update(|window, _| handle.is_focused(window))
}

#[gpui::test]
fn every_page_renders_across_empty_error_and_populated_states(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    cx.simulate_resize(size(px(1280.0), px(760.0)));

    // Render every rail page with populated fixture data.
    for tab in Tab::ALL {
        app.update(cx, |app, cx| app.switch_to(tab, cx));
        testkit::redraw(cx);
        assert_eq!(app.read_with(&*cx, |app, _| app.view.tab), tab);
    }

    // Overview variations: partial sample, missing facts, warning, reading, offline.
    app.update(cx, |app, cx| {
        app.show_partial_sample();
        app.redraw_page(cx);
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.show_engine_facts_missing();
        app.redraw_page(cx);
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.engine.error = "daemon timed out".into();
        app.redraw_page(cx);
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.engine.error.clear();
        app.engine.loading = true;
        app.engine.client = None;
        app.redraw_page(cx);
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.engine.loading = false;
        app.redraw_page(cx);
    });
    testkit::redraw(cx);

    // Disk page without df: pending state and explicit df_error state.
    app.update(cx, |app, cx| {
        app.switch_to(Tab::Disk, cx);
        let mut snapshot = (*app.snapshot).clone();
        snapshot.df = None;
        snapshot.df_error.clear();
        app.snapshot = Arc::new(snapshot);
        cx.notify();
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        let mut snapshot = (*app.snapshot).clone();
        snapshot.df_error = "disk query timed out".into();
        app.snapshot = Arc::new(snapshot);
        cx.notify();
    });
    testkit::redraw(cx);

    // Runtimes page with active, reachable-secondary, unreachable, and empty states.
    app.update(cx, |app, cx| {
        app.switch_to(Tab::Runtimes, cx);
        app.engine.runtimes.push(Runtime {
            name: "Colima".into(),
            kind: Kind::Colima,
            dialect: Dialect::Docker,
            socket: "/tmp/colima.sock".into(),
            reachable: false,
            error_text: "connection refused".into(),
            ..Default::default()
        });
        app.engine.runtimes.push(Runtime {
            name: "Podman".into(),
            kind: Kind::PodmanMachine,
            dialect: Dialect::Podman,
            socket: "/tmp/podman.sock".into(),
            reachable: true,
            ..Default::default()
        });
        cx.notify();
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.engine.runtimes.clear();
        app.engine.loading = false;
        cx.notify();
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.engine.loading = true;
        cx.notify();
    });
    testkit::redraw(cx);

    // Stacks page when no Compose projects exist, and Settings in Dark/Light/System.
    app.update(cx, |app, cx| {
        app.switch_to(Tab::Stacks, cx);
        let mut snapshot = (*app.snapshot).clone();
        snapshot.projects.clear();
        app.snapshot = Arc::new(snapshot);
        cx.notify();
    });
    testkit::redraw(cx);
    app.update_in(cx, |app, window, cx| {
        app.switch_to(Tab::Settings, cx);
        app.set_appearance(Appearance::Dark, window, cx);
    });
    testkit::redraw(cx);
    app.update_in(cx, |app, window, cx| {
        app.set_appearance(Appearance::Light, window, cx);
    });
    testkit::redraw(cx);
    app.update_in(cx, |app, window, cx| {
        app.set_appearance(Appearance::System, window, cx);
    });
    testkit::redraw(cx);

    // Images page with the Pull & Run creation card expanded.
    app.update(cx, |app, cx| {
        app.switch_to(Tab::Images, cx);
        app.view.creation_open = true;
        cx.notify();
    });
    testkit::redraw(cx);
}

#[gpui::test]
fn every_inspector_tab_and_object_inspector_renders_and_copies(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    cx.simulate_resize(size(px(1280.0), px(760.0)));

    // Container inspector across all four tabs with full fixture facts/top/changes.
    for tab in InspectorTab::ALL {
        app.update(cx, |app, cx| {
            app.select_fixture_container(tab, cx);
            if tab == InspectorTab::Inspect {
                app.inspector.inspect_text = Some("{\n  \"Id\": \"demo-1\"\n}".into());
                app.redraw_inspector(cx);
            }
        });
        testkit::redraw(cx);
        assert_eq!(app.read_with(&*cx, |app, _| app.inspector.tab), tab);
    }

    // Copy actions surface honest toasts.
    app.update(cx, |app, cx| {
        app.copy_logs(cx);
        assert_eq!(
            app.view.toast.as_ref().map(|t| t.text.as_str()),
            Some("Logs copied")
        );
        app.copy_run_command(cx);
        assert_eq!(
            app.view.toast.as_ref().map(|t| t.text.as_str()),
            Some("Run command copied · some env values redacted")
        );
        app.copy_inspect(cx);
        assert_eq!(
            app.view.toast.as_ref().map(|t| t.text.as_str()),
            Some("Inspect JSON copied")
        );
    });

    // Empty/loading container inspector fallbacks for Config, Runtime, and Inspect.
    app.update(cx, |app, cx| {
        app.select(0, cx);
        app.inspector.inspect_facts = None;
        app.inspector.detail_top = None;
        app.inspector.detail_changes.clear();
        app.inspector.inspect_text = None;
        app.inspector.inspect_loading = false;
        app.show_inspector_tab(InspectorTab::Config, cx);
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.show_inspector_tab(InspectorTab::Runtime, cx);
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.show_inspector_tab(InspectorTab::Inspect, cx);
    });
    testkit::redraw(cx);

    // Image inspector with layers and without layers (dangling image).
    app.update(cx, |app, cx| {
        app.switch_to(Tab::Images, cx);
        app.select_fixture_image(cx);
    });
    testkit::redraw(cx);
    assert_eq!(
        app.read_with(&*cx, |app, _| app.view.detail_kind),
        DetailKind::Image
    );
    app.update(cx, |app, cx| {
        let dangling = app.snapshot.images[2].clone();
        app.select_image(&dangling.id, dangling.label(), cx);
        app.inspector.image_layers_loading = false;
        app.redraw_inspector(cx);
    });
    testkit::redraw(cx);

    // Volume inspector and Network inspector.
    app.update(cx, |app, cx| {
        app.switch_to(Tab::Volumes, cx);
        app.select_volume("shop-data", cx);
    });
    testkit::redraw(cx);
    assert_eq!(
        app.read_with(&*cx, |app, _| app.view.detail_kind),
        DetailKind::Volume
    );

    app.update(cx, |app, cx| {
        app.switch_to(Tab::Networks, cx);
        app.select_network("net-shop_default", "shop_default", cx);
    });
    testkit::redraw(cx);
    assert_eq!(
        app.read_with(&*cx, |app, _| app.view.detail_kind),
        DetailKind::Network
    );
}

#[gpui::test]
fn sheets_toasts_and_shortcuts_render_and_dispatch(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    cx.simulate_resize(size(px(1280.0), px(760.0)));

    // Render every sheet variant and toast placement (with and without inspector).
    app.update(cx, |app, cx| app.show_prune(cx));
    testkit::redraw(cx);
    app.update(cx, |app, cx| app.show_cleanup_with_nothing_to_do(cx));
    testkit::redraw(cx);
    app.update(cx, |app, cx| app.show_create(CreateKind::Volume, cx));
    testkit::redraw(cx);
    app.update(cx, |app, cx| app.show_create(CreateKind::Network, cx));
    testkit::redraw(cx);
    app.update(cx, |app, cx| app.confirm_first_delete(cx));
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.confirm(
            Pending::Batch {
                action: ContainerAction::Delete,
                count: 2,
            },
            cx,
        );
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.confirm(
            Pending::Engine {
                op: EngineOp::Restart,
                kind: Kind::Lima,
                socket: "/home/me/.lima/berth-docker/sock/docker.sock".into(),
            },
            cx,
        );
    });
    testkit::redraw(cx);
    app.update(cx, |app, cx| {
        app.close_sheet(cx);
        app.select(0, cx);
        app.show_toast_for_screenshot(cx);
    });
    testkit::redraw(cx);
    assert!(app.read_with(&*cx, |app, _| app.view.sheet.is_none()));

    // Page shortcuts switch across all ten tabs.
    let shortcuts = [
        ("1", Tab::Overview),
        ("2", Tab::Containers),
        ("3", Tab::Images),
        ("4", Tab::Volumes),
        ("5", Tab::Networks),
        ("6", Tab::Events),
        ("7", Tab::Stacks),
        ("8", Tab::Disk),
        ("9", Tab::Runtimes),
        (",", Tab::Settings),
    ];
    for (digit, expected) in shortcuts {
        let key = shortcut(digit);
        cx.simulate_keystrokes(&key);
        assert_eq!(app.read_with(&*cx, |app, _| app.view.tab), expected);
    }

    // Cmd/Ctrl+F focuses the active table's search field, and Escape clears it.
    let find = shortcut("f");
    for (tab, kind) in [
        (Tab::Containers, SearchKind::Containers),
        (Tab::Images, SearchKind::Images),
        (Tab::Volumes, SearchKind::Volumes),
        (Tab::Networks, SearchKind::Networks),
        (Tab::Events, SearchKind::Events),
    ] {
        app.update(cx, |app, cx| app.switch_to(tab, cx));
        testkit::redraw(cx);
        cx.simulate_keystrokes(&find);
        let handle = app.read_with(&*cx, |app, cx| app.search(kind).focus_handle(cx));
        assert!(focused(cx, &handle), "find focused search on {tab:?}");
        cx.simulate_input("abc");
        assert_eq!(
            app.read_with(&*cx, |app, _| app.query(kind).to_owned()),
            "abc"
        );
        testkit::press(cx, "escape");
        assert_eq!(app.read_with(&*cx, |app, _| app.query(kind).to_owned()), "");
    }

    // Every container sort column sorts cleanly both ways.
    app.update(cx, |app, cx| {
        app.switch_to(Tab::Containers, cx);
        for col in 0..=8 {
            app.toggle_sort(0, col, cx);
            assert_eq!(app.visible_rows().len(), 2);
            app.toggle_sort(0, col, cx);
            assert_eq!(app.visible_rows().len(), 2);
        }
        assert!(matches!(app.view.sheet, None::<Sheet>));
    });
}
