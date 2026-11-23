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
