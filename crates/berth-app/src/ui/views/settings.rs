//! Settings: about, appearance, the engine chooser and maintenance.

use berth_core::format;
use berth_ui::layout::{self, Widths};
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, Role, div, px};

use crate::app::BerthApp;
use crate::settings::Appearance;
use crate::ui::kit::{button, caption, card, card_row, card_title, chip};
use crate::ui::page::page_header;

/// The page doesn't stretch past this, however wide the window.
const PAGE_MAX: f32 = 920.0;
/// Below this the two shorter cards stack instead of sitting side by side.
const TWO_COLUMNS_FROM: f32 = 860.0;
/// The longest line of prose, so a paragraph stays readable.
const PROSE_MAX: f32 = 740.0;

pub fn settings(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let header = page_header(app, widths, None, None, cx);

    let page_w = widths.content.min(PAGE_MAX);
    let two_columns = page_w >= TWO_COLUMNS_FROM;
    let col_w = if two_columns {
        (page_w - layout::CARD_GAP) / 2.0
    } else {
        page_w
    };

    let prose_w = (page_w - 2.0 * layout::CARD_PADDING).min(PROSE_MAX);
    let about = card(tokens)
        .child(card_title("About berth", tokens))
        .child(
            div()
                .flex()
                .flex_col()
                .w(px(prose_w))
                .gap(px(6.0))
                .child(caption(
                    "berth is a native monitor for Docker, Podman, Colima and Lima. It talks to the engine's HTTP API over its Unix socket directly — no webview, no helper daemon, no telemetry.",
                    tokens,
                ))
                .child(caption(
                    "The last complete snapshot stays on screen while a worker refreshes the next one, so the tables and graphs neither blank nor freeze. Container CPU and memory use the same two-sample window as docker stats.",
                    tokens,
                )),
        );

    let appearance = card(tokens)
        .child(card_title("Appearance", tokens))
        .child(caption("System follows this machine's desktop.", tokens))
        .child(
            div()
                .id("appearance-choices")
                .role(Role::RadioGroup)
                .aria_label("Appearance")
                .flex()
                .flex_row()
                .gap(px(6.0))
                .pt(px(4.0))
                .children(
                    [Appearance::System, Appearance::Light, Appearance::Dark].map(|option| {
                        chip(
                            ("appearance", option as usize),
                            option.label(),
                            app.appearance == option,
                            tokens,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.set_appearance(option, window, cx);
                            },
                        ))
                    }),
                ),
        )
        .child(caption(
            "Changes apply immediately and are remembered between launches.",
            tokens,
        ))
        .w(px(col_w));

    let engine = card(tokens).child(card_title("Engine", tokens));
    let engine = if let Some(runtime) = app.engine.active_runtime() {
        let mut card = engine
            .child(
                div()
                    .text_size(px(type_scale::SECONDARY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(tokens.text)
                    .child(runtime.name.clone()),
            )
            .child(caption(
                if runtime.reachable {
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
                    .text_size(px(type_scale::MONO))
                    .font_family(type_scale::MONO_FAMILY)
                    .text_color(tokens.muted)
                    .child(runtime.socket.clone()),
            );
        let reachable: Vec<_> = app
            .engine
            .runtimes
            .iter()
            .enumerate()
            .filter(|(_, runtime)| runtime.reachable)
            .collect();
        if reachable.len() > 1 {
            // More than one engine answers: pick it here. One of a set, so
            // each is announced as such, with the one in use selected.
            card = card.child(
                div()
                    .id("engine-choices")
                    .role(Role::RadioGroup)
                    .aria_label("Engine")
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(6.0))
                    .children(reachable.into_iter().map(|(index, runtime)| {
                        chip(
                            ("engine-choice", index),
                            runtime.name.clone(),
                            app.engine.active_index() == Some(index),
                            tokens,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_runtime(index, cx);
                        }))
                    })),
            );
        }
        card
    } else {
        engine.child(caption("No engine selected.", tokens))
    };

    let engine = engine.w(px(col_w));
    // Side by side, the two cards stretch to the taller one: one bottom edge.
    let middle = if two_columns {
        card_row().child(appearance).child(engine)
    } else {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(layout::CARD_GAP))
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
                .gap(px(8.0))
                .child(card_title("Maintenance", tokens))
                .child(div().flex_1())
                .child(caption(
                    app.snapshot
                        .df_reclaimable()
                        .map(|bytes| format!("{} reclaimable", format::bytes(bytes.max(0) as u64)))
                        .unwrap_or_else(|| "reclaimable —".to_owned()),
                    tokens,
                )),
        )
        .child(caption(
            "Clean up removes stopped containers, unused images and build cache; unused volumes and networks are opt-in. Refresh reloads the engine immediately instead of waiting for the next five-second pass.",
            tokens,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .pt(px(2.0))
                // Opening the cleanup asks first; it destroys nothing itself,
                // so it isn't drawn as the destructive button.
                .child(
                    button("Clean up unused…", "Clean up unused…", tokens)
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.open_prune(cx))),
                )
                .child(
                    button("refresh-now", "Refresh now", tokens)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_now(cx))),
                ),
        );

    div()
        .flex()
        .flex_col()
        .w(px(page_w))
        .flex_1()
        .gap(px(layout::CARD_GAP))
        .child(header)
        .child(about)
        .child(middle)
        .child(maintenance)
        .into_any_element()
}
