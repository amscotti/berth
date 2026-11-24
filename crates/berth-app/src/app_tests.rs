//! Tests for the view vocabulary in `app.rs`.

use berth_engine::dto::Container;

use crate::app::*;

#[test]
fn container_search_matches_the_fields_the_row_shows() {
    let snapshot = crate::fixture::fixture_snapshot();
    let container = &snapshot.containers[0];
    assert!(container_matches(container, ""));
    assert!(container_matches(container, "demo"));
    assert!(container_matches(container, "nginx"));
    assert!(container_matches(container, "8080"));
    assert!(container_matches(container, "shop"));
    assert!(!container_matches(container, "redis"));
}

#[test]
fn the_running_filter_counts_paused_as_up() {
    let running = Container {
        state: "running".into(),
        ..Default::default()
    };
    let paused = Container {
        state: "paused".into(),
        ..Default::default()
    };
    let exited = Container {
        state: "exited".into(),
        ..Default::default()
    };
    assert!(Filter::Running.keeps(&running));
    assert!(Filter::Running.keeps(&paused));
    assert!(!Filter::Running.keeps(&exited));
    assert!(Filter::Stopped.keeps(&exited));
    assert!(Filter::All.keeps(&exited));
}

#[test]
fn rail_figures_come_from_the_snapshot() {
    let snapshot = crate::fixture::fixture_snapshot();
    assert_eq!(Tab::Containers.figure(&snapshot, 1), "2/2");
    assert_eq!(Tab::Images.figure(&snapshot, 1), "3");
    assert_eq!(Tab::Volumes.figure(&snapshot, 1), "1");
    assert_eq!(Tab::Networks.figure(&snapshot, 1), "4");
    assert_eq!(Tab::Settings.figure(&snapshot, 1), "");
    assert!(Tab::Disk.figure(&snapshot, 1).contains("MiB"));
}

#[gpui::test]
fn the_fixture_starts_on_its_engine(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.read_with(&*cx, |app, _| {
        assert_eq!(app.engine.active_index(), Some(0));
        assert_eq!(
            app.engine
                .active_runtime()
                .map(|runtime| runtime.name.as_str()),
            Some("Lima · lima-berth-docker")
        );
    });
}

#[gpui::test]
fn set_appearance_updates_active_tokens_and_blocks_os_override(cx: &mut gpui::TestAppContext) {
    use crate::settings::Appearance;
    use berth_ui::theme;
    use gpui::WindowAppearance;

    let (app, cx) = crate::testkit::fixture(cx);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.set_appearance(Appearance::Dark, window, cx);
        });
        assert_eq!(
            theme::ActiveTokens::get(window, cx),
            theme::dark(),
            "ActiveTokens must update immediately for InputState and other standalone views"
        );

        // An OS appearance switch must NOT hijack an explicit Dark preference.
        app.update(cx, |app, cx| {
            app.apply_window_appearance(WindowAppearance::Light, cx);
        });
        assert_eq!(app.read(cx).tokens, theme::dark());
        assert_eq!(theme::ActiveTokens::get(window, cx), theme::dark());

        // Switching back to System restores following OS appearance changes.
        app.update(cx, |app, cx| {
            app.set_appearance(Appearance::System, window, cx);
            app.apply_window_appearance(WindowAppearance::Light, cx);
        });
        assert_eq!(app.read(cx).tokens, theme::light());
        assert_eq!(theme::ActiveTokens::get(window, cx), theme::light());
    });
}

#[gpui::test]
fn refresh_now_starts_discovery_when_offline(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.engine.client = None;
        app.discovery_task = None;
        app.engine.df_at = 12345;
        app.refresh_now(cx);
        assert_eq!(app.engine.df_at, 0);
        assert!(
            app.discovery_task.is_some(),
            "Cmd+R / Refresh now must probe sockets when disconnected"
        );
    });
}

#[test]
fn open_port_validates_engine_supplied_hosts() {
    use crate::app_detail::port_url;

    assert_eq!(port_url("", 8080).as_deref(), Some("http://localhost:8080"));
    assert_eq!(
        port_url("0.0.0.0", 443).as_deref(),
        Some("https://localhost:443")
    );
    assert_eq!(
        port_url("::", 8443).as_deref(),
        Some("https://localhost:8443")
    );
    assert_eq!(
        port_url("127.0.0.1", 3000).as_deref(),
        Some("http://127.0.0.1:3000")
    );
    assert_eq!(port_url("::1", 8080).as_deref(), Some("http://[::1]:8080"));
    assert_eq!(
        port_url("[::1]", 8080).as_deref(),
        Some("http://[::1]:8080")
    );
    assert_eq!(
        port_url("my-host.local", 80).as_deref(),
        Some("http://my-host.local:80")
    );

    // Reject port 0 and any host carrying URL/shell/injection characters.
    for (host, port) in [
        ("127.0.0.1", 0),
        ("evil.com/path", 80),
        ("user@evil.com", 80),
        ("127.0.0.1?q=1", 80),
        ("127.0.0.1#frag", 80),
        ("bad host", 80),
        ("foo:9999", 80),
        ("-leading-dash", 80),
        ("trailing-dash-", 80),
        ("bad..host", 80),
    ] {
        assert_eq!(port_url(host, port), None, "{host}:{port}");
    }
}

#[gpui::test]
fn select_container_by_id_recomputes_rows_after_clearing_filter_and_search(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::testkit::fixture(cx);
    app.update(cx, |app, cx| {
        app.set_filter(Filter::Stopped, cx);
        app.view.lists[SearchKind::Containers as usize].query = "nothing-matches".into();
        // Populate `kept` with the empty filtered result.
        assert!(app.visible_rows().is_empty());

        app.select_container_by_id("demo-2", cx);
        assert_eq!(app.view.filter, Filter::All);
        assert_eq!(app.query(SearchKind::Containers), "");
        assert_eq!(app.view.selected_id.as_deref(), Some("demo-2"));
        assert_eq!(
            app.view.lists[SearchKind::Containers as usize]
                .kept
                .peek()
                .map(|rows| rows.len()),
            Some(2),
            "kept cache was refreshed so reveal could locate the unhidden row"
        );
    });
}
