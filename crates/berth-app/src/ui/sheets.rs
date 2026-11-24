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

use crate::actions::{ContainerAction, Pending, Target};
use crate::app::{BerthApp, CreateKind};
use crate::jobs::mutation::PruneOptions;
use crate::ui::kit::{Button, button, caption, checkbox};
use crate::view::Sheet;

pub fn overlay(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Option<AnyElement> {
    let sheet = app.view.sheet.as_ref()?;
    let panel = match sheet {
        Sheet::Confirm(pending) => confirm_panel(app, pending, tokens, cx),
        Sheet::Create { kind, .. } => create_panel(app, *kind, tokens, cx),
        Sheet::Prune(options) => prune_panel(app, options, tokens, cx),
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
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                this.on_dismiss(&berth_commands::app_actions::Dismiss, window, cx);
            }))
            .child(
                div()
                    .id("sheet-panel")
                    .role(Role::Dialog)
                    .aria_label(sheet.label())
                    .on_scroll(|_, _, cx| cx.stop_propagation())
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

/// Action-specific label for the destructive button in a confirmation sheet.
pub(crate) fn confirm_verb(pending: &Pending) -> &'static str {
    match pending {
        Pending::One(Target::Container { action, .. }) => action.label(),
        Pending::One(Target::Removal { what, .. }) => what.verb(),
        Pending::Batch { action, .. } => action.label(),
        Pending::Engine { op, .. } => op.label(),
    }
}

/// Explanation of what confirming `pending` will do, naming force-stop or the
/// target engine's running containers where known.
pub(crate) fn confirm_body(app: &BerthApp, pending: &Pending) -> String {
    match pending {
        Pending::Engine { kind, socket, .. } => {
            let running = if app
                .engine
                .active_runtime()
                .is_some_and(|runtime| runtime.socket == *socket)
            {
                Some(app.snapshot.running())
            } else {
                app.engine
                    .runtimes
                    .iter()
                    .find(|runtime| runtime.socket == *socket && runtime.info_read)
                    .map(|runtime| runtime.info.containers_running)
            };
            match running {
                Some(count) => format!(
                    "{} on {} will stop with the engine.",
                    format::count(count, "running container", "running containers"),
                    kind.label()
                ),
                None => format!(
                    "Any running containers on {} will stop with the engine.",
                    kind.label()
                ),
            }
        }
        Pending::One(Target::Container {
            action: ContainerAction::Delete,
            id,
            ..
        }) if app
            .snapshot
            .selected_container(id)
            .is_some_and(|container| container.is_up()) =>
        {
            "This container is running and will be force-stopped before removal. This cannot be undone.".to_owned()
        }
        Pending::Batch {
            action: ContainerAction::Delete,
            ..
        } if app.view.bulk_selected.iter().any(|id| {
            app.snapshot
                .selected_container(id)
                .is_some_and(|container| container.is_up())
        }) =>
        {
            "Running containers in this selection will be force-stopped before removal. This cannot be undone.".to_owned()
        }
        _ => "This cannot be undone.".to_owned(),
    }
}

fn confirm_panel(
    app: &BerthApp,
    pending: &Pending,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let title = pending.title();
    // Confirm waits for a running mutation, instead of closing the sheet and
    // dropping the action when the request is refused.
    let busy = app.mutating();
    let body = confirm_body(app, pending);

    let cancel = button("sheet-Cancel", "Cancel", tokens)
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.cancel.clone())
        .on_click(cx.listener(|this, _, _, cx| this.resolve_pending(false, cx)));
    // The way forward destroys something, so it is the destructive button.
    let confirm = button("sheet-Confirm", confirm_verb(pending), tokens)
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
    let field_w = SHEET_MAX_WIDTH - 2.0 * CARD_PADDING;
    app.view
        .create_name
        .update(cx, |input, _| input.set_width(field_w));
    let name = app.view.create_name.read(cx).text().trim().to_owned();
    // The sentence comes once Create was pressed with a name the rule
    // refuses, and not before.
    let error = match &app.view.sheet {
        Some(Sheet::Create { attempted, .. }) if *attempted => {
            crate::app_forms::create_name_error(kind, &name, &app.snapshot.volumes)
        }
        _ => None,
    };

    let cancel = button("sheet-Cancel", "Cancel", tokens)
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.cancel.clone())
        .on_click(cx.listener(|this, _, _, cx| this.close_sheet(cx)));
    let create = button("sheet-Create", "Create", tokens)
        .primary()
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.create.clone())
        .description("Enter a name first")
        .disabled(name.is_empty())
        .on_click(cx.listener(|this, _, _, cx| this.submit_create(cx)));

    let mut panel = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(sheet_title(format!("New {}", kind.noun()), tokens.text))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(typography::LABEL))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(tokens.muted)
                        .child("Name"),
                )
                .child(app.view.create_name.clone()),
        )
        .child(caption(
            "2–128 letters, digits, dot, dash or underscore; starts with a letter or digit.",
            tokens,
        ));
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

fn prune_panel(
    app: &BerthApp,
    options: &PruneOptions,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let build_supported = app.prune_build_supported();
    let selected = app.prune_selected_bytes(options);

    let cancel = button("sheet-Cancel", "Cancel", tokens)
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.cancel.clone())
        .on_click(cx.listener(|this, _, _, cx| this.close_sheet(cx)));
    let nothing_to_clean = app.prune_has_work(options) == Some(false);
    // Cleaning up destroys, so it is the destructive button. It waits only
    // while berth can see there is nothing ticked that would go.
    let clean_up = button("sheet-Clean up", "Clean up", tokens)
        .danger()
        .width(px(DIALOG_BUTTON_WIDTH))
        .focus(app.view.sheet_focus.clean_up.clone())
        .description("Tick something there is left to clean up")
        .disabled(nothing_to_clean)
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
            app.prune_ticked(options, field),
            tokens,
            cx,
        ));
    }
    if nothing_to_clean {
        panel = panel.child(caption("Nothing ticked has data left to remove.", tokens));
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
    .on_click(cx.listener(move |this, _, _, cx| this.toggle_prune_option(field, cx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Removal;
    use crate::jobs::engine_ops::EngineOp;
    use berth_engine::discovery::{Kind, Runtime};
    use berth_engine::dto::Info;
    use gpui::TestAppContext;

    #[test]
    fn confirm_buttons_name_the_specific_action() {
        assert_eq!(
            confirm_verb(&Pending::One(Target::Container {
                action: ContainerAction::Delete,
                id: "a".into(),
                name: "web".into(),
            })),
            "Delete"
        );
        assert_eq!(
            confirm_verb(&Pending::One(Target::Removal {
                what: Removal::Volume,
                id: "v".into(),
                name: "v".into(),
            })),
            "Remove"
        );
        assert_eq!(
            confirm_verb(&Pending::Engine {
                op: EngineOp::Stop,
                kind: Kind::Lima,
                socket: "/tmp/a.sock".into(),
            }),
            "Stop"
        );
    }

    #[gpui::test]
    fn confirm_body_warns_about_running_containers_and_target_engine(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, _| {
            app.engine.runtimes.push(Runtime {
                kind: Kind::Colima,
                name: "Colima".into(),
                socket: "/tmp/colima.sock".into(),
                reachable: true,
                info_read: true,
                info: Info {
                    containers_running: 5,
                    ..Default::default()
                },
                ..Default::default()
            });
            // Running container delete warns about force-stop.
            let body = confirm_body(
                app,
                &Pending::One(Target::Container {
                    action: ContainerAction::Delete,
                    id: "demo-1".into(),
                    name: "berth-demo-web".into(),
                }),
            );
            assert!(body.contains("force-stopped"), "{body}");

            // Engine op on a non-active engine reads that engine's count (5), not active (2).
            let body = confirm_body(
                app,
                &Pending::Engine {
                    op: EngineOp::Stop,
                    kind: Kind::Colima,
                    socket: "/tmp/colima.sock".into(),
                },
            );
            assert!(body.contains("5 running containers on Colima"), "{body}");
        });
    }
}
