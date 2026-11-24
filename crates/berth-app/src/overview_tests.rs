//! The Overview: it fits its window, and it says how the engine is with no
//! more than the state needs.

use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

use crate::app::{BerthApp, Tab};
use crate::testkit;
use crate::ui::views::overview::{Health, health};

/// The fixture on its Overview page, in a window of `width` by `height`.
fn overview_in(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (Entity<BerthApp>, &mut VisualTestContext) {
    let (app, cx) = testkit::fixture(cx);
    app.update(cx, |app, cx| app.switch_to(Tab::Overview, cx));
    cx.simulate_resize(size(px(width), px(height)));
    testkit::redraw(cx);
    (app, cx)
}

#[gpui::test]
fn the_overview_fits_its_window(cx: &mut TestAppContext) {
    let (app, cx) = overview_in(cx, 1280.0, 760.0);
    app.read_with(&*cx, |app, _| {
        assert_eq!(health(app, true, false), Health::Healthy);
        let to_scroll = app.view.page_scroll.max_offset().y;
        assert!(
            to_scroll <= px(0.0),
            "with all well, the page fits a 760 pt window: {to_scroll:?} to scroll"
        );
    });
}

#[gpui::test]
fn only_an_engine_with_something_to_say_gets_a_band(cx: &mut TestAppContext) {
    let (app, cx) = testkit::fixture(cx);
    app.read_with(&*cx, |app, _| {
        assert_eq!(health(app, true, false), Health::Healthy);
        assert_eq!(health(app, false, true), Health::Reading);
        assert_eq!(health(app, false, false), Health::NotAnswering);
    });
    // A complaint outranks the rest, even from an engine that is answering.
    app.update(cx, |app, _| {
        app.engine.error = "engine went away".to_owned()
    });
    app.read_with(&*cx, |app, _| {
        assert_eq!(
            health(app, true, false),
            Health::Warning("engine went away".to_owned())
        );
    });
}
