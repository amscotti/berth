//! Overview disk card and its legend helpers.

use berth_core::format;
use berth_engine::df::Store;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{Context, Div, FontWeight, div, px};

use crate::app::BerthApp;
use crate::ui::graph;
use crate::ui::kit::{Card, button, caption, card, card_title};

pub(crate) fn disk_card(
    tokens: Tokens,
    width: f32,
    snapshot: &crate::state::Snapshot,
    dialect: berth_engine::discovery::Dialect,
    total: &str,
    reclaimable: &str,
    cx: &mut Context<BerthApp>,
) -> Card {
    let mut card = card(tokens)
        .w(px(width))
        .gap(px(9.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .child(card_title("Disk by store", tokens))
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(type_scale::LABEL))
                        .text_color(tokens.muted)
                        .child(format!("{reclaimable} reclaimable")),
                ),
        )
        .child(
            div()
                .text_size(px(type_scale::METRIC))
                .font_weight(FontWeight::BOLD)
                .font_features(crate::ui::tabular())
                .text_color(tokens.text)
                .child(total.to_owned()),
        );
    if let Some(df) = &snapshot.df {
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
                                format::measured(df.store(dialect, Store::Images)),
                                tokens.accent_fill(),
                                tokens,
                            ))
                            .child(legend(
                                "containers",
                                format::measured(df.store(dialect, Store::Containers)),
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
                                format::measured(df.store(dialect, Store::Volumes)),
                                tokens.store_volume(),
                                tokens,
                            ))
                            .child(legend(
                                "build cache",
                                format::measured(df.store(dialect, Store::BuildCache)),
                                tokens.muted,
                                tokens,
                            )),
                    ),
            );
        card = card.child(
            div().flex().flex_row().w_full().justify_end().child(
                // Opening the cleanup asks first; it destroys nothing itself.
                button("overview-cleanup", "Clean up unused…", tokens)
                    .on_click(cx.listener(|this, _, _, cx| this.open_prune(cx))),
            ),
        );
    } else {
        card = card.child(caption(
            "Disk figures arrive on the next slow pass.",
            tokens,
        ));
    }
    card
}

fn legend(label: &str, figure: String, color: gpui::Hsla, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.0))
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(color))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.muted)
                .child(format!("{label} {figure}")),
        )
}
