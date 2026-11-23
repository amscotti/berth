//! Tests for where keyboard focus goes: the order Tab crosses the page, and
//! the hold a sheet keeps on the keyboard while it is open.

use gpui::{Entity, FocusHandle, Focusable, TestAppContext, VisualTestContext};

use crate::app::{BerthApp, Filter, SearchKind, Tab};
use crate::testkit;

/// Whether `handle` has focus.
fn focused(cx: &mut VisualTestContext, handle: &FocusHandle) -> bool {
    cx.update(|window, _| handle.is_focused(window))
}

/// Whether focus is on one of the open sheet's controls.
fn in_sheet(app: &Entity<BerthApp>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        let app = app.read(cx);
        app.sheet_focus.holds_focus(app, cx, window)
    })
}

/// The key that shows the first page, on this platform.
fn page_one() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-1"
    } else {
        "ctrl-1"
    }
}

#[gpui::test]
fn tab_crosses_the_rail_the_search_the_chips_and_the_headers_before_the_table(
    cx: &mut TestAppContext,
) {
    let (app, cx) = testkit::fixture(cx);
    let (search, table) = app.read_with(&*cx, |app, cx| {
        (
            app.search(SearchKind::Containers).focus_handle(cx),
            app.table_focus.clone(),
        )
    });
    // How many columns this window shows, from the same arithmetic the page
    // uses to lay them out.
    let headers = cx.update(|window, _| {
        let width: f32 = window.viewport_size().width.into();
        let content = berth_ui::layout::for_window(width, false).content;
        berth_ui::layout::container_columns(content, false).header_count()
    });

    // The first stop is the rail's first page: Enter on it goes there.
    testkit::press(cx, "tab");
    testkit::press(cx, "enter");
    assert_eq!(app.read_with(&*cx, |app, _| app.tab), Tab::Overview);
    app.update(cx, |app, cx| app.switch_to(Tab::Containers, cx));

    // Then one stop for each of the rail's other pages, the page's action,
    // and the search.
    for _ in 1..Tab::ALL.len() {
        testkit::press(cx, "tab");
    }
    testkit::press(cx, "tab");
    testkit::press(cx, "tab");
    assert!(focused(cx, &search), "the search follows the rail");
    // It is a field: what is typed lands in it, and Escape empties it.
    cx.simulate_input("web");
    assert_eq!(
        app.read_with(&*cx, |app, _| app.query(SearchKind::Containers).to_owned()),
        "web"
    );
    testkit::press(cx, "escape");
    assert_eq!(
        app.read_with(&*cx, |app, _| app.query(SearchKind::Containers).to_owned()),
        ""
    );

    // Then the filter chips, in order: two on from the search is Running,
    // and Enter on it filters.
    testkit::press(cx, "tab");
    testkit::press(cx, "tab");
    testkit::press(cx, "enter");
    assert_eq!(app.read_with(&*cx, |app, _| app.filter), Filter::Running);

    // Then the headers, which sort. Name is sorted ascending to begin with;
    // Enter on its header turns that round.
    testkit::press(cx, "tab");
    testkit::press(cx, "tab");
    testkit::press(cx, "enter");
    assert_eq!(app.read_with(&*cx, |app, _| app.sort_for(0)), (0, false));

    // One stop for each of the other headers, then the table: one stop for
    // every row it holds.
    for _ in 0..headers {
        testkit::press(cx, "tab");
    }
    assert!(focused(cx, &table), "the table follows its headers");

    // A row's own actions are not stops: Tab leaves the table for the top of
    // the window, which is the rail's first page.
    testkit::press(cx, "tab");
    testkit::press(cx, "enter");
    assert_eq!(app.read_with(&*cx, |app, _| app.tab), Tab::Overview);
    app.update(cx, |app, cx| app.switch_to(Tab::Containers, cx));

    // Back to the table, where the arrow keys move the selection: past the
    // rest of the rail, the page's action, the search, the chips, the
    // headers, and on to the table itself.
    let to_table = (Tab::ALL.len() - 1) + 1 + 1 + Filter::ALL.len() + headers + 1;
    for _ in 0..to_table {
        testkit::press(cx, "tab");
    }
    assert!(focused(cx, &table), "back at the table");
    testkit::press(cx, "down");
    let first = app.read_with(&*cx, |app, _| {
        let rows = app.visible_rows();
        app.snapshot.containers[rows[0]].id.clone()
    });
    assert_eq!(
        app.read_with(&*cx, |app, _| app.selected_id.clone()),
        Some(first),
        "Down selects the first row"
    );
}

#[gpui::test]
fn a_sheet_holds_the_keyboard_until_it_closes(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    let search = app.read_with(&*cx, |app, cx| {
        app.search(SearchKind::Containers).focus_handle(cx)
    });

    // Focus is on the page when the sheet opens.
    testkit::tab_until(cx, |cx| focused(cx, &search), Tab::ALL.len() + 2);
    assert!(focused(cx, &search), "the search is focused first");
    app.update(cx, |app, cx| app.open_prune(cx));
    testkit::draw(cx);

    // It moves into the sheet, to the first control it lists.
    let first = app.read_with(&*cx, |app, _| app.sheet_focus.options[0].clone());
    assert!(focused(cx, &first), "the sheet takes focus");

    // Tab crosses the sheet's controls and nothing else: every one of them,
    // wrapping at each end.
    let stops = app.read_with(&*cx, |app, cx| app.sheet_focus.stops(app, cx));
    assert!(stops.len() >= 3, "a cleanup offers options and two buttons");
    let which = |cx: &mut VisualTestContext| {
        stops
            .iter()
            .position(|handle| focused(cx, handle))
            .expect("focus is on a sheet control")
    };
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..stops.len() {
        testkit::press(cx, "tab");
        assert!(in_sheet(&app, cx), "Tab stayed in the sheet");
        seen.insert(which(cx));
    }
    assert_eq!(seen.len(), stops.len(), "Tab reached every control");
    for _ in 0..stops.len() {
        testkit::press(cx, "shift-tab");
        assert!(in_sheet(&app, cx), "Shift+Tab stayed in the sheet");
    }

    // The page behind gets none of its own keys.
    testkit::press(cx, "down");
    assert!(
        app.read_with(&*cx, |app, _| app.selected_id.is_none()),
        "the arrows don't reach the table behind a sheet"
    );
    testkit::press(cx, page_one());
    assert_eq!(
        app.read_with(&*cx, |app, _| app.tab),
        Tab::Containers,
        "the page shortcuts don't switch pages behind a sheet"
    );

    // Escape is the way out, and focus goes back to where it was.
    testkit::press(cx, "escape");
    assert!(!app.read_with(&*cx, |app, _| app.prune_open));
    testkit::draw(cx);
    assert!(focused(cx, &search), "focus returns to where it was");
}

#[gpui::test]
fn a_confirmation_opens_on_the_way_out(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    app.update(cx, |app, cx| app.confirm_first_delete(cx));
    testkit::draw(cx);

    // The destructive button is not where focus lands: Cancel is.
    let cancel = app.read_with(&*cx, |app, _| app.sheet_focus.cancel.clone());
    assert!(focused(cx, &cancel), "a confirmation opens on Cancel");
    testkit::press(cx, "enter");
    assert!(
        app.read_with(&*cx, |app, _| app.pending.is_none()),
        "Enter there closes the sheet and does nothing else"
    );
    assert_eq!(
        app.read_with(&*cx, |app, _| app.snapshot.containers.len()),
        crate::state::fixture_snapshot().containers.len(),
        "nothing was deleted"
    );
}
