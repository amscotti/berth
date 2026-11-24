//! What redraws when. The page and the inspector are views of their own
//! (`subviews`), so a change that only one of them shows redraws that one, a
//! change the shell announces redraws both, and a steady screen draws
//! nothing at all.

use berth_ui::theme;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::app::{BerthApp, InspectorTab, SearchKind};
use crate::subviews::Corner;
use crate::testkit::{self, redraw};

/// How many times each view has been drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Draws {
    shell: u32,
    page: u32,
    pane: u32,
}

fn draws(app: &Entity<BerthApp>, cx: &mut VisualTestContext) -> Draws {
    app.read_with(cx, |app, cx| Draws {
        shell: app.draws(None, cx),
        page: app.draws(Some(Corner::Page), cx),
        pane: app.draws(Some(Corner::Inspector), cx),
    })
}

/// Draw until nothing is dirty, and say where that left off.
fn settle(app: &Entity<BerthApp>, cx: &mut VisualTestContext) -> Draws {
    for _ in 0..3 {
        redraw(cx);
    }
    draws(app, cx)
}

/// An app with a row selected, so the page and the inspector are both on
/// screen, and drawn until still.
fn both_corners(cx: &mut TestAppContext) -> (Entity<BerthApp>, &mut VisualTestContext, Draws) {
    let (app, cx) = cx.add_window_view(|_, cx| BerthApp::new_fixture(theme::light(), cx));
    app.update(cx, |app, cx| app.select(0, cx));
    let settled = settle(&app, cx);
    assert!(
        settled.page > 0 && settled.pane > 0,
        "both corners were drawn to get here: {settled:?}"
    );
    (app, cx, settled)
}

#[gpui::test]
fn a_steady_screen_draws_nothing(cx: &mut TestAppContext) {
    let (app, cx, settled) = both_corners(cx);
    redraw(cx);
    redraw(cx);
    assert_eq!(
        draws(&app, cx),
        settled,
        "nothing changed, so nothing was drawn"
    );
}

#[gpui::test]
fn a_change_the_shell_announces_redraws_both(cx: &mut TestAppContext) {
    let (app, cx, before) = both_corners(cx);

    // A toast is the shell's: it is drawn over whichever corner is under it.
    app.update(cx, |app, cx| app.notify("Logs copied", false, cx));
    redraw(cx);

    let after = draws(&app, cx);
    assert!(
        after.shell > before.shell && after.page > before.page && after.pane > before.pane,
        "a change the shell announces reaches both: {before:?} then {after:?}"
    );
}

#[gpui::test]
fn selecting_a_row_brings_up_the_inspector(cx: &mut TestAppContext) {
    let (app, cx) = cx.add_window_view(|_, cx| BerthApp::new_fixture(theme::light(), cx));
    let closed = settle(&app, cx);
    assert_eq!(closed.pane, 0, "nothing selected, no inspector drawn");

    app.update(cx, |app, cx| app.select(0, cx));
    redraw(cx);

    let open = draws(&app, cx);
    assert!(
        open.pane > 0 && open.page > closed.page,
        "the page marks the row, and the inspector appears: {closed:?} then {open:?}"
    );
}

#[gpui::test]
fn a_search_edit_redraws_the_page_and_leaves_the_inspector(cx: &mut TestAppContext) {
    let (app, cx, _) = both_corners(cx);
    cx.update(|window, cx| {
        let search = app.read(cx).search(SearchKind::Containers).clone();
        search.update(cx, |input, cx| input.focus(window, cx));
    });
    // A first keystroke, so the window is already being typed into: the key
    // that follows a mouse movement makes GPUI redraw everything once.
    cx.simulate_input("w");
    cx.run_until_parked();
    let before = settle(&app, cx);

    cx.simulate_input("e");
    cx.run_until_parked();
    redraw(cx);

    let after = draws(&app, cx);
    assert_eq!(
        app.read_with(cx, |app, _| app.query(SearchKind::Containers).to_owned()),
        "we"
    );
    assert!(
        after.page > before.page,
        "the page shows the edit: {before:?} then {after:?}"
    );
    assert_eq!(
        after.pane, before.pane,
        "the inspector shows nothing of it: {before:?} then {after:?}"
    );
}

#[gpui::test]
fn the_bulk_selection_is_the_pages_to_show(cx: &mut TestAppContext) {
    let (app, cx, before) = both_corners(cx);

    let count = app.update(cx, |app, cx| {
        app.toggle_bulk(cx);
        app.bulk_select_all(cx);
        let first = app
            .view
            .bulk_selected
            .first()
            .cloned()
            .expect("a row on screen to select");
        let count = app.view.bulk_selected.len();
        app.bulk_toggle(&first, cx);
        count
    });
    redraw(cx);

    let after = draws(&app, cx);
    assert_eq!(
        app.read_with(cx, |app, _| app.view.bulk_selected.len()),
        count - 1,
        "selected by all, then one deselected by hand"
    );
    assert!(
        after.page > before.page,
        "the page shows it: {before:?} then {after:?}"
    );
    assert_eq!(
        after.pane, before.pane,
        "the inspector shows nothing of it: {before:?} then {after:?}"
    );
}

#[gpui::test]
fn a_log_read_redraws_the_inspector_and_leaves_the_page(cx: &mut TestAppContext) {
    let engine = testkit::scripted_engine();
    let (app, cx) = testkit::app_on(&engine, cx);
    app.update(cx, |app, cx| app.select(0, cx));
    cx.run_until_parked();
    let before = settle(&app, cx);

    // The logs read again, as the toolbar asks for it.
    app.update(cx, |app, cx| app.toggle_log_timestamps(cx));
    cx.run_until_parked();
    redraw(cx);

    let after = draws(&app, cx);
    assert!(app.read_with(cx, |app, _| app.inspector.logs_timestamps));
    assert!(
        after.pane > before.pane,
        "the inspector shows the read: {before:?} then {after:?}"
    );
    assert_eq!(
        after.page, before.page,
        "the page shows nothing of it: {before:?} then {after:?}"
    );

    // So for a change of the inspector's own page.
    let before = after;
    app.update(cx, |app, cx| {
        app.show_inspector_tab(InspectorTab::Inspect, cx)
    });
    redraw(cx);
    let after = draws(&app, cx);
    assert!(
        after.pane > before.pane,
        "the inspector changes its page: {before:?} then {after:?}"
    );
    assert_eq!(
        after.page, before.page,
        "the page shows nothing of it: {before:?} then {after:?}"
    );
}
