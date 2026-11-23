//! Disk: what the engine's stores hold and what a cleanup could reclaim.

use berth_core::format;
use berth_engine::df::Store;
use berth_engine::discovery::Kind;
use berth_ui::layout::Widths;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Hsla, div, px};

use crate::app::BerthApp;
use crate::ui::graph;
use crate::ui::kit::{button, caption, card, card_title};
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

    let dialect = app.engine.dialect;
    let figure = |store: Store| format::measured(df.store(dialect, store));
    let free = df.reclaimable_by_store();
    let shares = df.shares();
    let metrics = div()
        .flex()
        .flex_row()
        .w_full()
        .items_end()
        .gap(px(28.0))
        .pb(px(12.0))
        .child(metric("IMAGES", figure(Store::Images), tokens))
        .child(metric("CONTAINERS", figure(Store::Containers), tokens))
        .child(metric("VOLUMES", figure(Store::Volumes), tokens))
        .child(metric("BUILD CACHE", figure(Store::BuildCache), tokens))
        .child(metric(
            "RECLAIMABLE",
            format::measured(Some(free.total())),
            tokens,
        ));

    let bar = graph::stacked_bar(
        vec![
            (shares.images, tokens.accent_fill()),
            (shares.containers, tokens.success),
            (shares.volumes, tokens.store_volume()),
            (shares.build_cache, tokens.muted),
        ],
        tokens.track,
    );

    let engine_kind = app.engine.active_runtime().map(|runtime| runtime.kind);
    let explanation = card(tokens)
        .child(card_title("What these numbers are", tokens))
        .child(caption(
            "Read from /system/df, which walks the image and volume stores. It is cached because it is expensive.",
            tokens,
        ))
        .child(caption(location_note(engine_kind), tokens))
        .child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .justify_end()
                // Opening the cleanup asks first; it destroys nothing itself.
                .child(
                    button("disk-cleanup", "Clean up unused…", tokens)
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.open_prune(cx))),
                ),
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
                    figure(Store::Images),
                    tokens.accent_fill(),
                    tokens,
                ))
                .child(legend(
                    "containers",
                    figure(Store::Containers),
                    tokens.success,
                    tokens,
                ))
                .child(legend(
                    "volumes",
                    figure(Store::Volumes),
                    tokens.store_volume(),
                    tokens,
                ))
                .child(legend(
                    "build cache",
                    figure(Store::BuildCache),
                    tokens.muted,
                    tokens,
                )),
        )
        .child(explanation);

    if app.snapshot.df_error.is_empty() {
        content = content.child(
            div()
                .text_size(px(type_scale::LABEL))
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
                .text_size(px(type_scale::SECONDARY))
                .text_color(tokens.warning_text())
                .pt(px(8.0))
                .child(app.snapshot.df_error.clone()),
        );
    }
    content.into_any_element()
}

/// Where the engine's disk is. That decides what pruning does to the host's
/// free space: inside a virtual machine it frees space in the guest, and the
/// disk image only shrinks when it is compacted.
pub(crate) fn location_note(engine: Option<Kind>) -> &'static str {
    match engine {
        Some(kind) if berth_engine::discovery::vm_backed(kind) => {
            "This is space inside the engine's virtual machine. Pruning frees it there; the disk image on the host only shrinks when it is compacted."
        }
        Some(_) => {
            "This is space on the host, in the engine's data directory. Pruning frees it at once."
        }
        None => "These are the engine's stores. Pruning frees what they no longer need.",
    }
}

fn metric(label: &'static str, figure: String, tokens: Tokens) -> Div {
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
                .child(figure),
        )
}

fn legend(label: &str, figure: String, color: Hsla, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.0))
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(color))
        .child(
            div()
                .text_size(px(type_scale::SECONDARY))
                .text_color(tokens.muted)
                .child(format!("{label} {figure}")),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_says_where_the_disk_really_is() {
        // A machine that runs in a VM frees space inside it, not on the host.
        assert!(location_note(Some(Kind::Lima)).contains("virtual machine"));
        assert!(location_note(Some(Kind::DockerDesktop)).contains("virtual machine"));
        // A native daemon frees space on the host at once.
        assert!(location_note(Some(Kind::Dockerd)).contains("on the host"));
        assert!(location_note(Some(Kind::Podman)).contains("on the host"));
        assert!(!location_note(Some(Kind::Dockerd)).contains("VM"));
        assert!(location_note(None).contains("engine's stores"));
    }
}
