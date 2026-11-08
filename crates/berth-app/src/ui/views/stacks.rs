//! Compose projects: containers grouped by their project label.

use berth_engine::projects::Summary;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, px};

use crate::app::BerthApp;
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
    let mut card = div()
        .id(SharedString::from(format!("project-{}", project.name)))
        .flex()
        .flex_col()
        .w_full()
        .gap(px(6.0))
        .p(px(14.0))
        .rounded(px(layout::CARD_RADIUS))
        .bg(tokens.surface)
        .border_1()
        .border_color(tokens.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .child(project.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(tokens.muted)
                        .child(format!("{}/{} running", project.running, project.total)),
                )
                .child(div().flex_1())
                .when(project.running < project.total, |this| {
                    this.child(small_button(
                        &project.name,
                        "Start all",
                        true,
                        tokens,
                        cx.listener({
                            let name = project.name.clone();
                            move |this, _, _, cx| this.project_action(&name, true, cx)
                        }),
                    ))
                })
                .when(project.running > 0, |this| {
                    this.child(small_button(
                        &project.name,
                        "Restart all",
                        false,
                        tokens,
                        cx.listener({
                            let name = project.name.clone();
                            move |this, _, _, cx| this.project_restart(&name, cx)
                        }),
                    ))
                    .child(small_button(
                        &project.name,
                        "Stop all",
                        false,
                        tokens,
                        cx.listener({
                            let name = project.name.clone();
                            move |this, _, _, cx| this.project_action(&name, false, cx)
                        }),
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
            div()
                .id(SharedString::from(format!("stack-member-{}", container.id)))
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .h(px(32.0))
                .px(px(6.0))
                .gap(px(10.0))
                .rounded(px(4.0))
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
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(tokens.text)
                        .child(container.service().unwrap_or(container.name()).to_owned()),
                )
                .child(
                    div()
                        .w(px(150.0))
                        .truncate()
                        .text_size(px(11.5))
                        .text_color(tokens.muted)
                        .child(container.name().to_owned()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(11.0))
                        .font_family("Menlo")
                        .text_color(tokens.muted)
                        .child(container.image.clone()),
                )
                .when(!ports.is_empty(), |this| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .h(px(20.0))
                            .px(px(7.0))
                            .rounded(px(10.0))
                            .bg(tokens.warning_fill.alpha(0.14))
                            .child(
                                div()
                                    .text_size(px(10.5))
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
                            .text_size(px(11.0))
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

fn small_button(
    project: &str,
    label: &'static str,
    primary: bool,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("stack-{project}-{label}")))
        .flex()
        .items_center()
        .h(px(24.0))
        .px(px(10.0))
        .rounded(px(12.0))
        .bg(if primary {
            tokens.accent
        } else {
            tokens.surface_hover
        })
        .role(gpui::Role::Button)
        .aria_label(label)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(11.5))
                .text_color(if primary {
                    tokens.accent_text
                } else {
                    tokens.text
                })
                .child(label),
        )
}
