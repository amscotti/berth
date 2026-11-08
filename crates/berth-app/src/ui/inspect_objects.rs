//! Image, volume and network inspectors with cross-linked containers and image layers.

use berth_core::format;
use berth_engine::dto::{Image, Network, Volume};
use berth_ui::theme::Tokens;
use gpui::prelude::*;
use gpui::{Context, Div, FontWeight, IntoElement, Role, SharedString, div, px};

use super::inspect_shared::*;
use super::table::pill;
use super::tabular;
use crate::app::BerthApp;

pub(crate) fn image_pane(
    app: &BerthApp,
    image: &Image,
    width: f32,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let name = if image.dangling() {
        "untagged"
    } else {
        image.tag()
    };
    let mut pane = pane_shell(name, "Image inspector", width, tokens, cx)
        .child(fact("Repository", name, tokens))
        .child(fact("ID", &short_id(&image.id), tokens))
        .child(fact("Size", &format::bytes(image.size), tokens))
        .child(fact(
            "Created",
            &format::ago(image.created, format::now()),
            tokens,
        ))
        .child(fact(
            "Status",
            if image.dangling() {
                "dangling"
            } else {
                "ready"
            },
            tokens,
        ));

    let users: Vec<_> = app
        .snapshot
        .containers
        .iter()
        .filter(|c| c.uses_image(image))
        .collect();
    pane = pane.child(section_title("Used by", users.len(), tokens));
    if users.is_empty() {
        pane = pane.child(muted_note("No containers use this image.", tokens));
    } else {
        let mut list = div().flex().flex_col().w_full().gap(px(4.0));
        for container in users {
            list = list.child(linked_container_row(
                &container.id,
                container.name(),
                None,
                container.health().unwrap_or(&container.state),
                tokens,
                cx,
            ));
        }
        pane = pane.child(list);
    }

    pane = pane.child(section_title("Layers", app.image_layers.len(), tokens));
    if app.image_layers.is_empty() {
        let note = if app.image_layers_loading {
            "Reading image history…"
        } else {
            "Layer history loads from the live engine."
        };
        pane = pane.child(muted_note(note, tokens));
    } else {
        let mut layers_well = div()
            .id("image-layers-well")
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .gap(px(4.0))
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(tokens.background)
            .border_1()
            .border_color(tokens.border);
        for layer in &app.image_layers {
            let cmd = clean_layer_cmd(&layer.created_by);
            layers_well = layers_well.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(10.5))
                            .font_family("Menlo")
                            .text_color(tokens.text)
                            .child(cmd),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_features(tabular())
                            .text_color(tokens.muted)
                            .child(if layer.size > 0 {
                                format::bytes(layer.size)
                            } else {
                                "0 B".to_owned()
                            }),
                    ),
            );
        }
        pane = pane.child(layers_well);
    }

    let name = name.to_owned();
    let id = image.id.clone();
    let label = image.label().to_owned();
    if app.image_layers.is_empty() {
        pane = pane.child(div().flex_1());
    }
    pane.child(
        div()
            .flex()
            .flex_row()
            .w_full()
            .gap(px(6.0))
            .child(verb_button(
                "Run…",
                tokens,
                cx.listener({
                    let label = label.clone();
                    move |this, _, _, cx| {
                        this.creation_open = true;
                        this.run_image
                            .update(cx, |input, cx| input.set_value(label.clone(), cx));
                        cx.notify();
                    }
                }),
            ))
            .child(delete_button(
                "Delete",
                tokens,
                cx.listener(move |this, _, _, cx| {
                    this.confirm_action(
                        "DELETE".into(),
                        format!("/images/{}?force=1", berth_engine::client::encode(&id)),
                        "Delete".into(),
                        name.clone(),
                        cx,
                    );
                }),
            )),
    )
}

pub(crate) fn volume_pane(
    app: &BerthApp,
    volume: &Volume,
    width: f32,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let mut pane = pane_shell(&volume.name, "Volume inspector", width, tokens, cx)
        .child(fact("Name", &volume.name, tokens))
        .child(fact("Driver", &volume.driver, tokens))
        .child(fact("Scope", &volume.scope, tokens))
        .child(fact("Mountpoint", &volume.mountpoint, tokens));
    if let Some(usage) = &volume.usage_data {
        pane = pane
            .child(fact(
                "Size",
                &if usage.size >= 0 {
                    format::bytes(usage.size as u64)
                } else {
                    "—".to_owned()
                },
                tokens,
            ))
            .child(fact("In use", &usage.ref_count.to_string(), tokens));
    } else {
        pane = pane
            .child(fact("Size", "—", tokens))
            .child(fact("In use", "—", tokens));
    }

    let mounts: Vec<_> = app
        .snapshot
        .containers
        .iter()
        .filter_map(|c| c.mount_for_volume(&volume.name).map(|m| (c, m)))
        .collect();
    pane = pane.child(section_title("Mounted by", mounts.len(), tokens));
    if mounts.is_empty() {
        pane = pane.child(muted_note(
            "No containers currently mount this volume.",
            tokens,
        ));
    } else {
        let mut list = div().flex().flex_col().w_full().gap(px(4.0));
        for (container, mount) in mounts {
            let detail = format!(
                "{} ({})",
                mount.destination,
                if mount.rw { "rw" } else { "ro" }
            );
            list = list.child(linked_container_row(
                &container.id,
                container.name(),
                Some(&detail),
                container.health().unwrap_or(&container.state),
                tokens,
                cx,
            ));
        }
        pane = pane.child(list);
    }

    let name = volume.name.clone();
    pane.child(div().flex_1()).child(delete_button(
        "Remove",
        tokens,
        cx.listener(move |this, _, _, cx| {
            this.confirm_action(
                "DELETE".into(),
                format!("/volumes/{}", berth_engine::client::encode(&name)),
                "Remove".into(),
                name.clone(),
                cx,
            );
        }),
    ))
}

pub(crate) fn network_pane(
    app: &BerthApp,
    network: &Network,
    attached: usize,
    width: f32,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let mut pane = pane_shell(&network.name, "Network inspector", width, tokens, cx)
        .child(fact("Name", &network.name, tokens))
        .child(fact("ID", &short_id(&network.id), tokens))
        .child(fact(
            "Driver",
            if network.driver == "null" {
                "—"
            } else {
                &network.driver
            },
            tokens,
        ))
        .child(fact("Scope", &network.scope, tokens))
        .child(fact("Attached", &attached.to_string(), tokens));

    let connected: Vec<_> = app
        .snapshot
        .containers
        .iter()
        .filter_map(|c| c.network_ip(network).map(|ip| (c, ip)))
        .collect();
    pane = pane.child(section_title(
        "Connected containers",
        connected.len(),
        tokens,
    ));
    if connected.is_empty() {
        pane = pane.child(muted_note(
            "No containers attached to this network.",
            tokens,
        ));
    } else {
        let mut list = div().flex().flex_col().w_full().gap(px(4.0));
        for (container, ip) in connected {
            list = list.child(linked_container_row(
                &container.id,
                container.name(),
                (!ip.is_empty()).then_some(ip),
                container.health().unwrap_or(&container.state),
                tokens,
                cx,
            ));
        }
        pane = pane.child(list);
    }

    pane = pane.child(div().flex_1());
    if network.builtin() {
        pane = pane.child(
            div()
                .text_size(px(11.5))
                .text_color(tokens.muted)
                .child("Built-in networks cannot be removed."),
        );
    } else {
        let id = if network.id.is_empty() {
            network.name.clone()
        } else {
            network.id.clone()
        };
        let name = network.name.clone();
        pane = pane.child(delete_button(
            "Remove",
            tokens,
            cx.listener(move |this, _, _, cx| {
                this.confirm_action(
                    "DELETE".into(),
                    format!("/networks/{}", berth_engine::client::encode(&id)),
                    "Remove".into(),
                    name.clone(),
                    cx,
                );
            }),
        ));
    }
    pane
}

fn linked_container_row(
    id: &str,
    name: &str,
    detail: Option<&str>,
    state: &str,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let click_id = id.to_owned();
    div()
        .id(SharedString::from(format!("linked-container-{id}")))
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .h(px(28.0))
        .px(px(8.0))
        .gap(px(8.0))
        .rounded(px(6.0))
        .bg(tokens.background)
        .border_1()
        .border_color(tokens.border)
        .hover(move |style| style.bg(tokens.surface_hover))
        .role(Role::Button)
        .aria_label(format!("Inspect container {name}"))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_container_by_id(&click_id, cx);
        }))
        .child(
            div()
                .truncate()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tokens.text)
                .child(name.to_owned()),
        )
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(10.5))
                    .font_family("Menlo")
                    .text_color(tokens.muted)
                    .child(detail.to_owned()),
            )
        })
        .when(detail.is_none(), |this| this.child(div().flex_1()))
        .child(pill(state, tokens))
}

fn section_title(title: &'static str, count: usize, tokens: Tokens) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .pt(px(6.0))
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.muted)
                .child(title),
        )
        .child(
            div()
                .text_size(px(10.5))
                .font_features(tabular())
                .text_color(tokens.muted.alpha(0.75))
                .child(format!("({count})")),
        )
}

fn muted_note(text: &str, tokens: Tokens) -> Div {
    div()
        .text_size(px(11.0))
        .text_color(tokens.muted)
        .child(text.to_owned())
}

fn clean_layer_cmd(raw: &str) -> String {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("/bin/sh -c #(nop) ")
        .or_else(|| trimmed.strip_prefix("/bin/sh -c "))
        .unwrap_or(trimmed)
        .trim();
    if stripped.is_empty() {
        "<layer>".to_owned()
    } else {
        stripped.to_owned()
    }
}
