//! Events: one row per thing the engine did while berth watched.

use berth_core::format;
use berth_engine::events::Event;
use berth_ui::layout::{self, Widths};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px};

use crate::app::{BerthApp, SearchKind};
use crate::ui::data_table::{Column, DataTable, RowKey, pill_cell, text_cell};
use crate::ui::page::{empty_state, no_search_results, page_header};

pub fn events(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Events).to_ascii_lowercase();
    // Worked out once per snapshot and query, and kept from one frame to the
    // next while those hold. The list is newest first, as the engine tells it.
    let shown = app.rows_of(SearchKind::Events, || {
        let mut shown: Vec<usize> = app
            .snapshot
            .events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.matches(&query))
            .map(|(index, _)| index)
            .collect();
        shown.sort_by_key(|&index| std::cmp::Reverse(app.snapshot.events[index].time_nano));
        shown
    });

    let header = page_header(app, widths, Some(SearchKind::Events), None, cx);
    let mut content = div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .child(header);

    if app.snapshot.events.is_empty() {
        let (heading, detail) = if app.snapshot.events_error {
            (
                "Could not read events",
                "The engine answered, but /events did not.",
            )
        } else if !app.engine.error.is_empty() {
            ("Engine not answering", app.engine.error.as_str())
        } else {
            (
                "No events yet",
                "Exits, restarts, OOMs and cleanups land here.",
            )
        };
        content = content.child(empty_state(heading, detail, tokens));
        return content.into_any_element();
    }
    if shown.is_empty() {
        content = content.child(no_search_results(
            app.query(SearchKind::Events),
            "Clear the search to see every event.",
            SearchKind::Events,
            tokens,
            cx,
        ));
        return content.into_any_element();
    }

    let full = widths.content >= 834.0;
    let (time, kind, action, actor) = if full {
        (90.0, 90.0, 110.0, 220.0)
    } else {
        (80.0, 70.0, 100.0, 160.0)
    };
    let detail = if full {
        (widths.content - time - kind - action - actor - 74.0).max(180.0)
    } else {
        0.0
    };
    let mut table = DataTable::new(
        "events-table",
        "Events",
        "events-header",
        4,
        row_key,
        open_row,
    )
    .column(Column::plain("time", "TIME", time, false, time_cell))
    .column(Column::plain("type", "TYPE", kind, false, kind_cell))
    .column(Column::plain(
        "action",
        "ACTION",
        action,
        false,
        action_cell,
    ))
    .column(Column::plain("actor", "ACTOR", actor, false, actor_cell));
    if detail > 0.0 {
        table = table.column(Column::plain(
            "detail",
            "DETAIL",
            detail,
            false,
            detail_cell,
        ));
    }
    let table = table.hairline();
    let header = table.header(app, tokens, cx);
    let body = table.body(
        shown,
        layout::ROW_EVENT,
        &app.view.lists[SearchKind::Events as usize].scroll,
        tokens,
        cx,
    );
    content.child(header).child(body).into_any_element()
}

fn row_key(app: &BerthApp, ix: usize) -> RowKey {
    let event = &app.snapshot.events[ix];
    // An event about a container opens that container, while it is still
    // there to open.
    let opens = event.kind == "container"
        && app
            .snapshot
            .containers
            .iter()
            .any(|c| c.id == event.actor.id);
    RowKey {
        id: SharedString::from(format!("event-{}-{}-{ix}", event.time_nano, event.action)),
        name: event.name().to_owned(),
        selected: false,
        opens,
    }
}

fn open_row(app: &mut BerthApp, ix: usize, cx: &mut Context<BerthApp>) {
    let actor_id = app.snapshot.events[ix].actor.id.clone();
    app.select_container_by_id(&actor_id, cx);
}

fn time_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let event = &app.snapshot.events[ix];
    text_cell(format::clock(event.time), width, false, app.tokens).into_any_element()
}

fn kind_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let event = &app.snapshot.events[ix];
    text_cell(event.kind.clone(), width, false, app.tokens).into_any_element()
}

fn action_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let event = &app.snapshot.events[ix];
    pill_cell(&event.action, width, app.tokens).into_any_element()
}

fn actor_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let event = &app.snapshot.events[ix];
    text_cell(event.name().to_owned(), width, false, app.tokens).into_any_element()
}

fn detail_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let event = &app.snapshot.events[ix];
    text_cell(event_detail_text(event), width, false, app.tokens).into_any_element()
}

fn event_detail_text(event: &Event) -> String {
    if let Some(code) = event.actor.attributes.get("exitCode")
        && let Some(image) = event.actor.attributes.get("image")
    {
        return format!("{image} · exit {code}");
    }
    if let Some(code) = event.actor.attributes.get("exitCode") {
        return format!("exit {code}");
    }
    if let Some(image) = event.actor.attributes.get("image")
        && !image.is_empty()
    {
        return image.clone();
    }
    if !event.from.is_empty() && event.from != event.name() {
        return event.from.clone();
    }
    let short: String = event.actor.id.chars().take(12).collect();
    if !short.is_empty() && short != event.name() {
        short
    } else {
        "—".to_owned()
    }
}
