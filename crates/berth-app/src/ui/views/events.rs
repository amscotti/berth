//! Events: one row per thing the engine did while berth watched.

use berth_core::format;
use berth_engine::events::Event;
use berth_ui::layout::{self, Widths};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Role, div, px};

use crate::app::{BerthApp, SearchKind};
use crate::ui::table::{empty_state, header_cell, page_header, pill, text_cell};

pub fn events(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Events).to_ascii_lowercase();
    let mut shown: Vec<&Event> = app
        .snapshot
        .events
        .iter()
        .filter(|event| event.matches(&query))
        .collect();
    shown.sort_by_key(|event| std::cmp::Reverse(event.time_nano));

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
        content = content.child(empty_state(
            &format!("No results for “{}”", app.query(SearchKind::Events)),
            "Clear the search to see every event.",
            tokens,
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
    let mut header_row = div()
        .id("events-header")
        .role(Role::Row)
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .px(px(12.0))
        .pb(px(6.0))
        .gap(px(10.0))
        .child(header_cell("TIME", time, false, tokens))
        .child(header_cell("TYPE", kind, false, tokens))
        .child(header_cell("ACTION", action, false, tokens))
        .child(header_cell("ACTOR", actor, false, tokens));
    if detail > 0.0 {
        header_row = header_row.child(header_cell("DETAIL", detail, false, tokens));
    }
    content = content.child(
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header_row)
            .child(div().w_full().h(px(1.0)).bg(tokens.border)),
    );

    let rows = shown.into_iter().enumerate().map(|(index, event)| {
        let actor_id = event.actor.id.clone();
        let can_inspect =
            event.kind == "container" && app.snapshot.containers.iter().any(|c| c.id == actor_id);
        let mut row = div()
            .id(gpui::SharedString::from(format!(
                "event-{}-{}-{index}",
                event.time_nano, event.action
            )))
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(layout::ROW_EVENT))
            .px(px(12.0))
            .gap(px(10.0))
            .rounded(px(4.0))
            .bg(if index % 2 == 1 {
                tokens.surface_hover.alpha(0.35)
            } else {
                tokens.surface
            })
            .when(can_inspect, |this| {
                this.hover(move |style| style.bg(tokens.surface_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_container_by_id(&actor_id, cx);
                    }))
            })
            .role(Role::Row)
            .aria_label(event.name())
            .child(text_cell(format::clock(event.time), time, false, tokens))
            .child(text_cell(event.kind.clone(), kind, false, tokens))
            .child(pill(&event.action, tokens))
            .child(text_cell(event.name().to_owned(), actor, false, tokens));
        if detail > 0.0 {
            let text = event_detail_text(event);
            row = row.child(text_cell(text, detail, false, tokens));
        }
        row
    });
    content
        .child(
            div()
                .id("events-table")
                .flex()
                .flex_col()
                .w_full()
                .role(Role::Table)
                .aria_label("Events")
                .children(rows),
        )
        .into_any_element()
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
