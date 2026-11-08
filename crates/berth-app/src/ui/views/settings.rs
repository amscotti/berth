//! Settings: about, appearance, the engine chooser and maintenance.

use berth_core::format;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Role, div, px};

use crate::app::BerthApp;
use crate::settings::Appearance;
use crate::ui::table::page_header;

pub fn settings(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let header = page_header(app, widths, None, None, cx);

    let page_w = widths.content.min(920.0);
    let col_w = if page_w >= 860.0 {
        (page_w - 12.0) / 2.0
    } else {
        page_w
    };
    let about = card(tokens)
        .child(title("About berth", tokens))
        .child(
            div()
                .flex()
                .flex_col()
                .w(px((page_w - 32.0).min(740.0)))
                .gap(px(6.0))
                .child(muted("berth is a native monitor for Docker, Podman, Colima and Lima. It talks to the engine's HTTP API over its Unix socket directly — no webview, no helper daemon, no telemetry.", tokens))
                .child(muted("The last complete snapshot stays on screen while a worker refreshes the next one, so the tables and graphs neither blank nor freeze. Container CPU and memory use the same two-sample window as `docker stats`.", tokens)),
        );

    let appearance = card(tokens)
        .child(title("Appearance", tokens))
        .child(muted(
            "System follows this machine's desktop. The choice is remembered between launches.",
            tokens,
        ))
        .child(div().flex().flex_row().gap(px(6.0)).pt(px(4.0)).children(
            [Appearance::System, Appearance::Light, Appearance::Dark].map(|option| {
                let active = app.appearance == option;
                div()
                    .id(("appearance", option as usize))
                    .flex()
                    .items_center()
                    .h(px(28.0))
                    .px(px(12.0))
                    .rounded(px(14.0))
                    .bg(if active {
                        tokens.accent.alpha(0.16)
                    } else {
                        tokens.surface_hover
                    })
                    .role(Role::Button)
                    .aria_label(option.label())
                    .aria_selected(active)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_appearance(option, window, cx);
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(if active {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if active {
                                tokens.accent_text_pill()
                            } else {
                                tokens.text
                            })
                            .child(option.label()),
                    )
            }),
        ))
        .child(muted(
            "Changes apply immediately and are remembered.",
            tokens,
        ))
        .w(px(col_w));

    let engine = card(tokens).child(title("Engine", tokens));
    let engine = if let Some(runtime) = app.engine.active_runtime() {
        let mut card = engine
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(tokens.text)
                    .child(runtime.name.clone()),
            )
            .child(muted(
                &if runtime.reachable {
                    format!(
                        "v{} · API {}",
                        runtime.version.version, runtime.version.api_version
                    )
                } else {
                    "not answering".to_owned()
                },
                tokens,
            ))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_family("Menlo")
                    .text_color(tokens.muted)
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .truncate()
                            .child(runtime.socket.clone()),
                    ),
            );
        let reachable: Vec<_> = app
            .engine
            .runtimes
            .iter()
            .enumerate()
            .filter(|(_, runtime)| runtime.reachable)
            .collect();
        if reachable.len() > 1 {
            card = card.child(div().flex().flex_col().gap(px(4.0)).children(
                reachable.into_iter().map(|(index, runtime)| {
                    let active = index == app.engine.active;
                    div()
                        .id(("engine-choice", index))
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(px(30.0))
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .bg(if active {
                            tokens.accent.alpha(0.10)
                        } else {
                            tokens.surface_hover
                        })
                        .role(Role::Button)
                        .aria_label(runtime.name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_runtime(index, cx);
                        }))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(tokens.text)
                                .child(runtime.name.clone()),
                        )
                }),
            ));
        }
        card
    } else {
        engine.child(muted("No engine selected.", tokens))
    };

    let engine = engine.w(px(col_w));
    let middle = if page_w >= 860.0 {
        div()
            .flex()
            .flex_row()
            .w_full()
            .items_start()
            .gap(px(12.0))
            .child(appearance)
            .child(engine)
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(12.0))
            .child(appearance)
            .child(engine)
    };

    let maintenance = card(tokens)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .child(title("Maintenance", tokens))
                .child(div().flex_1())
                .child(muted(
                    &app.snapshot
                        .df_reclaimable()
                        .map(|bytes| format!("{} reclaimable", format::bytes(bytes.max(0) as u64)))
                        .unwrap_or_else(|| "reclaimable —".to_owned()),
                    tokens,
                )),
        )
        .child(muted(
            "Clean up removes stopped containers, unused images and build cache; unused volumes and networks are opt-in. Refresh reloads the engine immediately instead of waiting for the next five-second pass.",
            tokens,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .pt(px(2.0))
                .child(primary_button("Clean up unused…", tokens, cx))
                .child(secondary_button("Refresh now", tokens, cx)),
        );

    div()
        .flex()
        .flex_col()
        .w(px(page_w))
        .flex_1()
        .gap(px(12.0))
        .child(header)
        .child(about)
        .child(middle)
        .child(maintenance)
        .into_any_element()
}

fn card(tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .p(px(16.0))
        .rounded(px(layout::CARD_RADIUS))
        .bg(tokens.surface)
        .border_1()
        .border_color(tokens.border)
}

fn title(text: &'static str, tokens: Tokens) -> Div {
    div()
        .text_size(px(type_scale::CARD_TITLE))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.text)
        .child(text)
}

fn muted(text: &str, tokens: Tokens) -> Div {
    div()
        .text_size(px(12.0))
        .text_color(tokens.muted)
        .child(text.to_owned())
}

fn primary_button(
    label: &'static str,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    div()
        .id(label)
        .flex()
        .items_center()
        .h(px(28.0))
        .px(px(12.0))
        .rounded(px(14.0))
        .bg(tokens.accent)
        .role(Role::Button)
        .aria_label(label)
        .on_click(cx.listener(|this, _, _, cx| this.open_prune(cx)))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.accent_text)
                .child(label),
        )
}

fn secondary_button(
    label: &'static str,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    div()
        .id("refresh-now")
        .flex()
        .items_center()
        .h(px(28.0))
        .px(px(12.0))
        .rounded(px(14.0))
        .bg(tokens.surface_hover)
        .role(Role::Button)
        .aria_label(label)
        .on_click(cx.listener(|this, _, _, cx| this.refresh_now(cx)))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(tokens.text)
                .child(label),
        )
}
