//! Overview disk card and its legend helpers.

use berth_core::format;
use berth_ui::layout;
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{Context, Div, FontWeight, Role, div, px};

use super::overview_cards::{muted, title};
use crate::app::BerthApp;
use crate::ui::graph;

pub(crate) fn disk_card(
    tokens: Tokens,
    width: f32,
    snapshot: &crate::state::Snapshot,
    total: &str,
    reclaimable: &str,
    cx: &mut Context<BerthApp>,
) -> Div {
    let mut card = div()
        .flex()
        .flex_col()
        .w(px(width))
        .gap(px(9.0))
        .p(px(16.0))
        .rounded(px(layout::CARD_RADIUS))
        .bg(tokens.surface)
        .border_1()
        .border_color(tokens.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .child(title("Disk", tokens))
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(tokens.muted)
                        .child(format!("{reclaimable} reclaimable")),
                ),
        )
        .child(
            div()
                .text_size(px(26.0))
                .font_weight(FontWeight::BOLD)
                .font_features(crate::ui::tabular())
                .text_color(tokens.text)
                .child(total.to_owned()),
        );
    if let Some(df) = &snapshot.df {
        let stores = df.stores();
        let shares = df.shares();
        card = card
            .child(graph::stacked_bar(
                vec![
                    (shares.images, tokens.accent_fill()),
                    (shares.containers, tokens.success),
                    (shares.volumes, tokens.store_volume()),
                    (shares.build_cache, tokens.muted),
                ],
                tokens.track,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .w_full()
                            .gap(px(12.0))
                            .child(legend(
                                "images",
                                stores.images.max(0) as u64,
                                tokens.accent_fill(),
                                tokens,
                            ))
                            .child(legend(
                                "containers",
                                stores.containers.max(0) as u64,
                                tokens.success,
                                tokens,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .w_full()
                            .gap(px(12.0))
                            .child(legend(
                                "volumes",
                                stores.volumes.max(0) as u64,
                                tokens.store_volume(),
                                tokens,
                            ))
                            .child(legend(
                                "build cache",
                                stores.build_cache.max(0) as u64,
                                tokens.muted,
                                tokens,
                            )),
                    ),
            );
        card = card.child(
            div().flex().flex_row().w_full().justify_end().child(
                div()
                    .id("overview-cleanup")
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .px(px(10.0))
                    .rounded(px(13.0))
                    .bg(tokens.surface_hover)
                    .role(Role::Button)
                    .aria_label("Clean up unused")
                    .on_click(cx.listener(|this, _, _, cx| this.open_prune(cx)))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(tokens.text)
                            .child("Clean up unused"),
                    ),
            ),
        );
    } else {
        card = card.child(muted("Disk figures arrive on the next slow pass.", tokens));
    }
    card
}

fn legend(label: &str, bytes: u64, color: gpui::Hsla, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.0))
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(color))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(tokens.muted)
                .child(format!("{} {}", label, format::bytes(bytes))),
        )
}
