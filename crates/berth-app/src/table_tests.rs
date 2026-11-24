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

    // A click in the lower half of the page, where only a scrolled list has rows.
    let (width, height) = cx.update(|window, _| {
        let size = window.viewport_size();
        (f32::from(size.width), f32::from(size.height))
    });
    let top = app.read_with(&*cx, |app, _| scrolled_rows(app));
    cx.simulate_click(
        point(px(width * 0.3), px(height * 0.8)),
        Modifiers::default(),
    );

    app.read_with(&*cx, |app, _| {
        let position = selected_position(app).expect("the click opened a row");
        assert!(
            (top..top + 30).contains(&position),
            "the click opened a row that is on screen: the {position}th of the list, with the list scrolled {top} rows down"
        );
    });
}
