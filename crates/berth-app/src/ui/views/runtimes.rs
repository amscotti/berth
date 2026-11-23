//! Runtimes: the sockets berth found, and the one it is reading.

use berth_engine::discovery::{Kind, Runtime};
use berth_ui::layout::{self, Widths};
use berth_ui::theme::{TintKind, Tokens, tint};
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, px};

use crate::app::BerthApp;
use crate::ui::kit::{Button, button, caption, card, card_header, icon_chip};
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

fn custom_socket(app: &BerthApp, tokens: Tokens, cx: &mut Context<BerthApp>) -> impl IntoElement {
    card(tokens)
        .child(card_header(
            "Custom socket",
            Some(caption("unix:// is optional", tokens).into_any_element()),
            tokens,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .gap(px(8.0))
                .child(app.socket_input.clone())
                .child(
                    button("connect-socket", "Connect", tokens)
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            let path = this.socket_input.read(cx).text().trim().to_owned();
                            if !path.is_empty() {
                                this.use_socket(&path, cx);
                            }
                        })),
                ),
        )
        .child(
            div().flex().flex_row().w_full().justify_end().child(
                button("rescan", "Rescan sockets", tokens)
                    .on_click(cx.listener(|this, _, _, cx| this.rescan(cx))),
            ),
        )
}

/// One engine. The card itself isn't a control: it holds one, so that the
/// buttons inside it aren't buttons inside a button.
fn runtime_card(
    app: &BerthApp,
    index: usize,
    runtime: &Runtime,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let active = app.engine.active_index() == Some(index);
    let tint = tint(tokens, TintKind::Runtimes);
    // The active engine's card is tinted, and its buttons are drawn for that.
    let ground = if active {
        tokens.active_wash()
    } else {
        tokens.surface
    };

    let choose: AnyElement = if active {
        div()
            .flex()
            .items_center()
            .h(px(layout::CONTROL_SMALL))
            .px(px(9.0))
            .rounded(px(layout::CONTROL_SMALL / 2.0))
            .bg(tokens.accent_wash())
            .text_size(px(type_scale::LABEL))
            .font_weight(FontWeight::MEDIUM)
            .text_color(tokens.accent_text_pill())
            .child("active")
            .into_any_element()
    } else if runtime.reachable {
        button(
            SharedString::from(format!("runtime-use-{index}")),
            "Use this engine",
            tokens,
        )
        .small()
        .on_click(cx.listener(move |this, _, _, cx| this.select_runtime(index, cx)))
        .into_any_element()
    } else {
        div().into_any_element()
    };

    let mut card = card(tokens)
        .gap(px(6.0))
        .p(px(14.0))
        .bg(ground)
        .border_color(if active { tokens.accent } else { tokens.border })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(icon_chip("runtimes", 12.0, tint, tokens, layout::CHIP))
                .child(
                    div()
                        .text_size(px(type_scale::BODY))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.text)
                        .child(runtime.name.clone()),
                )
                .child(div().flex_1())
                .child(choose)
                .child(
                    div()
                        .text_size(px(type_scale::LABEL))
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
                .text_size(px(type_scale::MONO))
                .font_family(type_scale::MONO_FAMILY)
                .text_color(tokens.muted)
                .child(runtime.socket.clone()),
        );
    if runtime.reachable {
        card = card.child(caption(
            format!(
                "v{} · API {} · {} containers",
                runtime.version.version,
                runtime.version.api_version,
                runtime.info.containers_running
            ),
            tokens,
        ));
    } else if !runtime.error_text.is_empty() {
        card = card.child(
            div()
                .text_size(px(type_scale::LABEL))
                .text_color(tokens.danger_text())
                .child(runtime.error_text.clone()),
        );
    }
    if ops_supported(runtime.kind) {
        let mut row = div().flex().flex_row().gap(px(6.0)).pt(px(2.0));
        for verb in machine_ops(runtime) {
            row = row.child(machine_button(verb, index, runtime, ground, tokens, cx));
        }
        card = card.child(row);
    }
    card
}

fn ops_supported(kind: Kind) -> bool {
    matches!(kind, Kind::Lima | Kind::Colima | Kind::PodmanMachine)
}

/// What can be done to a machine. One that is answering can be stopped or
/// restarted; one that isn't can only be started. Offering the others is
/// offering a refusal.
pub(crate) fn machine_ops(runtime: &Runtime) -> &'static [&'static str] {
    if !ops_supported(runtime.kind) {
        return &[];
    }
    if runtime.reachable {
        &["Stop", "Restart"]
    } else {
        &["Start"]
    }
}

/// A machine command. Stopping a machine takes every container on it down,
/// so it is drawn as the destructive action it is.
fn machine_button(
    label: &str,
    index: usize,
    runtime: &Runtime,
    on: gpui::Hsla,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> Button {
    let verb = label.to_ascii_lowercase();
    let kind = runtime.kind;
    let socket = runtime.socket.clone();
    // The names come from `machine_ops`, so they are known at compile time.
    let label = match label {
        "Stop" => "Stop",
        "Restart" => "Restart",
        _ => "Start",
    };
    let button = button(
        SharedString::from(format!("runtime-op-{index}-{verb}")),
        label,
        tokens,
    )
    .small();
    let button = if label == "Stop" {
        // A wash is a tint of what it is on, and on the active engine's card
        // that isn't a plain card.
        button.danger_wash().on_surface(on)
    } else {
        button
    };
    button.on_click(cx.listener(move |this, _, _, cx| {
        this.open_engine_op(&verb, kind, socket.clone(), cx);
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_offers_only_what_its_state_allows() {
        let mut runtime = Runtime {
            kind: Kind::Lima,
            reachable: true,
            ..Default::default()
        };
        assert_eq!(machine_ops(&runtime), ["Stop", "Restart"]);
        runtime.reachable = false;
        assert_eq!(machine_ops(&runtime), ["Start"]);
        // Not a machine at all.
        runtime.kind = Kind::Dockerd;
        assert!(machine_ops(&runtime).is_empty());
    }
}
