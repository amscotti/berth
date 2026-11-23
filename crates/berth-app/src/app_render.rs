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
    fn detail_pane(
        &self,
        pane_width: f32,
        tokens: Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.view.selected_id.as_deref()?;
        let element = match self.view.detail_kind {
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
        self.view.focus_handle.clone()
    }
}

impl Render for BerthApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.place_focus(window, cx);
        let window_width: f32 = window.viewport_size().width.into();
        let widths = layout::for_window(window_width, self.view.selected_id.is_some());
        let tokens = self.tokens;
        let columns = layout::container_columns(widths.content, self.view.bulk_enabled);

        let body = match self.view.tab {
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

        // The page: rail, table and inspector. Its shortcuts are bound here
        // and not on the root, so that a sheet, which is not inside it, owns
        // the keyboard while it is open.
        let page = div()
            .id("berth-page")
            .track_focus(&self.view.focus_handle)
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
            .on_action(cx.listener(Self::on_select_next))
            .on_action(cx.listener(Self::on_select_prev))
            .flex()
            .flex_row()
            .w_full()
            .h_full()
            .min_w(px(0.0))
            .child(rail::rail(self, widths, cx))
            .child(main)
            .children(self.detail_pane(widths.pane, tokens, cx))
            .children(toast::overlay(self, tokens, cx));

        div()
            .id("berth-root")
            .role(Role::Application)
            .aria_label("berth")
            // What the shell answers with a sheet open as well: the way out
            // of it, and moving within it.
            .on_action(cx.listener(Self::on_dismiss))
            .on_action(cx.listener(Self::on_focus_next))
            .on_action(cx.listener(Self::on_focus_prev))
            .flex()
            .flex_row()
            .size_full()
            .bg(tokens.background)
            .text_color(tokens.text)
            .text_size(px(type_scale::BODY))
            .relative()
            .child(page)
            .children(sheets::overlay(self, tokens, cx))
    }
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
            assert!(app.view.selected_id.is_some());
            app.switch_to(Tab::Images, cx);
        });
        let (tab, selected) =
            app.read_with(&*cx, |app, _| (app.view.tab, app.view.selected_id.clone()));
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
        let selected = app.read_with(&*cx, |app, _| app.view.selected_id.clone());
        assert!(selected.is_none());
    }
}
