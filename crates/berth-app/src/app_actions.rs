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

fn network_key(network: &berth_engine::dto::Network) -> &str {
    if network.id.is_empty() {
        network.name.as_str()
    } else {
        network.id.as_str()
    }
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
        let step = |current: Option<usize>, len: usize| match current {
            Some(pos) => (pos as isize + delta).clamp(0, len as isize - 1) as usize,
            None if delta >= 0 => 0,
            None => len - 1,
        };
        match self.view.tab {
            Tab::Containers => {
                let rows = self.visible_rows();
                if rows.is_empty() {
                    return;
                }
                let current = self.view.selected_id.as_deref().and_then(|id| {
                    rows.iter()
                        .position(|&ix| self.snapshot.containers[ix].id == id)
                });
                self.select(rows[step(current, rows.len())], cx);
            }
            Tab::Images => {
                let rows = self.visible_images();
                if rows.is_empty() {
                    return;
                }
                let current = self.view.selected_id.as_deref().and_then(|id| {
                    rows.iter()
                        .position(|&ix| self.snapshot.images[ix].id == id)
                });
                let ix = rows[step(current, rows.len())];
                let (id, label) = {
                    let image = &self.snapshot.images[ix];
                    (image.id.clone(), image.label().to_owned())
                };
                self.select_image(&id, &label, cx);
            }
            Tab::Volumes => {
                let rows = self.visible_volumes();
                if rows.is_empty() {
                    return;
                }
                let current = self.view.selected_id.as_deref().and_then(|id| {
                    rows.iter()
                        .position(|&ix| self.snapshot.volumes[ix].name == id)
                });
                let name = self.snapshot.volumes[rows[step(current, rows.len())]]
                    .name
                    .clone();
                self.select_volume(&name, cx);
            }
            Tab::Networks => {
                let rows = self.visible_networks();
                if rows.is_empty() {
                    return;
                }
                let current = self.view.selected_id.as_deref().and_then(|id| {
                    rows.iter()
                        .position(|&ix| network_key(&self.snapshot.networks[ix]) == id)
                });
                let ix = rows[step(current, rows.len())];
                let (key, name) = {
                    let net = &self.snapshot.networks[ix];
                    (network_key(net).to_owned(), net.name.clone())
                };
                self.select_network(&key, &name, cx);
            }
            Tab::Events => {
                let rows = self.visible_events();
                if rows.is_empty() {
                    return;
                }
                let current = self
                    .view
                    .event_cursor
                    .and_then(|cur| rows.iter().position(|&ix| ix == cur));
                let ix = rows[step(current, rows.len())];
                self.view.event_cursor = Some(ix);
                self.reveal(SearchKind::Events, ix);
                self.redraw_page(cx);
            }
            _ => {}
        }
    }

    pub(crate) fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        // The sheet first, whichever it is.
        if self.view.sheet.is_some() {
            self.close_sheet(cx);
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
        let tokens = match appearance {
            Appearance::Light => berth_ui::theme::light(),
            Appearance::Dark => berth_ui::theme::dark(),
            Appearance::System => berth_ui::theme::for_appearance(window.appearance()),
        };
        self.set_tokens(tokens, cx);
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
        self.engine.preferred_socket = Some(runtime.socket.clone());
        self.switch_engine(&runtime, cx);
        self.save_settings(cx);
        cx.notify();
    }

    pub(crate) fn use_socket(&mut self, path: &str, cx: &mut Context<Self>) {
        let path = path.strip_prefix("unix://").unwrap_or(path).trim();
        if path.is_empty() {
            return;
        }
        self.engine.preferred_socket = Some(path.to_owned());
        self.engine.requested_socket = Some(path.to_owned());
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
        if self.engine.client.is_none() {
            self.start_discovery(cx);
        } else {
            self.start_refresh(cx);
        }
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
            self.snapshot_id,
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

    pub(crate) fn visible_images(&self) -> Vec<usize> {
        self.rows_of(SearchKind::Images, || {
            let query = self.query(SearchKind::Images).to_ascii_lowercase();
            let (col, asc) = self.sort_for(SearchKind::Images as usize);
            let mut rows: Vec<usize> = self
                .snapshot
                .images
                .iter()
                .enumerate()
                .filter(|(_, img)| {
                    query.is_empty()
                        || img.label().to_ascii_lowercase().contains(&query)
                        || img.id.to_ascii_lowercase().contains(&query)
                })
                .map(|(ix, _)| ix)
                .collect();
            rows.sort_by(|&a, &b| {
                let (a, b) = (&self.snapshot.images[a], &self.snapshot.images[b]);
                let ord = match col {
                    1 => a.id.cmp(&b.id),
                    2 => a.size.cmp(&b.size),
                    3 => a.created.cmp(&b.created),
                    4 => a.dangling().cmp(&b.dangling()),
                    _ => a.label().cmp(b.label()),
                };
                if asc { ord } else { ord.reverse() }
            });
            rows
        })
    }

    pub(crate) fn visible_volumes(&self) -> Vec<usize> {
        self.rows_of(SearchKind::Volumes, || {
            let query = self.query(SearchKind::Volumes).to_ascii_lowercase();
            let (col, asc) = self.sort_for(SearchKind::Volumes as usize);
            let mut rows: Vec<usize> = self
                .snapshot
                .volumes
                .iter()
                .enumerate()
                .filter(|(_, v)| {
                    query.is_empty()
                        || v.name.to_ascii_lowercase().contains(&query)
                        || v.driver.to_ascii_lowercase().contains(&query)
                        || v.mountpoint.to_ascii_lowercase().contains(&query)
                })
                .map(|(ix, _)| ix)
                .collect();
            rows.sort_by(|&a, &b| {
                let (a, b) = (&self.snapshot.volumes[a], &self.snapshot.volumes[b]);
                let (ua, ub) = (a.usage_data.as_ref(), b.usage_data.as_ref());
                let ord = match col {
                    1 => a.driver.cmp(&b.driver),
                    2 => ua.map_or(-1, |u| u.size).cmp(&ub.map_or(-1, |u| u.size)),
                    3 => ua
                        .map_or(-1, |u| u.ref_count)
                        .cmp(&ub.map_or(-1, |u| u.ref_count)),
                    4 => a.mountpoint.cmp(&b.mountpoint),
                    _ => a.name.cmp(&b.name),
                };
                if asc { ord } else { ord.reverse() }
            });
            rows
        })
    }

    pub(crate) fn visible_networks(&self) -> Vec<usize> {
        self.rows_of(SearchKind::Networks, || {
            let query = self.query(SearchKind::Networks).to_ascii_lowercase();
            let (col, asc) = self.sort_for(SearchKind::Networks as usize);
            let mut rows: Vec<usize> = self
                .snapshot
                .networks
                .iter()
                .enumerate()
                .filter(|(_, n)| {
                    query.is_empty()
                        || n.name.to_ascii_lowercase().contains(&query)
                        || n.driver.to_ascii_lowercase().contains(&query)
                        || n.scope.to_ascii_lowercase().contains(&query)
                })
                .map(|(ix, _)| ix)
                .collect();
            rows.sort_by(|&a, &b| {
                let (a, b) = (&self.snapshot.networks[a], &self.snapshot.networks[b]);
                let ord = match col {
                    1 => a.driver.cmp(&b.driver),
                    2 => a.scope.cmp(&b.scope),
                    3 => self
                        .snapshot
                        .network_counts
                        .get(&a.name)
                        .cmp(&self.snapshot.network_counts.get(&b.name)),
                    _ => a.name.cmp(&b.name),
                };
                if asc { ord } else { ord.reverse() }
            });
            rows
        })
    }

    pub(crate) fn visible_events(&self) -> Vec<usize> {
        self.rows_of(SearchKind::Events, || {
            let query = self.query(SearchKind::Events).to_ascii_lowercase();
            let mut rows: Vec<usize> = self
                .snapshot
                .events
                .iter()
                .enumerate()
                .filter(|(_, ev)| ev.matches(&query))
                .map(|(ix, _)| ix)
                .collect();
            rows.sort_by_key(|&ix| std::cmp::Reverse(self.snapshot.events[ix].time_nano));
            rows
        })
    }
}
