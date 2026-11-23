//! Compose projects: containers grouped by their project label.

use berth_engine::projects::Summary;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, px};

use crate::app::BerthApp;
use crate::ui::kit::{button, caption, card, card_title};
use crate::ui::table::{empty_state, page_header, pill};

pub fn stacks(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let header = page_header(app, widths, None, None, cx);
    if app.snapshot.projects.is_empty() {
        return div()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .child(header)
            .child(empty_state(
                "No Compose stacks",
                "Containers started by docker compose appear here, grouped by project.",
                tokens,
            ))
            .into_any_element();
    }

    let projects = app.snapshot.projects.clone();
    let mut cards = Vec::with_capacity(projects.len());
    for project in &projects {
        cards.push(project_card(app, project, tokens, cx));
    }
    div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .gap(px(10.0))
        .child(header)
        .children(cards)
        .into_any_element()
}

fn project_card(
    app: &BerthApp,
    project: &Summary,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> AnyElement {
    // One small button per thing a stack can be asked to do as a whole.
    let whole = |label: &'static str| {
        button(
            SharedString::from(format!("stack-{}-{label}", project.name)),
            label,
            tokens,
        )
        .small()
    };
    let (start, restart, stop) = (
        project.name.clone(),
        project.name.clone(),
        project.name.clone(),
    );
    let mut card = card(tokens).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .gap(px(layout::ROW_GAP))
            .child(card_title(project.name.clone(), tokens))
            .child(caption(
                format!("{}/{} running", project.running, project.total),
                tokens,
            ))
            .child(div().flex_1())
            // Starting a stack is the way forward.
            .when(project.running < project.total, |this| {
                this.child(whole("Start all").primary().on_click(
                    cx.listener(move |this, _, _, cx| this.project_action(&start, true, cx)),
                ))
            })
            .when(project.running > 0, |this| {
                this.child(whole("Restart all").on_click(
                    cx.listener(move |this, _, _, cx| this.project_restart(&restart, cx)),
                ))
                // Stopping a stack takes every container in it down.
                .child(whole("Stop all").danger_wash().on_click(
                    cx.listener(move |this, _, _, cx| this.project_action(&stop, false, cx)),
                ))
            }),
    );

    for index in &project.members {
        let Some(container) = app.snapshot.containers.get(*index) else {
            continue;
        };
        let stat = app.snapshot.stat(*index);
        let click_id = container.id.clone();
        let state = container.health().unwrap_or(&container.state).to_owned();
        let ports = container.ports_text(2);
        let metrics = if container.is_up() && stat.sampled {
            let cpu = if stat.cpu_known {
                berth_core::format::percent(stat.cpu)
            } else {
                "—".to_owned()
            };
            let mem = if stat.mem_known {
                berth_core::format::bytes(stat.mem)
            } else {
                "—".to_owned()
            };
            format!("{cpu} · {mem}")
        } else {
            String::new()
        };
        card = card.child(
            // A member opens its container. Until the page has a table of
            // its own, with arrows to move in it, each is a stop for Tab.
            crate::ui::kit::focusable(
                div().id(SharedString::from(format!("stack-member-{}", container.id))),
                tokens.focus_ring(),
            )
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(32.0))
            .px(px(6.0))
            .gap(px(layout::ROW_GAP))
            // The hairline that the focus ring is drawn in.
            .border_1()
            .border_color(tokens.invisible())
            .rounded(px(layout::RADIUS_ROW))
            .hover(move |style| style.bg(tokens.surface_hover))
            .role(gpui::Role::Row)
            .aria_label(container.name())
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(index) = this
                    .snapshot
                    .containers
                    .iter()
                    .position(|candidate| candidate.id == click_id)
                {
                    this.select(index, cx);
                }
            }))
            .child(
                div()
                    .w(px(110.0))
                    .truncate()
                    .text_size(px(type_scale::SECONDARY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(tokens.text)
                    .child(container.service().unwrap_or(container.name()).to_owned()),
            )
            .child(
                div()
                    .w(px(150.0))
                    .truncate()
                    .text_size(px(type_scale::SECONDARY))
                    .text_color(tokens.muted)
                    .child(container.name().to_owned()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(type_scale::MONO))
                    .font_family(type_scale::MONO_FAMILY)
                    .text_color(tokens.muted)
                    .child(container.image.clone()),
            )
            .when(!ports.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .h(px(layout::PILL_HEIGHT))
                        .px(px(layout::PILL_PADDING))
                        .rounded(px(layout::PILL_HEIGHT / 2.0))
                        .bg(tokens.pill_wash(tokens.warning_fill))
                        .child(
                            div()
                                .text_size(px(type_scale::LABEL))
                                .font_features(crate::ui::tabular())
                                .text_color(tokens.warning_text())
                                .child(ports),
                        ),
                )
            })
            .when(!metrics.is_empty(), |this| {
                this.child(
                    div()
                        .w(px(120.0))
                        .flex()
                        .justify_end()
                        .text_size(px(type_scale::LABEL))
                        .font_features(crate::ui::tabular())
                        .text_color(tokens.muted)
                        .child(metrics),
                )
            })
            .child(pill(&state, tokens)),
        );
    }
    card.into_any_element()
}
