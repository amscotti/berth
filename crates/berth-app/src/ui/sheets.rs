//! Modal sheets: destructive confirmation, create volume/network, and prune.
//! One sheet at a time; the backdrop owns every click outside the panel.

use berth_core::format;
use berth_engine::prune::{self, Engine};
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Role, div, px};

use crate::app::{BerthApp, CreateKind, Pending};

pub fn overlay(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Option<AnyElement> {
    let (label, panel) = if let Some(pending) = &app.pending {
        (
            format!("{} {}?", pending.verb, pending.name),
            confirm_panel(app, pending, tokens, cx),
        )
    } else if let Some(kind) = app.create_kind {
        (
            format!("New {}", kind.noun()),
            create_panel(app, kind, tokens, cx),
        )
    } else if app.prune_open {
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
            .bg(tokens.text.alpha(0.38))
            .on_click(cx.listener(|this, _, window, cx| {
                this.on_dismiss(&berth_commands::app_actions::Dismiss, window, cx);
            }))
            .child(
                div()
                    .id("sheet-panel")
                    .role(Role::Dialog)
                    .aria_label(label)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .w(px(620.0))
                    .p(px(16.0))
                    .rounded(px(10.0))
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

fn confirm_panel(
    app: &BerthApp,
    pending: &Pending,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let engine = pending.method == "ENGINE";
    let title = format!("{} {}?", pending.verb, pending.name);
    let body = if engine {
        format!(
            "Stopping or restarting this machine takes every running container on {} down with it.",
            pending.name
        )
    } else {
        "This cannot be undone.".to_owned()
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.danger_text())
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(tokens.muted)
                .child(body),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .justify_end()
                .gap(px(8.0))
                .child(button(
                    "Cancel",
                    false,
                    tokens,
                    cx.listener(|this, _, _, cx| this.resolve_pending(false, cx)),
                ))
                .child(button(
                    "Confirm",
                    true,
                    tokens,
                    cx.listener(|this, _, _, cx| this.resolve_pending(true, cx)),
                )),
        )
        .when(app.mutating(), |this| {
            this.child(
                div()
                    .text_size(px(11.0))
                    .text_color(tokens.muted)
                    .child("An engine operation is already running."),
            )
        })
}

fn create_panel(
    app: &BerthApp,
    kind: CreateKind,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Div {
    let name = app.create_name.read(cx).text().to_owned();
    let error = app
        .create_attempted
        .then(|| crate::app_forms::create_name_error(kind, &name, &app.snapshot.volumes))
        .flatten();

    let mut panel = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.text)
                .child(format!("New {}", kind.noun())),
        )
        .child(app.create_name.clone());
    if let Some(error) = error {
        panel = panel.child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.danger_text())
                .child(error),
        );
    }
    panel.child(
        div()
            .flex()
            .flex_row()
            .w_full()
            .justify_end()
            .gap(px(8.0))
            .child(button(
                "Cancel",
                false,
                tokens,
                cx.listener(|this, _, _, cx| this.close_create(cx)),
            ))
            .child(button(
                "Create",
                true,
                tokens,
                cx.listener(|this, _, _, cx| this.submit_create(cx)),
            )),
    )
}

fn prune_panel(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Div {
    let engine = match app.engine.dialect {
        berth_engine::discovery::Dialect::Podman => Engine::Podman,
        _ => Engine::Docker,
    };
    let build_supported = prune::supported(prune::Kind::BuildCache, engine);
    let build_on = prune::build_cache_enabled(app.prune_options.build_cache, engine);
    let free = app
        .snapshot
        .df
        .as_ref()
        .map(|df| df.reclaimable_by_store())
        .unwrap_or_default();
    let selected = {
        let mut total = 0i64;
        if app.prune_options.containers {
            total = total.saturating_add(free.containers);
        }
        if app.prune_options.images {
            total = total.saturating_add(free.images);
        }
        if app.prune_options.volumes {
            total = total.saturating_add(free.volumes);
        }
        if build_on {
            total = total.saturating_add(free.build_cache);
        }
        total
    };

    let mut panel = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .items_center()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .child("Clean up unused data"),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(tokens.muted)
                        .child(format!("{} selected", format::bytes(selected.max(0) as u64))),
                ),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.muted)
                .child(if build_supported {
                    "Stopped containers, unused images and build cache. Volumes and networks are opt-in; this cannot be undone."
                } else {
                    "Stopped containers and unused images. Volumes and networks are opt-in; this cannot be undone."
                }),
        )
        .child(option(
            "Stopped containers",
            free.containers,
            app.prune_options.containers,
            tokens,
            cx.listener(|this, _, _, cx| {
                this.prune_options.containers = !this.prune_options.containers;
                cx.notify();
            }),
        ))
        .child(option(
            "Unused images",
            free.images,
            app.prune_options.images,
            tokens,
            cx.listener(|this, _, _, cx| {
                this.prune_options.images = !this.prune_options.images;
                cx.notify();
            }),
        ))
        .child(option(
            "Unused volumes",
            free.volumes,
            app.prune_options.volumes,
            tokens,
            cx.listener(|this, _, _, cx| {
                this.prune_options.volumes = !this.prune_options.volumes;
                cx.notify();
            }),
        ))
        .child(option(
            "Unused networks",
            -1,
            app.prune_options.networks,
            tokens,
            cx.listener(|this, _, _, cx| {
                this.prune_options.networks = !this.prune_options.networks;
                cx.notify();
            }),
        ));
    if build_supported {
        panel = panel.child(option(
            "Build cache",
            free.build_cache,
            build_on,
            tokens,
            cx.listener(|this, _, _, cx| {
                this.prune_options.build_cache = !this.prune_options.build_cache;
                cx.notify();
            }),
        ));
    }
    panel.child(
        div()
            .flex()
            .flex_row()
            .w_full()
            .justify_end()
            .gap(px(8.0))
            .child(button(
                "Cancel",
                false,
                tokens,
                cx.listener(|this, _, _, cx| this.close_prune(cx)),
            ))
            .child(button(
                "Clean up",
                true,
                tokens,
                cx.listener(|this, _, _, cx| this.run_prune(cx)),
            )),
    )
}

fn option(
    label: &'static str,
    amount: i64,
    on: bool,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(format!("prune-option-{label}")))
        .flex()
        .flex_row()
        .w_full()
        .items_center()
        .gap(px(8.0))
        .h(px(24.0))
        .role(Role::CheckBox)
        .aria_label(label)
        .aria_selected(on)
        .on_click(on_click)
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .rounded(px(4.0))
                .bg(if on { tokens.accent } else { tokens.track })
                .flex()
                .items_center()
                .justify_center()
                .when(on, |this| {
                    this.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(tokens.accent_text)
                            .child("✓"),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(tokens.text)
                .child(label),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(11.0))
                .text_color(tokens.muted)
                .child(if amount < 0 {
                    "—".to_owned()
                } else {
                    format::bytes(amount.max(0) as u64)
                }),
        )
}

fn button(
    label: &'static str,
    primary: bool,
    tokens: Tokens,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(format!("sheet-button-{label}")))
        .flex()
        .items_center()
        .justify_center()
        .w(px(92.0))
        .h(px(28.0))
        .rounded(px(14.0))
        .bg(if primary {
            tokens.accent
        } else {
            tokens.surface_hover
        })
        .role(Role::Button)
        .aria_label(label)
        .on_click(on_click)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(if primary {
                    tokens.accent_text
                } else {
                    tokens.text
                })
                .child(label),
        )
}
