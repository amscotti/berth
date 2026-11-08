//! Runtimes: the sockets berth found, and the one it is reading.

use berth_engine::discovery::{Kind, Runtime};
use berth_ui::layout::{self, Widths};
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, Role, div, px};

use crate::app::BerthApp;
use crate::ui::table::{empty_state, page_header};

pub fn runtimes(app: &BerthApp, widths: Widths, cx: &mut Context<BerthApp>) -> AnyElement {
    let tokens = app.tokens;
    let header = page_header(app, widths, None, None, cx);
    let mut content = div()
        .flex()
        .flex_col()
        .w_full()
        .flex_1()
        .gap(px(10.0))
        .child(header);

    content = content.child(custom_socket(app, tokens, cx));
    if app.engine.runtimes.is_empty() {
        content = content.child(empty_state(
            if app.engine.loading {
                "Reading engine…"
            } else {
                "No sockets found"
            },
            "berth probes Docker Desktop, OrbStack, Rancher, Colima, Lima and Podman sockets.",
            tokens,
        ));
        return content.into_any_element();
    }

    for (index, runtime) in app.engine.runtimes.iter().enumerate() {
        content = content.child(runtime_card(app, index, runtime, tokens, cx));
    }
    content.into_any_element()
}

fn custom_socket(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> Div {
    let mut card = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(8.0))
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
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .child("Custom socket"),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(tokens.muted)
                        .child("unix:// is optional"),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .gap(px(8.0))
                .child(app.socket_input.clone())
                .child(
                    div()
                        .id("connect-socket")
                        .flex()
                        .items_center()
                        .h(px(28.0))
                        .px(px(14.0))
                        .rounded(px(14.0))
                        .bg(tokens.accent)
                        .role(Role::Button)
                        .aria_label("Connect")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let path = this.socket_input.read(cx).text().trim().to_owned();
                            if !path.is_empty() {
                                this.use_socket(&path, cx);
                            }
                        }))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(tokens.accent_text)
                                .child("Connect"),
                        ),
                ),
        );
    card = card.child(
        div().flex().flex_row().w_full().justify_end().child(
            div()
                .id("rescan")
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(13.0))
                .bg(tokens.surface_hover)
                .role(Role::Button)
                .aria_label("Rescan sockets")
                .on_click(cx.listener(|this, _, _, cx| this.rescan(cx)))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(tokens.text)
                        .child("Rescan sockets"),
                ),
        ),
    );
    card
}

fn runtime_card(
    app: &BerthApp,
    index: usize,
    runtime: &Runtime,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let active = index == app.engine.active;
    let mut card = div()
        .id(("runtime", index))
        .flex()
        .flex_col()
        .w_full()
        .gap(px(6.0))
        .p(px(14.0))
        .rounded(px(layout::CARD_RADIUS))
        .bg(if active {
            tokens.accent.alpha(0.08)
        } else {
            tokens.surface
        })
        .border_1()
        .border_color(if active { tokens.accent } else { tokens.border })
        .role(Role::Button)
        .aria_label(runtime.name.clone())
        .on_click(cx.listener(move |this, _, _, cx| this.select_runtime(index, cx)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(chip(tokens.muted))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .child(runtime.name.clone()),
                )
                .when(active, |this| {
                    this.child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(tokens.accent_text_pill())
                            .child("active"),
                    )
                })
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(if runtime.reachable {
                            tokens.success_text()
                        } else {
                            tokens.danger_text()
                        })
                        .child(if runtime.reachable {
                            "answering".to_owned()
                        } else {
                            "not answering".to_owned()
                        }),
                ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .font_family("Menlo")
                .text_color(tokens.muted)
                .child(runtime.socket.clone()),
        );
    if runtime.reachable {
        card = card.child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.muted)
                .child(format!(
                    "v{} · API {} · {} containers",
                    runtime.version.version,
                    runtime.version.api_version,
                    runtime.info.containers_running
                )),
        );
    } else if !runtime.error_text.is_empty() {
        card = card.child(
            div()
                .text_size(px(11.0))
                .text_color(tokens.danger_text())
                .child(runtime.error_text.clone()),
        );
    }
    if ops_supported(runtime.kind) {
        card = card.child(
            div()
                .flex()
                .flex_row()
                .gap(px(6.0))
                .pt(px(2.0))
                .child(small_button("Start", index, runtime, tokens, cx))
                .child(small_button("Stop", index, runtime, tokens, cx))
                .child(small_button("Restart", index, runtime, tokens, cx)),
        );
    }
    card
}

fn ops_supported(kind: Kind) -> bool {
    matches!(kind, Kind::Lima | Kind::Colima | Kind::PodmanMachine)
}

fn chip(color: gpui::Hsla) -> Div {
    div()
        .w(px(20.0))
        .h(px(20.0))
        .rounded(px(5.0))
        .bg(color.alpha(0.14))
        .flex()
        .items_center()
        .justify_center()
        .child(div().w(px(8.0)).h(px(8.0)).rounded(px(2.0)).bg(color))
}

fn small_button(
    label: &'static str,
    index: usize,
    runtime: &Runtime,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let verb = label.to_ascii_lowercase();
    let kind = runtime.kind;
    let socket = runtime.socket.clone();
    let id = gpui::SharedString::from(format!("runtime-op-{index}-{verb}"));
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(12.0))
        .bg(tokens.surface_hover)
        .role(Role::Button)
        .aria_label(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.open_engine_op(&verb, kind, socket.clone(), cx);
        }))
        .child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.text)
                .child(label),
        )
}
