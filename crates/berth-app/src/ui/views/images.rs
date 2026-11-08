//! Images: repository, short ID, size, age and dangling/ready state.

use berth_core::format;
use berth_engine::dto::Image;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Role, SharedString, div, px};

use crate::app::{BerthApp, SearchKind};
use crate::ui::table::{empty_state, page_header, pill, sortable_header_cell, text_cell};
use berth_core::validation;

pub fn images(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Images).to_ascii_lowercase();
    let mut shown: Vec<&Image> = app
        .snapshot
        .images
        .iter()
        .filter(|image| image_matches(image, &query))
        .collect();
    let (sort_col, sort_asc) = app.sort_for(1);
    shown.sort_by(|a, b| {
        let ord = match sort_col {
            1 => a.id.cmp(&b.id),
            2 => a.size.cmp(&b.size),
            3 => a.created.cmp(&b.created),
            4 => a.dangling().cmp(&b.dangling()),
            _ => a.label().cmp(b.label()),
        };
        if sort_asc { ord } else { ord.reverse() }
    });

    let header = page_header(app, widths, Some(SearchKind::Images), None, cx);
    let mut content = div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .child(header)
        .child(creation_card(app, tokens, cx));

    if app.snapshot.images.is_empty() {
        content = content.child(empty_state(
            "No images",
            "Pull a reference from the Images card, or run a container.",
            tokens,
        ));
        return content.into_any_element();
    }
    if shown.is_empty() {
        content = content.child(empty_state(
            &format!("No results for “{}”", app.query(SearchKind::Images)),
            "Clear the search to see every image.",
            tokens,
        ));
        return content.into_any_element();
    }

    let full = widths.content >= 824.0;
    let (id, size, age, in_use, status) = if full {
        (120.0, 90.0, 90.0, 75.0, 90.0)
    } else {
        (90.0, 75.0, 0.0, 0.0, 75.0)
    };
    let fixed = id + size + age + in_use + status + if full { 74.0 } else { 54.0 };
    let repo = (widths.content - fixed).clamp(170.0, 380.0);
    let mut header_row = div()
        .id("images-header")
        .role(Role::Row)
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .px(px(12.0))
        .pb(px(6.0))
        .gap(px(10.0));
    let sort = (sort_col, sort_asc);
    header_row = header_row
        .child(sortable_header_cell(
            1,
            0,
            "REPOSITORY",
            repo,
            false,
            sort,
            tokens,
            cx,
        ))
        .child(sortable_header_cell(
            1, 1, "IMAGE ID", id, false, sort, tokens, cx,
        ))
        .child(sortable_header_cell(
            1, 2, "SIZE", size, true, sort, tokens, cx,
        ));
    if age > 0.0 {
        header_row = header_row.child(sortable_header_cell(
            1, 3, "CREATED", age, true, sort, tokens, cx,
        ));
    }
    if in_use > 0.0 {
        header_row = header_row.child(crate::ui::table::header_cell(
            "IN USE", in_use, true, tokens,
        ));
    }
    header_row = header_row.child(sortable_header_cell(
        1, 4, "STATUS", status, false, sort, tokens, cx,
    ));
    content = content.child(
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(header_row)
            .child(div().w_full().h(px(1.0)).bg(tokens.border)),
    );

    let rows = shown.into_iter().enumerate().map(|(index, image)| {
        let label = if image.dangling() {
            "untagged"
        } else {
            image.tag()
        };
        let used_count = app
            .snapshot
            .containers
            .iter()
            .filter(|c| c.uses_image(image))
            .count();
        let selected = app.detail_kind == crate::app::DetailKind::Image
            && app.selected_id.as_deref() == Some(image.id.as_str());
        let click_id = image.id.clone();
        let click_label = label.to_owned();
        let mut row = div()
            .id(SharedString::from(format!("image-{}", image.id)))
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(layout::ROW_CONTAINER))
            .px(px(12.0))
            .gap(px(10.0))
            .rounded(px(4.0))
            .bg(if selected {
                tokens.accent.alpha(0.10)
            } else if index % 2 == 1 {
                tokens.surface_hover.alpha(0.35)
            } else {
                tokens.surface
            })
            .when(!selected, |this| {
                this.hover(move |style| style.bg(tokens.surface_hover))
            })
            .role(Role::Row)
            .aria_label(label)
            .aria_selected(selected)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_image(&click_id, &click_label, cx);
            }));
        row = row
            .child(text_cell(label.to_owned(), repo, false, tokens))
            .child(text_cell(short_id(&image.id), id, false, tokens))
            .child(text_cell(format::bytes(image.size), size, true, tokens));
        if age > 0.0 {
            row = row.child(text_cell(
                format::ago(image.created, format::now()),
                age,
                true,
                tokens,
            ));
        }
        if in_use > 0.0 {
            row = row.child(text_cell(used_count.to_string(), in_use, true, tokens));
        }
        row.child(pill(
            if image.dangling() {
                "dangling"
            } else {
                "ready"
            },
            tokens,
        ))
    });
    content
        .child(
            div()
                .id("images-table")
                .flex()
                .flex_col()
                .w_full()
                .role(Role::Table)
                .aria_label("Images")
                .children(rows),
        )
        .into_any_element()
}

fn image_matches(image: &Image, query: &str) -> bool {
    query.is_empty()
        || image.label().to_ascii_lowercase().contains(query)
        || image.id.to_ascii_lowercase().contains(query)
}

fn short_id(id: &str) -> String {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    id.chars().take(12).collect()
}

fn creation_card(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> AnyElement {
    let mut card = div()
        .flex()
        .flex_col()
        .w_full()
        .mb(px(12.0))
        .gap(px(8.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .bg(tokens.surface)
        .border_1()
        .border_color(tokens.border)
        .child(
            div()
                .id("creation-head")
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .gap(px(8.0))
                .role(gpui::Role::Button)
                .aria_label("Get and run images")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_creation_open(!this.creation_open, cx);
                }))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .child("Get and run images"),
                )
                .child(div().flex_1())
                .child(div().text_size(px(11.5)).text_color(tokens.muted).child(
                    if app.creation_open {
                        "Collapse"
                    } else {
                        "Pull an image, or run one"
                    },
                )),
        );
    if !app.creation_open {
        return card.into_any_element();
    }

    let pull_failure = app
        .pull_attempted
        .then(|| validation::pull(app.pull_input.read(cx).text()))
        .flatten();
    let run_failure = app
        .run_attempted
        .then(|| {
            validation::run(
                app.run_image.read(cx).text(),
                app.run_name.read(cx).text(),
                app.run_ports.read(cx).text(),
                app.run_volumes.read(cx).text(),
                app.run_env.read(cx).text(),
            )
        })
        .flatten();

    let pull_row = div()
        .flex()
        .flex_row()
        .items_end()
        .w_full()
        .gap(px(8.0))
        .child(field("Reference", app.pull_input.clone(), tokens))
        .child(command_button(
            "Pull",
            tokens,
            cx.listener(|this, _, _, cx| this.start_pull(cx)),
        ));
    card = card.child(pull_row);
    if let Some(failure) = pull_failure {
        card = card.child(error_line(&validation::describe(&failure), tokens));
    }
    card = card.child(div().w_full().h(px(1.0)).bg(tokens.border));

    let mut first = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .w_full()
        .items_end()
        .gap(px(10.0));
    first = first
        .child(field("Image", app.run_image.clone(), tokens))
        .child(field("Name", app.run_name.clone(), tokens));
    card = card.child(first);
    if let Some(failure) = &run_failure {
        card = card.child(error_line(&validation::describe(failure), tokens));
    }
    let mut second = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .w_full()
        .items_end()
        .gap(px(10.0));
    second = second
        .child(field("Ports", app.run_ports.clone(), tokens))
        .child(field("Volumes", app.run_volumes.clone(), tokens));
    card = card.child(second);
    card = card.child(field("Env", app.run_env.clone(), tokens));
    card = card.child(
        div()
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(10.5))
                    .text_color(tokens.muted)
                    .child("host:container ports, source:container volumes, KEY=value env; comma-separated"),
            )
            .child(command_button(
                "Run",
                tokens,
                cx.listener(|this, _, _, cx| this.start_run(cx)),
            )),
    );
    card.into_any_element()
}

fn field(
    label: &'static str,
    input: gpui::Entity<crate::ui::input::InputState>,
    tokens: Tokens,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(input)
}

fn command_button(
    label: &'static str,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("images-cmd-{label}")))
        .flex()
        .items_center()
        .justify_center()
        .h(px(28.0))
        .px(px(14.0))
        .rounded(px(14.0))
        .bg(tokens.accent)
        .role(gpui::Role::Button)
        .aria_label(label)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.accent_text)
                .child(label),
        )
}

fn error_line(text: &str, tokens: Tokens) -> Div {
    div()
        .text_size(px(11.5))
        .text_color(tokens.danger_text())
        .child(text.to_owned())
}
