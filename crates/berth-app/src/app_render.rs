//! The shell frame: dispatch to the current page and the inspector.

use crate::app::*;
use crate::ui::{inspector, rail, sheets, table, toast, views};
use berth_ui::layout;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, FocusHandle, Focusable, IntoElement, Render, Role, Window, div, px,
};

impl BerthApp {
    // --- drawing -----------------------------------------------------------

    /// Headless review hooks: open a sheet without a pointer.
    pub fn show_prune(&mut self, cx: &mut Context<Self>) {
        self.open_prune(cx);
    }

    pub fn show_create(&mut self, kind: CreateKind, cx: &mut Context<Self>) {
        self.open_create(kind, cx);
    }

    pub fn show_toast_for_screenshot(&mut self, cx: &mut Context<Self>) {
        self.notify("Deleted berth-demo-web", false, cx);
    }

    pub fn confirm_first_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(container) = self.snapshot.containers.first() {
            self.confirm_action(
                "DELETE".into(),
                format!("/containers/{}?force=1", container.id),
                "Delete".into(),
                container.name().to_owned(),
                cx,
            );
        }
    }

    pub fn select_fixture_container(&mut self, tab: InspectorTab, cx: &mut Context<Self>) {
        self.select(0, cx);
        self.inspector_tab = tab;
        self.logs_loading = false;
        self.inspect_loading = false;
        self.logs_text =
            "2026-11-07 15:20:01 [notice] 1#1: nginx/1.27.2\n2026-11-07 15:20:01 [notice] 1#1: start worker processes\n172.18.0.1 - - [07/Nov/2026:15:21:10 +0000] \"GET / HTTP/1.1\" 200 615".into();
        self.detail_lifetime = Some(berth_engine::detail::Lifetime {
            started_at: Some(berth_core::format::now() - 18 * 3600),
            restarts: 0,
            exit_code: None,
            oom_killed: false,
            restart_policy: Some("unless-stopped".into()),
            health_log: None,
            command: Some("nginx -g 'daemon off;'".into()),
            workdir: Some("/usr/share/nginx/html".into()),
            env: vec![
                ("NGINX_HOST".into(), "localhost".into(), false),
                ("NGINX_PORT".into(), "80".into(), false),
                ("API_SECRET_TOKEN".into(), "<redacted>".into(), true),
            ],
            mounts: vec![berth_engine::detail::MountSummary {
                kind: "volume".into(),
                source: "shop-data".into(),
                destination: "/var/www/html".into(),
                rw: true,
            }],
            ips: vec![("shop_default".into(), "172.18.0.2".into())],
            run_cmd: Some(
                "docker run -d --name berth-demo-web -p 8080:80 -v shop-data:/var/www/html nginx:alpine".into(),
            ),
            run_redacted: true,
        });
        self.detail_top = Some(berth_engine::dto::ContainerTop {
            titles: vec!["PID".into(), "USER".into(), "TIME".into(), "COMMAND".into()],
            processes: vec![
                vec![
                    "1042".into(),
                    "root".into(),
                    "0:00".into(),
                    "nginx: master process nginx -g daemon off;".into(),
                ],
                vec![
                    "1089".into(),
                    "nginx".into(),
                    "0:02".into(),
                    "nginx: worker process".into(),
                ],
            ],
        });
        self.detail_changes = vec![
            berth_engine::dto::FsChange {
                path: "/var/cache/nginx/client_temp".into(),
                kind: 1,
            },
            berth_engine::dto::FsChange {
                path: "/etc/nginx/conf.d/default.conf".into(),
                kind: 0,
            },
            berth_engine::dto::FsChange {
                path: "/run/nginx.pid".into(),
                kind: 1,
            },
        ];
        cx.notify();
    }

    pub fn select_fixture_image(&mut self, cx: &mut Context<Self>) {
        if let Some(image) = self.snapshot.images.first().cloned() {
            self.select_image(&image.id, image.label(), cx);
            self.image_layers_loading = false;
            self.image_layers = vec![
                berth_engine::dto::ImageLayer {
                    id: "sha256:a1".into(),
                    created: berth_core::format::now() - 86_400 * 12,
                    created_by: "/bin/sh -c #(nop) CMD [\"nginx\" \"-g\" \"daemon off;\"]".into(),
                    size: 0,
                    comment: String::new(),
                },
                berth_engine::dto::ImageLayer {
                    id: "sha256:a2".into(),
                    created: berth_core::format::now() - 86_400 * 12,
                    created_by: "/bin/sh -c apk add --no-cache nginx-mod-http-js".into(),
                    size: 40 * 1024 * 1024,
                    comment: String::new(),
                },
                berth_engine::dto::ImageLayer {
                    id: "sha256:a3".into(),
                    created: berth_core::format::now() - 86_400 * 30,
                    created_by: "/bin/sh -c #(nop) ADD file:alpine-minirootfs-3.22.tar.gz in /"
                        .into(),
                    size: 8 * 1024 * 1024,
                    comment: String::new(),
                },
            ];
            cx.notify();
        }
    }

    fn detail_pane(
        &self,
        pane_width: f32,
        tokens: Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.selected_id.as_deref()?;
        let element = match self.detail_kind {
            DetailKind::Container => {
                let index = self
                    .snapshot
                    .containers
                    .iter()
                    .position(|container| container.id == id)?;
                inspector::container_pane(
                    self,
                    &self.snapshot.containers[index],
                    self.snapshot.stat(index),
                    pane_width,
                    tokens,
                    cx,
                )
                .into_any_element()
            }
            DetailKind::Image => inspector::image_pane(
                self,
                self.snapshot.selected_image(id)?,
                pane_width,
                tokens,
                cx,
            )
            .into_any_element(),
            DetailKind::Volume => inspector::volume_pane(
                self,
                self.snapshot.selected_volume(id)?,
                pane_width,
                tokens,
                cx,
            )
            .into_any_element(),
            DetailKind::Network => {
                let network = self.snapshot.selected_network(id)?;
                inspector::network_pane(
                    self,
                    network,
                    self.snapshot.network_counts.get(&network.name),
                    pane_width,
                    tokens,
                    cx,
                )
                .into_any_element()
            }
        };
        Some(element)
    }
}

impl Focusable for BerthApp {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BerthApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_width: f32 = window.viewport_size().width.into();
        let widths = layout::for_window(window_width, self.selected_id.is_some());
        let tokens = self.tokens;
        let columns = layout::container_columns(widths.content, self.bulk_enabled);

        let body = match self.tab {
            Tab::Containers => {
                let page = table::page_header(
                    self,
                    widths,
                    Some(SearchKind::Containers),
                    Some("Select"),
                    cx,
                );
                let header = table::table_header(self, columns, cx);
                let table = table::table(self, columns, cx);
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(page)
                    .child(header)
                    .child(table)
                    .into_any_element()
            }
            Tab::Images => views::images::images(self, widths, cx),
            Tab::Volumes => views::volumes::volumes(self, widths, cx),
            Tab::Networks => views::networks::networks(self, widths, cx),
            Tab::Events => views::events::events(self, widths, cx),
            Tab::Overview => views::overview::overview(self, widths, cx),
            Tab::Disk => views::disk::disk(self, widths, cx),
            Tab::Stacks => views::stacks::stacks(self, widths, cx),
            Tab::Runtimes => views::runtimes::runtimes(self, widths, cx),
            Tab::Settings => views::settings::settings(self, widths, cx),
        };

        let main = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .px(px(20.0))
            .pt(px(16.0))
            .pb(px(12.0))
            .gap(px(4.0))
            .bg(tokens.background)
            .child(
                div()
                    .id("page-scroll")
                    .flex()
                    .flex_col()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(body),
            );

        div()
            .id("berth-root")
            .role(Role::Application)
            .aria_label("berth")
            .track_focus(&self.focus_handle)
            .key_context("Berth")
            .on_action(cx.listener(Self::on_show_overview))
            .on_action(cx.listener(Self::on_show_containers))
            .on_action(cx.listener(Self::on_show_images))
            .on_action(cx.listener(Self::on_show_volumes))
            .on_action(cx.listener(Self::on_show_networks))
            .on_action(cx.listener(Self::on_show_events))
            .on_action(cx.listener(Self::on_show_stacks))
            .on_action(cx.listener(Self::on_show_disk))
            .on_action(cx.listener(Self::on_show_runtimes))
            .on_action(cx.listener(Self::on_show_settings))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_refresh))
            .on_action(cx.listener(Self::on_dismiss))
            .on_action(cx.listener(Self::on_select_next))
            .on_action(cx.listener(Self::on_select_prev))
            .flex()
            .flex_row()
            .size_full()
            .bg(tokens.background)
            .text_color(tokens.text)
            .text_size(px(type_scale::BODY))
            .relative()
            .child(rail::rail(self, widths, cx))
            .child(main)
            .children(self.detail_pane(widths.pane, tokens, cx))
            .children(toast::overlay(self, tokens, cx))
            .children(sheets::overlay(self, tokens, cx))
    }
}

pub(crate) fn pretty_inspect(detail: &berth_engine::detail::ContainerDetail) -> String {
    serde_json::to_string_pretty(detail).unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use berth_ui::theme;

    #[gpui::test]
    fn switching_pages_clears_a_selection(cx: &mut gpui::TestAppContext) {
        let (app, cx) = cx.add_window_view(|_, cx| BerthApp::new_fixture(theme::light(), cx));
        app.update(cx, |app, cx| {
            app.select(0, cx);
            assert!(app.selected_id.is_some());
            app.switch_to(Tab::Images, cx);
        });
        let (tab, selected) = app.read_with(&*cx, |app, _| (app.tab, app.selected_id.clone()));
        assert_eq!(tab, Tab::Images);
        assert!(selected.is_none());
    }

    #[gpui::test]
    fn dismiss_clears_the_inspector(cx: &mut gpui::TestAppContext) {
        let (app, cx) = cx.add_window_view(|_, cx| BerthApp::new_fixture(theme::light(), cx));
        app.update(cx, |app, cx| {
            app.select(1, cx);
            app.clear_selection(cx);
        });
        let selected = app.read_with(&*cx, |app, _| app.selected_id.clone());
        assert!(selected.is_none());
    }
}
