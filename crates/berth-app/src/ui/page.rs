//! The frame every page is drawn in: its title, its search and its action,
//! and what it says when there is nothing to show.

use berth_core::format;
use berth_ui::layout::Widths;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, div, px};

use crate::app::{BerthApp, Filter, SearchKind, Tab};
use crate::ui::kit::{button, chip};

pub(crate) fn page_header(
    app: &BerthApp,
    widths: Widths,
    search_kind: Option<SearchKind>,
    action_label: Option<&'static str>,
    cx: &mut Context<BerthApp>,
) -> AnyElement {
    let tokens = app.tokens;
    let running = app.snapshot.running();
    let total = app.snapshot.containers.len();
    let subtitle = match app.view.tab {
        Tab::Containers => format!("{running} running · {total} total"),
        Tab::Overview => app.engine.engine_label(),
        Tab::Disk => app
            .snapshot
            .df
            .as_ref()
            .map(|_| "as of the last refresh".to_owned())
            .unwrap_or_else(|| "no disk figures yet".to_owned()),
        Tab::Images => format::count(app.snapshot.images.len(), "image", "images"),
        Tab::Volumes => format::count(app.snapshot.volumes.len(), "volume", "volumes"),
        Tab::Networks => format::count(app.snapshot.networks.len(), "network", "networks"),
        Tab::Events => format::count(app.snapshot.events.len(), "event", "events"),
        Tab::Stacks => format::count(app.snapshot.projects.len(), "project", "projects"),
        Tab::Runtimes => format::count(app.engine.reachable(), "engine", "engines"),
        Tab::Settings => format!(
            "berth {} · a native container monitor",
            env!("CARGO_PKG_VERSION")
        ),
    };

    let mut header = div()
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .gap(px(10.0))
        .pb(px(10.0))
        .child(
            div()
                .text_size(px(type_scale::TITLE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.text)
                .child(app.view.tab.label()),
        )
        .child(
            div()
                .text_size(px(type_scale::SECONDARY))
                .text_color(tokens.muted)
                .child(subtitle),
        )
        .child(div().flex_1());

    if let Some(kind) = search_kind {
        let search_width = if widths.content < 760.0 { 132.0 } else { 190.0 };
        if let Some(label) = action_label {
            header = header.child(button("page-action", label, tokens).on_click(cx.listener(
                |this, _, _, cx| match this.view.tab {
                    Tab::Containers => this.toggle_bulk(cx),
                    Tab::Volumes => this.open_create(crate::app::CreateKind::Volume, cx),
                    Tab::Networks => this.open_create(crate::app::CreateKind::Network, cx),
                    _ => {}
                },
            )));
        }
        let search = app.search(kind).clone();
        search.update(cx, |input, _| input.set_width(search_width));
        header = header.child(search);
    }

    // The chip that is on is the one that looks switched on: a wash of the
    // accent in the accent's own text colour. The others are plain surfaces.
    let chips = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .pb(px(10.0))
        .children(Filter::ALL.into_iter().map(|filter| {
            chip(
                ("filter", filter as usize),
                filter.label(),
                app.view.filter == filter,
                tokens,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_filter(filter, cx)))
        }));

    if app.view.tab == Tab::Containers {
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header)
            .child(chips)
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header)
            .into_any_element()
    }
}

pub(crate) fn empty_state(heading: &str, detail: &str, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .w_full()
        .py(px(48.0))
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(type_scale::BODY))
                .text_color(tokens.text)
                .child(heading.to_owned()),
        )
        .child(
            div()
                .text_size(px(type_scale::SECONDARY))
                .text_color(tokens.muted)
                .child(detail.to_owned()),
        )
}
