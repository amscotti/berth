//! A table draws the rows the window shows, and no more; and the rows it
//! draws are the right ones wherever the list has scrolled to.

use berth_ui::layout;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point, px};

use crate::app::{BerthApp, SearchKind};
use crate::testkit;

/// An app on a fleet of `count` containers, in a window, drawn once.
fn fleet(cx: &mut TestAppContext, count: usize) -> (Entity<BerthApp>, &mut VisualTestContext) {
    cx.update(|cx| berth_commands::install_keymap(cx, berth_commands::Platform::current()));
    let (app, cx) = cx.add_window_view(|_, cx| {
        berth_ui::theme::ActiveTokens::set(berth_ui::theme::light(), cx);
        BerthApp::with_fixture(
            berth_ui::theme::light(),
            crate::fixture_snapshot_with(count),
            cx,
        )
    });
    (app, cx)
}

/// How far down the containers list has scrolled, in rows.
fn scrolled_rows(app: &BerthApp) -> usize {
    let offset = app.view.lists[SearchKind::Containers as usize]
        .scroll
        .0
        .borrow()
        .base_handle
        .offset();
    (-offset.y / px(layout::ROW_CONTAINER)).floor() as usize
}

/// Where the selected container sits among the rows on screen.
fn selected_position(app: &BerthApp) -> Option<usize> {
    let selected = app.view.selected_id.as_deref()?;
    app.visible_rows()
        .iter()
        .position(|&ix| app.snapshot.containers[ix].id == selected)
}

#[gpui::test]
fn a_table_draws_the_rows_the_window_shows_and_no_more(cx: &mut TestAppContext) {
    let (app, cx) = fleet(cx, 500);
    assert_eq!(
        app.read_with(&*cx, |app, _| app.visible_rows().len()),
        500,
        "every container is on the list"
    );
    // A frame after a change: what a commit costs the table.
    app.update(cx, |app, cx| {
        app.view.rows_drawn = 0;
        cx.notify();
    });
    testkit::redraw(cx);

    let drawn = app.read_with(&*cx, |app, _| app.view.rows_drawn);
    let height = cx.update(|window, _| f32::from(window.viewport_size().height));
    // The whole window holds that many rows, were the table all of it, with
    // one more measured to find their height.
    let window_holds = (height / layout::ROW_CONTAINER).ceil() as usize + 1;
    assert!(drawn > 0, "rows were drawn");
    assert!(
        drawn <= window_holds,
        "{drawn} rows were drawn for one frame of a 500-row table, in a window that holds {window_holds} at most"
    );
}

#[gpui::test]
fn the_arrows_reach_past_the_window_and_bring_the_row_back(cx: &mut TestAppContext) {
    let (app, cx) = fleet(cx, 500);
    testkit::draw(cx);
    for _ in 0..40 {
        testkit::press(cx, "down");
    }
    testkit::draw(cx);

    app.read_with(&*cx, |app, _| {
        let position = selected_position(app).expect("a row is selected");
        assert_eq!(position, 39, "forty presses down reach the fortieth row");
        assert!(
            scrolled_rows(app) > 0,
            "the list followed the selection down: {} rows",
            scrolled_rows(app)
        );
        assert!(
            position >= scrolled_rows(app),
            "the selected row is on screen, not above the window"
        );
    });

    // And back up: the top of the list is where it was.
    for _ in 0..40 {
        testkit::press(cx, "up");
    }
    testkit::draw(cx);
    app.read_with(&*cx, |app, _| {
        assert_eq!(selected_position(app), Some(0));
        assert_eq!(scrolled_rows(app), 0, "the list followed the selection up");
    });
}

#[gpui::test]
fn a_row_clicked_where_the_list_has_scrolled_to_opens_that_row(cx: &mut TestAppContext) {
    let (app, cx) = fleet(cx, 500);
    testkit::draw(cx);
    for _ in 0..40 {
        testkit::press(cx, "down");
    }
    testkit::draw(cx);

    // A click in the middle of the scrolled viewport must hit a different
    // visible row than the 40th row already selected at the bottom.
    let (width, height) = cx.update(|window, _| {
        let size = window.viewport_size();
        (f32::from(size.width), f32::from(size.height))
    });
    let (top, before) = app.read_with(&*cx, |app, _| (scrolled_rows(app), selected_position(app)));
    let visible_rows = (height / layout::ROW_CONTAINER).ceil() as usize;
    cx.simulate_click(
        point(px(width * 0.3), px(height * 0.45)),
        Modifiers::default(),
    );

    app.read_with(&*cx, |app, _| {
        let position = selected_position(app).expect("the click opened a row");
        assert_ne!(
            Some(position),
            before,
            "the click actually moved selection from {before:?} to a newly clicked row"
        );
        assert!(
            (top..top + visible_rows).contains(&position),
            "the click opened a row that is on screen: the {position}th of the list, with the list scrolled {top} rows down"
        );
    });
}

#[gpui::test]
fn arrow_navigation_works_across_all_five_tables(cx: &mut TestAppContext) {
    use crate::app::{DetailKind, Tab};

    let (app, cx) = testkit::fixture(cx);
    testkit::draw(cx);

    // 1. Containers
    testkit::press(cx, "down");
    assert_eq!(
        app.read_with(&*cx, |app, _| (
            app.view.detail_kind,
            app.view.selected_id.clone()
        )),
        (DetailKind::Container, Some("demo-1".into()))
    );

    // 2. Images
    app.update(cx, |app, cx| app.switch_to(Tab::Images, cx));
    testkit::draw(cx);
    testkit::press(cx, "down");
    let first_image = app.read_with(&*cx, |app, _| app.view.selected_id.clone());
    assert!(first_image.is_some());
    assert_eq!(
        app.read_with(&*cx, |app, _| app.view.detail_kind),
        DetailKind::Image
    );
    testkit::press(cx, "down");
    let second_image = app.read_with(&*cx, |app, _| app.view.selected_id.clone());
    assert_ne!(first_image, second_image, "Down advances in Images");
    testkit::press(cx, "up");
    assert_eq!(
        app.read_with(&*cx, |app, _| app.view.selected_id.clone()),
        first_image,
        "Up moves back in Images"
    );

    // 3. Volumes
    app.update(cx, |app, cx| app.switch_to(Tab::Volumes, cx));
    testkit::draw(cx);
    testkit::press(cx, "down");
    assert_eq!(
        app.read_with(&*cx, |app, _| (
            app.view.detail_kind,
            app.view.selected_id.clone()
        )),
        (DetailKind::Volume, Some("shop-data".into()))
    );

    // 4. Networks
    app.update(cx, |app, cx| app.switch_to(Tab::Networks, cx));
    testkit::draw(cx);
    testkit::press(cx, "down");
    let first_net = app.read_with(&*cx, |app, _| app.view.selected_id.clone());
    assert!(first_net.is_some());
    assert_eq!(
        app.read_with(&*cx, |app, _| app.view.detail_kind),
        DetailKind::Network
    );
    testkit::press(cx, "down");
    assert_ne!(
        app.read_with(&*cx, |app, _| app.view.selected_id.clone()),
        first_net,
        "Down advances in Networks"
    );

    // 5. Events: Down moves cursor, Enter opens container event on Containers tab.
    app.update(cx, |app, cx| app.switch_to(Tab::Events, cx));
    testkit::draw(cx);
    testkit::press(cx, "down");
    let cursor = app.read_with(&*cx, |app, _| app.view.event_cursor);
    assert!(cursor.is_some(), "Down sets event_cursor in Events");
}

#[gpui::test]
fn space_toggles_selected_container_in_bulk_mode_only_on_table(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    testkit::draw(cx);

    app.update(cx, |app, cx| {
        app.toggle_bulk(cx);
        app.select(0, cx);
    });
    testkit::draw(cx);

    // Space toggles the selected row's bulk checkbox on and off.
    testkit::press(cx, "space");
    assert!(
        app.read_with(&*cx, |app, _| app.view.bulk_selected.contains("demo-1")),
        "Space checks the selected container in bulk mode"
    );
    testkit::press(cx, "space");
    assert!(
        app.read_with(&*cx, |app, _| !app.view.bulk_selected.contains("demo-1")),
        "Space unchecks it on a second press"
    );

    // When focus is inside the search box, Space must NOT toggle bulk selection.
    cx.update(|window, cx| {
        let search = app.read(cx).search(SearchKind::Containers).clone();
        search.update(cx, |input, cx| input.focus(window, cx));
    });
    testkit::press(cx, "space");
    assert!(
        app.read_with(&*cx, |app, _| app.view.bulk_selected.is_empty()),
        "Space inside search does not toggle bulk selection"
    );
}
