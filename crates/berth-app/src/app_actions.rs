//! Action handlers and engine-choice methods.

use crate::app::*;
use crate::settings::Appearance;
use berth_commands::app_actions::*;
use gpui::{Context, Window};

/// The lowest published port, for sorting. A container with none sorts last.
fn first_port(container: &berth_engine::dto::Container) -> u16 {
    container
        .published_ports()
        .first()
        .map_or(u16::MAX, |binding| binding.port)
}

impl BerthApp {
    // --- actions -----------------------------------------------------------

    pub(crate) fn on_show_overview(
        &mut self,
        _: &ShowOverview,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Overview, cx);
    }

    pub(crate) fn on_show_containers(
        &mut self,
        _: &ShowContainers,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Containers, cx);
    }

    pub(crate) fn on_show_images(
        &mut self,
        _: &ShowImages,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Images, cx);
    }

    pub(crate) fn on_show_volumes(
        &mut self,
        _: &ShowVolumes,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Volumes, cx);
    }

    pub(crate) fn on_show_networks(
        &mut self,
        _: &ShowNetworks,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Networks, cx);
    }

    pub(crate) fn on_show_events(
        &mut self,
        _: &ShowEvents,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Events, cx);
    }

    pub(crate) fn on_show_stacks(
        &mut self,
        _: &ShowStacks,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Stacks, cx);
    }

    pub(crate) fn on_show_disk(&mut self, _: &ShowDisk, _: &mut Window, cx: &mut Context<Self>) {
        self.switch_to(Tab::Disk, cx);
    }

    pub(crate) fn on_show_runtimes(
        &mut self,
        _: &ShowRuntimes,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Runtimes, cx);
    }

    pub(crate) fn on_show_settings(
        &mut self,
        _: &ShowSettings,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_to(Tab::Settings, cx);
    }

    pub(crate) fn on_focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = match self.view.tab {
            Tab::Containers => Some(SearchKind::Containers),
            Tab::Images => Some(SearchKind::Images),
            Tab::Volumes => Some(SearchKind::Volumes),
            Tab::Networks => Some(SearchKind::Networks),
            Tab::Events => Some(SearchKind::Events),
            _ => None,
        };
        if let Some(kind) = kind {
            self.view.lists[kind as usize]
                .search
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    pub(crate) fn on_refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh_now(cx);
    }

    pub(crate) fn on_select_next(
        &mut self,
        _: &SelectNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_selection(1, cx);
    }

    /// Tab: the next control that can take focus.
    pub(crate) fn on_focus_next(
        &mut self,
        _: &FocusNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_focus(true, window, cx);
    }

    /// Shift+Tab: the previous one.
    pub(crate) fn on_focus_prev(
        &mut self,
        _: &FocusPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_focus(false, window, cx);
    }

    pub(crate) fn on_select_prev(
        &mut self,
        _: &SelectPrev,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_selection(-1, cx);
    }

    fn step_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.view.tab == Tab::Containers {
            let rows = self.visible_rows();
            if rows.is_empty() {
                return;
            }
            let current = self.view.selected_id.as_deref().and_then(|id| {
                rows.iter()
                    .position(|&ix| self.snapshot.containers[ix].id == id)
            });
            let next = match current {
                Some(pos) => (pos as isize + delta).clamp(0, rows.len() as isize - 1) as usize,
                None if delta >= 0 => 0,
                None => rows.len() - 1,
            };
            self.select(rows[next], cx);
        }
    }

    pub(crate) fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        // The sheet first, whichever it is.
        if self.view.sheet.take().is_some() {
            cx.notify();
            return;
        }
        for search in self.view.lists.iter().map(|list| &list.search) {
            let focused_non_empty =
                search.read(cx).focus_handle().is_focused(window) && !search.read(cx).is_empty();
            if focused_non_empty {
                search.update(cx, |input, cx| input.clear(cx));
                return;
            }
        }
        if self
            .inspector
            .log_search
            .read(cx)
            .focus_handle()
            .is_focused(window)
            && !self.inspector.log_search.read(cx).is_empty()
        {
            self.inspector
                .log_search
                .update(cx, |input, cx| input.clear(cx));
            return;
        }
        if self.view.selected_id.is_some() {
            self.clear_selection(cx);
        }
    }

    pub(crate) fn set_appearance(
        &mut self,
        appearance: Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.appearance = appearance;
        self.tokens = match appearance {
            Appearance::Light => berth_ui::theme::light(),
            Appearance::Dark => berth_ui::theme::dark(),
            Appearance::System => berth_ui::theme::for_appearance(window.appearance()),
        };
        self.save_settings(cx);
        cx.notify();
    }

    pub(crate) fn select_runtime(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(runtime) = self.engine.runtimes.get(index).cloned() else {
            return;
        };
        if !runtime.reachable {
            return;
        }
        self.engine.install(&runtime);
        self.engine.preferred_socket = Some(runtime.socket.clone());
        self.refresh_task = None;
        self.refresh_wanted = false;
        self.clear_selection(cx);
        let mut snapshot = (*self.snapshot).clone();
        snapshot.events.clear();
        snapshot.df = None;
        snapshot.df_at = 0;
        snapshot.df_error.clear();
        self.snapshot = std::sync::Arc::new(snapshot);
        self.save_settings(cx);
        self.start_refresh(cx);
        cx.notify();
    }

    pub(crate) fn use_socket(&mut self, path: &str, cx: &mut Context<Self>) {
        let path = path.strip_prefix("unix://").unwrap_or(path);
        self.engine.preferred_socket = Some(path.to_owned());
        self.save_settings(cx);
        self.start_discovery(cx);
    }

    /// Look for engines again. The engine being read stays live meanwhile, so
    /// the figures on screen keep moving; only a real change of engine starts
    /// them over.
    pub(crate) fn rescan(&mut self, cx: &mut Context<Self>) {
        self.start_discovery(cx);
    }

    pub(crate) fn refresh_now(&mut self, cx: &mut Context<Self>) {
        self.engine.df_at = 0;
        self.start_refresh(cx);
        cx.notify();
    }

    /// Show another subset of containers. A selection that no longer fits the
    /// subset is cleared completely, the way `clear_selection` does it: the
    /// detail job is cancelled, not left to finish against nothing.
    pub(crate) fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        if self.view.filter == filter {
            return;
        }
        self.view.filter = filter;
        if self
            .view
            .selected_id
            .as_deref()
            .and_then(|id| self.snapshot.selected_container(id))
            .is_some_and(|container| !filter.keeps(container))
        {
            self.clear_selection(cx);
        }
        cx.notify();
    }

    /// How a table is sorted: the column, and whether ascending.
    pub(crate) fn sort_for(&self, table: usize) -> (usize, bool) {
        self.view.lists[table].sort
    }

    pub(crate) fn toggle_sort(&mut self, table: usize, col: usize, cx: &mut Context<Self>) {
        self.view.lists[table].toggle_sort(col);
        self.redraw_page(cx);
    }

    /// The rows of `table` that are on screen, in the order it shows them:
    /// each an index into its list in the snapshot. `figure` works them out
    /// once per snapshot, query, sort and filter, and they are kept from one
    /// frame to the next while those hold.
    pub(crate) fn rows_of(
        &self,
        table: SearchKind,
        figure: impl FnOnce() -> Vec<usize>,
    ) -> Vec<usize> {
        let list = &self.view.lists[table as usize];
        list.kept.rows(
            &self.snapshot,
            &list.query,
            list.sort,
            self.view.filter,
            figure,
        )
    }

    /// Bring the row at `ix` of `table`'s list into view: where the table has
    /// scrolled past it, scroll back to it. A row that is on screen stays put.
    pub(crate) fn reveal(&self, table: SearchKind, ix: usize) {
        let list = &self.view.lists[table as usize];
        let position = list
            .kept
            .peek()
            .and_then(|rows| rows.iter().position(|&row| row == ix));
        if let Some(position) = position {
            list.scroll
                .scroll_to_item(position, gpui::ScrollStrategy::Nearest);
        }
    }

    /// The containers on screen, in the order the table shows them.
    pub(crate) fn visible_rows(&self) -> Vec<usize> {
        self.rows_of(SearchKind::Containers, || {
            let query = self.view.lists[SearchKind::Containers as usize]
                .query
                .to_ascii_lowercase();
            let mut rows: Vec<usize> = self
                .snapshot
                .containers
                .iter()
                .enumerate()
                .filter_map(|(index, container)| {
                    (self.view.filter.keeps(container) && container_matches(container, &query))
                        .then_some(index)
                })
                .collect();
            let (col, asc) = self.view.lists[0].sort;
            rows.sort_by(|&a, &b| {
                let ca = &self.snapshot.containers[a];
                let cb = &self.snapshot.containers[b];
                let sa = self.snapshot.stat(a);
                let sb = self.snapshot.stat(b);
                let ord = match col {
                    1 => ca
                        .state
                        .cmp(&cb.state)
                        .then_with(|| ca.status.cmp(&cb.status)),
                    2 => {
                        let ca_cpu = if sa.cpu_known { sa.cpu } else { -1.0 };
                        let cb_cpu = if sb.cpu_known { sb.cpu } else { -1.0 };
                        ca_cpu.total_cmp(&cb_cpu)
                    }
                    3 => sa.mem.cmp(&sb.mem),
                    4 => sa
                        .net_rx
                        .saturating_add(sa.net_tx)
                        .cmp(&sb.net_rx.saturating_add(sb.net_tx)),
                    5 => sa
                        .blk_read
                        .saturating_add(sa.blk_write)
                        .cmp(&sb.blk_read.saturating_add(sb.blk_write)),
                    6 => sa.pids.unwrap_or(0).cmp(&sb.pids.unwrap_or(0)),
                    7 => berth_engine::detail::up_seconds(&ca.status)
                        .unwrap_or(-1)
                        .cmp(&berth_engine::detail::up_seconds(&cb.status).unwrap_or(-1)),
                    8 => first_port(ca).cmp(&first_port(cb)),
                    _ => ca.name().cmp(cb.name()),
                };
                if asc { ord } else { ord.reverse() }
            });
            rows
        })
    }
}
