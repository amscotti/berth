//! Modal sheets: destructive confirmation, create volume/network, and prune.
//! One sheet at a time; the backdrop owns every click outside the panel.
//!
//! A sheet owns the keyboard while it is open. Its controls focus through
//! handles the app keeps, so that Tab crosses only them; see `focus.rs`.

use berth_core::format;
use berth_ui::layout::{CARD_PADDING, CARD_RADIUS, DIALOG_BUTTON_WIDTH, SHEET_MAX_WIDTH};
use berth_ui::theme::Tokens;
use berth_ui::typography;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Role, SharedString, div, px};

use crate::app::{BerthApp, CreateKind, Pending};
use crate::ui::kit::{Button, button, caption, checkbox};

pub fn overlay(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Option<AnyElement> {
    let (label, panel) = if let Some(pending) = &app.view.pending {
        (
            format!("{} {}?", pending.verb, pending.name),
            confirm_panel(app, pending, tokens, cx),
        )
    } else if let Some(kind) = app.view.create_kind {
        (
            format!("New {}", kind.noun()),
            create_panel(app, kind, tokens, cx),
        )
    } else if app.view.prune_open {
        (
            "Clean up unused data".to_owned(),
            prune_panel(app, tokens, cx),
        )
    } else {
        return None;
    };

    Some(
        div()
            .id("sheet-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(tokens.backdrop())
            .on_click(cx.listener(|this, _, window, cx| {
                this.on_dismiss(&berth_commands::app_actions::Dismiss, window, cx);
            }))
            .child(
                div()
                    .id("sheet-panel")
                    .role(Role::Dialog)
                    .aria_label(label)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .w(px(SHEET_MAX_WIDTH))
                    .p(px(CARD_PADDING))
                    .rounded(px(CARD_RADIUS))
                    .bg(tokens.surface)
                    .border_1()
                    .border_color(tokens.border)
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(panel),
            )
            .into_any_element(),
    )
}

/// A sheet's title. A dialog is the most important thing on screen while it
/// is open, so its title is a card's title, not body text.
fn sheet_title(text: impl Into<SharedString>, color: gpui::Hsla) -> Div {
    div()
        .text_size(px(typography::TITLE_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(text.into())
}

/// A dialog's buttons: Cancel, then the way forward, on one baseline.
fn dialog_buttons(cancel: Button, accept: Button) -> Div {
    div()
        .flex()
        .flex_row()
        .w_full()
        .justify_end()
        .gap(px(8.0))
        .child(cancel)
        .child(accept)
}

fn confirm_panel(
    app: &BerthApp,
    pending: &Pending,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let machine = pending.method == "ENGINE";
    let title = format!("{} {}?", pending.verb, pending.name);
    // Confirm waits for a running mutation, instead of closing the sheet and
    // dropping the action when the request is refused.
    let busy = app.mutating();
    // A confirmation names what it costs. A machine takes its whole fleet
    // down with it: say how many.
    let body = if machine {
        format!(
            "{} on {} will stop with the engine.",
            format::count(
                app.snapshot.running(),
                "running container",
                "running containers"
            ),
            pending.name
        )
    } else {
        "This cannot be undone.".to_owned()
    };

    let cancel = button("sheet-Cancel", "Cancel", tokens)
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.cancel.clone())
        .on_click(cx.listener(|this, _, _, cx| this.resolve_pending(false, cx)));
    // The way forward destroys something, so it is the destructive button.
    let confirm = button("sheet-Confirm", "Confirm", tokens)
        .danger()
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.confirm.clone())
        .description("Unavailable until the running engine operation finishes")
        .disabled(busy)
        .on_click(cx.listener(|this, _, _, cx| this.resolve_pending(true, cx)));

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(sheet_title(title, tokens.danger_text()))
        .child(caption(body, tokens))
        .child(dialog_buttons(cancel, confirm))
        .when(busy, |this| {
            this.child(caption("An engine operation is already running.", tokens))
        })
}

fn create_panel(
    app: &BerthApp,
    kind: CreateKind,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let name = app.view.create_name.read(cx).text().to_owned();
    let error = app
        .view
        .create_attempted
        .then(|| crate::app_forms::create_name_error(kind, &name, &app.snapshot.volumes))
        .flatten();

    let cancel = button("sheet-Cancel", "Cancel", tokens)
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.cancel.clone())
        .on_click(cx.listener(|this, _, _, cx| this.close_create(cx)));
    let create = button("sheet-Create", "Create", tokens)
        .primary()
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.create.clone())
        .on_click(cx.listener(|this, _, _, cx| this.submit_create(cx)));

    let mut panel = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(sheet_title(format!("New {}", kind.noun()), tokens.text))
        .child(app.view.create_name.clone());
    if let Some(error) = error {
        panel = panel.child(
            div()
                .text_size(px(typography::SECONDARY))
                .text_color(tokens.danger_text())
                .child(error),
        );
    }
    panel.child(dialog_buttons(cancel, create))
}

/// What a cleanup can remove, in the order the sheet lists it. A network is
/// fourth: it has no size, so there is no figure to show for it.
const PRUNE_LABELS: [&str; 5] = [
    "Stopped containers",
    "Unused images",
    "Unused volumes",
    "Unused networks",
    "Build cache",
];

fn prune_panel(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Div {
    let build_supported = app.prune_build_supported();
    let selected = app.prune_selected_bytes();

    let cancel = button("sheet-Cancel", "Cancel", tokens)
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.cancel.clone())
        .on_click(cx.listener(|this, _, _, cx| this.close_prune(cx)));
    // Cleaning up destroys, so it is the destructive button. It waits only
    // while berth can see there is nothing ticked that would go.
    let clean_up = button("sheet-Clean up", "Clean up", tokens)
        .danger()
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.clean_up.clone())
        .description("Tick something there is left to clean up")
        .disabled(app.prune_has_work() == Some(false))
        .on_click(cx.listener(|this, _, _, cx| this.run_prune(cx)));

    let mut panel = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .child(sheet_title("Clean up unused data", tokens.text))
                .child(div().flex_1())
                .child(caption(
                    format!(
                        "{} selected",
                        // Unknown is a dash, not zero.
                        selected.map_or_else(|| "—".to_owned(), |bytes| format::measured(Some(bytes)))
                    ),
                    tokens,
                )),
        )
        .child(caption(
            if build_supported {
                "Removes stopped containers, every image no container uses, and build cache. Volumes and networks are opt-in; this cannot be undone."
            } else {
                "Removes stopped containers and every image no container uses. Volumes and networks are opt-in; this cannot be undone."
            },
            tokens,
        ));
    for (field, label) in PRUNE_LABELS.iter().enumerate() {
        if field == 4 && !build_supported {
            continue;
        }
        panel = panel.child(prune_option(
            app,
            field,
            label,
            app.prune_bytes(field),
            app.prune_ticked(field),
            tokens,
            cx,
        ));
    }
    panel.child(dialog_buttons(cancel, clean_up))
}

/// What an option amounts to: how many things, and how much data, as far as
/// each is known.
fn amount(count: Option<i64>, bytes: Option<i64>) -> String {
    match (count, bytes.filter(|bytes| *bytes >= 0)) {
        (Some(count), Some(bytes)) => {
            format!("{count} · {}", format::measured(Some(bytes)))
        }
        (Some(count), None) => count.to_string(),
        (None, Some(bytes)) => format::measured(Some(bytes)),
        (None, None) => "—".to_owned(),
    }
}

/// One kind of data a cleanup can remove, with what it amounts to.
#[allow(clippy::too_many_arguments)]
fn prune_option(
    app: &BerthApp,
    field: usize,
    label: &'static str,
    bytes: Option<i64>,
    on: bool,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> crate::ui::kit::Checkbox {
    checkbox(
        SharedString::from(format!("prune-option-{label}")),
        label,
        on,
        tokens,
    )
    .figure(amount(app.prune_count(field), bytes))
    .focus(app.view.sheet_focus.options[field].clone())
    .on_click(cx.listener(move |this, _, _, cx| {
        let options = &mut this.view.prune_options;
        match field {
            0 => options.containers = !options.containers,
            1 => options.images = !options.images,
            2 => options.volumes = !options.volumes,
            3 => options.networks = !options.networks,
            _ => options.build_cache = !options.build_cache,
        }
        cx.notify();
    }))
}
