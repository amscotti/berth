//! Images: repository, short ID, size, age and dangling/ready state.

use berth_core::format;
use berth_core::validation;
use berth_engine::dto::Image;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, Role, SharedString, div, px};

use crate::app::{BerthApp, DetailKind, SearchKind};
use crate::ui::data_table::{Column, DataTable, RowKey, pill, text_cell};
use crate::ui::inspect_shared::short_id;
use crate::ui::kit::{button, card};
use crate::ui::page::{empty_state, page_header};

pub fn images(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let query = app.query(SearchKind::Images).to_ascii_lowercase();
    let (sort_col, sort_asc) = app.sort_for(1);
    // Worked out once per snapshot, query and sort, and kept from one frame
    // to the next while those hold.
    let shown = app.rows_of(SearchKind::Images, || {
        let mut shown: Vec<usize> = app
            .snapshot
            .images
            .iter()
            .enumerate()
            .filter(|(_, image)| image_matches(image, &query))
            .map(|(index, _)| index)
            .collect();
        shown.sort_by(|&a, &b| {
            let (a, b) = (&app.snapshot.images[a], &app.snapshot.images[b]);
            let ord = match sort_col {
                1 => a.id.cmp(&b.id),
                2 => a.size.cmp(&b.size),
                3 => a.created.cmp(&b.created),
                4 => a.dangling().cmp(&b.dangling()),
                _ => a.label().cmp(b.label()),
            };
            if sort_asc { ord } else { ord.reverse() }
        });
        shown
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
    let mut table = DataTable::new(
        "images-table",
        "Images",
        "images-header",
        1,
        row_key,
        open_row,
    )
    .column(Column::sorted(
        "repository",
        "REPOSITORY",
        repo,
        false,
        0,
        repository_cell,
    ))
    .column(Column::sorted("id", "IMAGE ID", id, false, 1, id_cell))
    .column(Column::sorted("size", "SIZE", size, true, 2, size_cell));
    if age > 0.0 {
        table = table.column(Column::sorted(
            "created",
            "CREATED",
            age,
            true,
            3,
            created_cell,
        ));
    }
    if in_use > 0.0 {
        table = table.column(Column::plain("in-use", "IN USE", in_use, true, in_use_cell));
    }
    let table = table
        .column(Column::sorted(
            "status",
            "STATUS",
            status,
            false,
            4,
            status_cell,
        ))
        .hairline();
    let header = table.header(app, tokens, cx);
    let body = table.body(
        shown,
        layout::ROW_CONTAINER,
        &app.view.lists[SearchKind::Images as usize].scroll,
        tokens,
        cx,
    );
    content.child(header).child(body).into_any_element()
}

fn row_key(app: &BerthApp, ix: usize) -> RowKey {
    let image = &app.snapshot.images[ix];
    RowKey {
        id: SharedString::from(format!("image-{}", image.id)),
        name: label_of(image).to_owned(),
        selected: app.view.detail_kind == DetailKind::Image
            && app.view.selected_id.as_deref() == Some(image.id.as_str()),
        opens: true,
    }
}

fn open_row(app: &mut BerthApp, ix: usize, cx: &mut Context<BerthApp>) {
    let image = &app.snapshot.images[ix];
    let (id, label) = (image.id.clone(), label_of(image).to_owned());
    app.select_image(&id, &label, cx);
}

/// What an image is called: its first tag, or "untagged".
fn label_of(image: &Image) -> &str {
    if image.dangling() {
        "untagged"
    } else {
        image.tag()
    }
}

fn repository_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let image = &app.snapshot.images[ix];
    text_cell(label_of(image).to_owned(), width, false, app.tokens).into_any_element()
}

fn id_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let image = &app.snapshot.images[ix];
    text_cell(short_id(&image.id), width, false, app.tokens).into_any_element()
}

fn size_cell(app: &mut BerthApp, ix: usize, width: f32, _cx: &mut Context<BerthApp>) -> AnyElement {
    let image = &app.snapshot.images[ix];
    text_cell(format::bytes(image.size), width, true, app.tokens).into_any_element()
}

fn created_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let image = &app.snapshot.images[ix];
    text_cell(
        format::ago(image.created, format::now()),
        width,
        true,
        app.tokens,
    )
    .into_any_element()
}

/// How many containers run it, figured out with the lists.
fn in_use_cell(
    app: &mut BerthApp,
    ix: usize,
    width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    text_cell(
        app.snapshot.image_use(ix).to_string(),
        width,
        true,
        app.tokens,
    )
    .into_any_element()
}

fn status_cell(
    app: &mut BerthApp,
    ix: usize,
    _width: f32,
    _cx: &mut Context<BerthApp>,
) -> AnyElement {
    let image = &app.snapshot.images[ix];
    pill(
        if image.dangling() {
            "dangling"
        } else {
            "ready"
        },
        app.tokens,
    )
    .into_any_element()
}

fn image_matches(image: &Image, query: &str) -> bool {
    query.is_empty()
        || image.label().to_ascii_lowercase().contains(query)
        || image.id.to_ascii_lowercase().contains(query)
}

/// "Get and run images": a card that folds down to its heading, and opens on
/// the two forms.
fn creation_card(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> AnyElement {
    let open = app.view.creation_open;
    let mut card = card(tokens).mb(px(layout::CARD_GAP)).child(
        // The heading opens and closes the forms. To assistive technology
        // it says which way it is.
        crate::ui::kit::focusable(div().id("creation-head"), tokens.focus_ring())
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .gap(px(layout::ROW_GAP))
            .px(px(6.0))
            // The hairline that the focus ring is drawn in.
            .border_1()
            .border_color(tokens.invisible())
            .rounded(px(layout::RADIUS_ROW))
            .role(Role::DisclosureTriangle)
            .aria_label("Get and run images")
            .aria_expanded(open)
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_creation_open(!this.view.creation_open, cx);
            }))
            .child(
                div()
                    .text_size(px(type_scale::BODY))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.text)
                    .child("Get and run images"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(type_scale::SECONDARY))
                    .text_color(tokens.muted)
                    .child(if open {
                        "Collapse"
                    } else {
                        "Pull an image, or run one"
                    }),
            ),
    );
    if !open {
        return card.into_any_element();
    }

    let pull_failure = app
        .view
        .pull_attempted
        .then(|| validation::pull(app.view.pull_input.read(cx).text()))
        .flatten();
    let run_failure = app
        .view
        .run_attempted
        .then(|| {
            validation::run(
                app.view.run_image.read(cx).text(),
                app.view.run_name.read(cx).text(),
                app.view.run_ports.read(cx).text(),
                app.view.run_volumes.read(cx).text(),
                app.view.run_env.read(cx).text(),
            )
        })
        .flatten();

    let pull_row = div()
        .flex()
        .flex_row()
        .items_end()
        .w_full()
        .gap(px(8.0))
        .child(field("Reference", app.view.pull_input.clone(), tokens))
        .child(
            button("images-cmd-Pull", "Pull", tokens)
                .primary()
                .on_click(cx.listener(|this, _, _, cx| this.start_pull(cx))),
        );
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
        .gap(px(layout::ROW_GAP));
    first = first
        .child(field("Image", app.view.run_image.clone(), tokens))
        .child(field("Name", app.view.run_name.clone(), tokens));
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
        .gap(px(layout::ROW_GAP));
    second = second
        .child(field("Ports", app.view.run_ports.clone(), tokens))
        .child(field("Volumes", app.view.run_volumes.clone(), tokens));
    card = card.child(second);
    card = card.child(field("Env", app.view.run_env.clone(), tokens));
    if app.view.run_env_missing > 0 {
        let noun = if app.view.run_env_missing == 1 {
            "value was"
        } else {
            "values were"
        };
        card = card.child(
            div()
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.warning_text())
                .child(format!(
                    "{} {noun} left empty because the source redacted them. Fill them in before running.",
                    app.view.run_env_missing
                )),
        );
    }
    card = card.child(
        div()
            .flex()
            .flex_row()
            .w_full()
            .items_center()
            .gap(px(layout::CARD_GAP))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(type_scale::LABEL))
                    .text_color(tokens.muted)
                    .child("host:container ports, source:container volumes, KEY=value env; comma-separated"),
            )
            .child(
                button("images-cmd-Run", "Run", tokens)
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.start_run(cx))),
            ),
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
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(input)
}

fn error_line(text: &str, tokens: Tokens) -> Div {
    div()
        .text_size(px(type_scale::SECONDARY))
        .text_color(tokens.danger_text())
        .child(text.to_owned())
}
