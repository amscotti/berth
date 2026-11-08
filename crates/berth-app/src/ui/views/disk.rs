//! Disk: what the engine's stores hold and what a cleanup could reclaim.

use berth_core::format;
use berth_engine::df::Df;
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Hsla, div, px};

use crate::app::BerthApp;
use crate::ui::graph;
use crate::ui::table::{empty_state, page_header};

pub fn disk(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let header = page_header(app, widths, None, None, cx);
    let Some(df) = &app.snapshot.df else {
        return div()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .child(header)
            .child(empty_state(
                "No disk figures yet",
                "The engine's /system/df is read on its own slower cadence.",
                tokens,
            ))
            .into_any_element();
    };

    let stores = df.stores();
    let free = df.reclaimable_by_store();
    let shares = df.shares();
    let metrics = div()
        .flex()
        .flex_row()
        .w_full()
        .items_end()
        .gap(px(28.0))
        .pb(px(12.0))
        .child(metric("IMAGES", stores.images, tokens))
        .child(metric("CONTAINERS", stores.containers, tokens))
        .child(metric("VOLUMES", stores.volumes, tokens))
        .child(metric("BUILD CACHE", stores.build_cache, tokens))
        .child(metric("RECLAIMABLE", free.total(), tokens));

    let bar = graph::stacked_bar(
        vec![
            (shares.images, tokens.accent_fill()),
            (shares.containers, tokens.success),
            (shares.volumes, tokens.store_volume()),
            (shares.build_cache, tokens.muted),
        ],
        tokens.track,
    );

    let explanation = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(6.0))
        .p(px(16.0))
        .rounded(px(layout::CARD_RADIUS))
        .bg(tokens.surface)
        .border_1()
        .border_color(tokens.border)
        .child(
            div()
                .text_size(px(type_scale::CARD_TITLE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.text)
                .child("What these numbers are"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(tokens.muted)
                .child("Read from /system/df, which walks the image and volume stores. It is cached because it is expensive."),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(tokens.muted)
                .child("On macOS this is space inside the engine's VM. Pruning frees it in the guest; the disk image file itself only shrinks when it is compacted."),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .justify_end()
                .pt(px(2.0))
                .child(button("Clean up unused…", tokens, true, cx)),
        );

    let mut content = div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .child(header)
        .child(metrics)
        .child(bar)
        .child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .gap(px(16.0))
                .py(px(10.0))
                .child(legend(
                    "images",
                    stores.images,
                    tokens.accent_fill(),
                    tokens,
                ))
                .child(legend(
                    "containers",
                    stores.containers,
                    tokens.success,
                    tokens,
                ))
                .child(legend(
                    "volumes",
                    stores.volumes,
                    tokens.store_volume(),
                    tokens,
                ))
                .child(legend(
                    "build cache",
                    stores.build_cache,
                    tokens.muted,
                    tokens,
                )),
        )
        .child(explanation);

    if app.snapshot.df_error.is_empty() {
        content = content.child(
            div()
                .text_size(px(11.0))
                .text_color(tokens.muted)
                .pt(px(8.0))
                .child(format!(
                    "read {} ago",
                    format::ago(app.snapshot.df_at, format::now())
                )),
        );
    } else {
        content = content.child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.warning_text())
                .pt(px(8.0))
                .child(app.snapshot.df_error.clone()),
        );
    }
    content.into_any_element()
}

fn metric(label: &'static str, bytes: i64, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(type_scale::LABEL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .text_size(px(type_scale::METRIC))
                .font_weight(FontWeight::SEMIBOLD)
                .font_features(crate::ui::tabular())
                .text_color(tokens.text)
                .child(format::bytes(bytes.max(0) as u64)),
        )
}

fn legend(label: &str, bytes: i64, color: Hsla, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.0))
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(color))
        .child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.muted)
                .child(format!("{} {}", label, format::bytes(bytes.max(0) as u64))),
        )
}

fn button(
    label: &'static str,
    tokens: Tokens,
    primary: bool,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    div()
        .id(label)
        .flex()
        .items_center()
        .h(px(28.0))
        .px(px(12.0))
        .rounded(px(14.0))
        .bg(if primary {
            tokens.accent
        } else {
            tokens.surface_hover
        })
        .role(gpui::Role::Button)
        .aria_label(label)
        .on_click(cx.listener(|this, _, _, cx| this.open_prune(cx)))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if primary {
                    tokens.accent_text
                } else {
                    tokens.text
                })
                .child(label),
        )
}

#[allow(dead_code)]
fn _df_shape(_: &Df) {}
