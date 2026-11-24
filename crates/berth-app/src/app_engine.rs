//! Discovery and refresh orchestration.

use crate::app::*;
use crate::jobs::refresh;
use gpui::Context;

impl BerthApp {
    // --- engine ------------------------------------------------------------

    /// Begin discovery. The running app calls this once the window exists;
    /// tests and headless renders do not.
    pub(crate) fn start_tray(&mut self, cx: &mut Context<Self>) {
        let Some(tray) = crate::tray::create() else {
            return;
        };
        self.tray = Some(tray);
        self.tray_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let events = this
                    .update(cx, |app, _| {
                        app.tray
                            .as_ref()
                            .map(crate::tray::drain)
                            .unwrap_or_default()
                    })
                    .ok()
                    .unwrap_or_default();
                for event in events {
                    this.update(cx, |app, cx| app.handle_tray(event, cx)).ok();
                }
            }
        }));
        self.update_tray_title();
    }

    fn handle_tray(&mut self, event: crate::tray::Event, cx: &mut Context<Self>) {
        match event {
            crate::tray::Event::Open => {
                if let Some(handle) = self.window_handle {
                    let _ = handle.update(cx, |_, window, cx| {
                        window.activate_window();
                        cx.activate(true);
                    });
                } else {
                    cx.activate(true);
                }
            }
            crate::tray::Event::Quit => cx.quit(),
        }
    }

    pub(crate) fn update_tray_title(&self) {
        let Some(tray) = &self.tray else {
            return;
        };
        let title = self.tray_title();
        crate::tray::set_title(tray, &title);
    }

    pub(crate) fn tray_title(&self) -> String {
        if self.engine.client.is_none()
            || !self.engine.error.is_empty()
            || self.engine.last_update == 0
        {
            return "— up · — · —".to_owned();
        }
        let mem = if self.snapshot.stats_complete() {
            berth_core::format::bytes(self.snapshot.container_memory())
        } else {
            "—".to_owned()
        };
        let cpu = self
            .engine
            .history
            .latest_cpu()
            .map(|value| berth_core::format::percent(value as f64))
            .unwrap_or_else(|| "—".to_owned());
        format!("{} up · {cpu} · {mem}", self.snapshot.running())
    }

    /// Find an engine and start reading it.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.start_discovery(cx);
    }

    pub(crate) fn start_discovery(&mut self, cx: &mut Context<Self>) {
        self.start_discovery_inner(true, cx);
    }

    fn start_discovery_inner(&mut self, notify_loading: bool, cx: &mut Context<Self>) {
        if self.discovery_task.is_some() {
            // A scan is running. This one goes next instead of being dropped.
            self.rescan_wanted = true;
            return;
        }
        self.engine.loading = true;
        // Never wipe an existing offline error on background retries: that
        // makes the Overview band flicker every two seconds.
        if notify_loading && self.engine.client.is_some() {
            self.engine.error.clear();
            cx.notify();
        } else if notify_loading && self.engine.error.is_empty() {
            cx.notify();
        }
        let generation = self.engine.generation;
        let requested = self.engine.requested_socket.take();
        let env_socket = std::env::var("BERTH_SOCKET")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let preferred = requested
            .clone()
            .or(env_socket)
            .or_else(|| self.engine.preferred_socket.clone())
            .unwrap_or_default();
        self.discovery_task = Some(cx.spawn(async move |this, cx| {
            let runtimes = cx
                .background_executor()
                .spawn(async move { berth_engine::discovery::scan(&preferred) })
                .await;
            this.update(cx, |app, cx| {
                app.adopt_runtimes_for(runtimes, generation, requested, cx)
            })
            .ok();
        }));
    }

    /// Install a different (or reconnecting) engine and drop everything
    /// carried over from the previous one: in-flight refresh, selection,
    /// bulk picks, events, disk figures and snapshot rows.
    pub(crate) fn switch_engine(
        &mut self,
        runtime: &berth_engine::discovery::Runtime,
        cx: &mut Context<Self>,
    ) {
        self.engine.install(runtime);
        self.refresh_task = None;
        self.refresh_wanted = false;
        self.timer_task = None;
        self.view.bulk_selected.clear();
        self.clear_selection(cx);
        self.set_snapshot(crate::state::Snapshot::empty());
        self.update_tray_title();
        self.start_refresh(cx);
    }

    #[cfg(test)]
    pub(crate) fn adopt_runtimes(
        &mut self,
        runtimes: Vec<berth_engine::discovery::Runtime>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        self.adopt_runtimes_for(runtimes, generation, None, cx);
    }

    pub(crate) fn adopt_runtimes_for(
        &mut self,
        runtimes: Vec<berth_engine::discovery::Runtime>,
        generation: u64,
        requested: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.discovery_task = None;
        self.engine.loading = false;
        if generation != self.engine.generation
            && self.engine.client.is_some()
            && requested.is_none()
        {
            // A newer engine was chosen while this scan was in flight. Do not
            // let the old scan overwrite the runtime list the new choice uses.
        } else {
            self.adopt(runtimes, requested.as_deref(), cx);
        }
        // A rescan asked for while this one ran goes next.
        if std::mem::take(&mut self.rescan_wanted) {
            self.start_discovery(cx);
        }
        cx.notify();
    }

    /// Take a scan's answer. Finding the same engine again changes nothing
    /// but the list: the figures already learned from it stay. A different
    /// engine, or the same one after it had gone away, starts over.
    fn adopt(
        &mut self,
        runtimes: Vec<berth_engine::discovery::Runtime>,
        requested: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        use berth_engine::discovery::{identity, same_socket};
        let connected = self.engine.client.is_some();
        if let Some(req) = requested {
            if let Some(runtime) = runtimes
                .iter()
                .find(|runtime| runtime.reachable && same_socket(&runtime.socket, req))
                .cloned()
            {
                let same_active = connected
                    && self.engine.identity.as_deref() == Some(identity(&runtime).as_str());
                self.engine.runtimes = runtimes;
                if !same_active {
                    self.switch_engine(&runtime, cx);
                }
                return;
            }
            self.notify(&format!("Could not connect to {req}"), true, cx);
        }
        // The engine being read is known by identity, and looked up in the
        // list when it is asked for, so a list that has been reordered, or
        // has lost the engine, needs no index put right.
        let still_listed = self
            .engine
            .identity
            .as_ref()
            .filter(|_| connected)
            .is_some_and(|current| {
                runtimes
                    .iter()
                    .any(|runtime| runtime.reachable && current == &identity(runtime))
            });
        self.engine.runtimes = runtimes;
        if still_listed {
            return;
        }
        if let Some(runtime) = self
            .engine
            .runtimes
            .iter()
            .find(|runtime| runtime.reachable)
            .cloned()
        {
            self.switch_engine(&runtime, cx);
        } else {
            self.engine.client = None;
            self.engine.error = "no engine answering".to_owned();
            self.update_tray_title();
            self.sample_host_async(cx);
            self.schedule_discovery(cx);
        }
    }

    fn sample_host_async(&mut self, cx: &mut Context<Self>) {
        self.host_task = Some(cx.spawn(async move |this, cx| {
            let host = cx
                .background_executor()
                .spawn(async { crate::host::sample() })
                .await;
            this.update(cx, |app, cx| {
                app.host_task = None;
                // Beside the snapshot, not in it: nothing about the engine
                // changed, so the snapshot stays the one on screen.
                app.host = host;
                cx.notify();
            })
            .ok();
        }));
    }

    fn schedule_discovery(&mut self, cx: &mut Context<Self>) {
        self.timer_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            this.update(cx, |app, cx| app.start_discovery_inner(false, cx))
                .ok();
        }));
    }

    pub(crate) fn start_refresh(&mut self, cx: &mut Context<Self>) {
        if self.engine.client.is_none() {
            return;
        }
        if self.refresh_task.is_some() {
            self.refresh_wanted = true;
            return;
        }
        let Some(client) = self.engine.client.clone() else {
            return;
        };
        let cores = self
            .engine
            .active_runtime()
            .map_or(1, |runtime| runtime.info.ncpu.max(1));
        let request = refresh::Request {
            socket: client.socket().to_owned(),
            api: client.api().to_owned(),
            dialect: client.dialect(),
            generation: self.engine.generation,
            events_since: self.engine.events_since,
            want_df: self.engine.df_due(),
            cores,
        };
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { refresh::load(request) })
                .await;
            this.update(cx, |app, cx| app.finish_refresh(outcome, cx))
                .ok();
        }));
    }

    pub(crate) fn finish_refresh(
        &mut self,
        outcome: Result<refresh::Outcome, refresh::RefreshError>,
        cx: &mut Context<Self>,
    ) {
        self.refresh_task = None;
        match outcome {
            Ok(outcome) if outcome.generation == self.engine.generation => {
                self.commit(outcome, cx);
                self.rearm_refresh(cx);
            }
            Ok(_) => {
                // A completion from an engine that was replaced while the
                // pass was in flight is discarded, not painted.
                self.rearm_refresh(cx);
            }
            Err(error) if error.generation == self.engine.generation => {
                self.engine.error = error.message;
                self.sample_host_async(cx);
                if error.engine_dead {
                    self.engine.client = None;
                    self.start_discovery(cx);
                } else {
                    self.rearm_refresh(cx);
                }
                self.update_tray_title();
                cx.notify();
            }
            Err(_) => {
                self.rearm_refresh(cx);
            }
        }
    }

    fn rearm_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh_wanted {
            self.refresh_wanted = false;
            self.start_refresh(cx);
        } else {
            self.schedule_refresh(cx);
        }
    }

    fn commit(&mut self, outcome: refresh::Outcome, cx: &mut Context<Self>) {
        let refresh::Outcome {
            snapshot,
            host,
            df,
            generation: _,
        } = outcome;
        self.host = host;
        let mut snapshot = snapshot;
        let previous_events = self.snapshot.events.clone();
        let first_pass = self.engine.last_update == 0;
        match df {
            Some(Ok(df)) => {
                snapshot.df = Some(df.clone());
                snapshot.df_at = berth_core::format::now();
                self.engine.df = Some(df);
                self.engine.df_at = berth_core::format::now();
                self.engine.df_error.clear();
            }
            Some(Err(error)) => {
                snapshot.df = self.engine.df.clone();
                snapshot.df_at = self.engine.df_at;
                snapshot.df_error = error.clone();
                self.engine.df_error = error;
            }
            None => {
                snapshot.df = self.engine.df.clone();
                snapshot.df_at = self.engine.df_at;
                snapshot.df_error = self.engine.df_error.clone();
            }
        }

        if let Some(df) = &snapshot.df {
            merge_volume_usage(&mut snapshot.volumes, df);
        }

        if snapshot.events_error || snapshot.events.is_empty() {
            snapshot.events = self.snapshot.events.clone();
        } else {
            snapshot.events = refresh::merge_events(&self.snapshot.events, &snapshot.events);
        }
        if !snapshot.events_error {
            // A time on the engine's own clock, not the host's: a VM's clock
            // drifts, and events would be skipped.
            self.engine.events_since =
                berth_engine::events::cursor(&snapshot.events, self.engine.events_since);
        }

        let cores = self
            .engine
            .active_runtime()
            .map(|runtime| runtime.info.ncpu.max(1) as f32)
            .unwrap_or(1.0);
        let cpu = snapshot.container_cpu(cores);
        self.engine.history.push(berth_core::history::Sample {
            cpu: cpu.unwrap_or(0.0),
            cpu_known: cpu.is_some(),
            mem: snapshot.container_memory() as f32,
            mem_known: snapshot.stats_complete(),
            running: snapshot.running() as f32,
            disk: snapshot.store_bytes().unwrap_or(0) as f32,
            host_cpu: self.host.cpu_percent,
            host_mem: self.host.mem_used as f32,
        });

        self.engine.error = snapshot.error.clone();
        self.engine.last_update = snapshot.last_update;
        let fresh_events = snapshot.events.clone();
        self.set_snapshot(snapshot);
        if !first_pass && berth_core::format::now() >= self.quiet_notices_until {
            for event in fresh_events {
                if berth_engine::events::has(&previous_events, &event) {
                    continue;
                }
                if let Some(notice) = event.notice() {
                    let name = event.name().to_owned();
                    self.notify(&format!("{name} {notice}"), true, cx);
                }
            }
        }
        self.reconcile_selection();
        if self.view.detail_kind == DetailKind::Container
            && self.view.selected_id.is_some()
            && self.inspector.logs_follow
            && self.inspector.detail_task.is_none()
        {
            self.start_detail(cx);
        }
        self.update_tray_title();
        cx.notify();
    }

    pub(crate) fn reconcile_selection(&mut self) {
        self.view.bulk_selected.retain(|id| {
            self.snapshot
                .containers
                .iter()
                .any(|container| &container.id == id)
        });
        if self
            .view
            .event_cursor
            .is_some_and(|ix| ix >= self.snapshot.events.len())
        {
            self.view.event_cursor = None;
        }
        // Selection is by id, found again in whatever snapshot is on screen.
        let Some(id) = self.view.selected_id.clone() else {
            return;
        };
        let still_there = match self.view.detail_kind {
            DetailKind::Container => self.snapshot.selected_container(&id).is_some(),
            DetailKind::Image => self.snapshot.selected_image(&id).is_some(),
            DetailKind::Volume => self.snapshot.selected_volume(&id).is_some(),
            DetailKind::Network => self.snapshot.selected_network(&id).is_some(),
        };
        if !still_there {
            self.view.selected_id = None;
        }
    }

    fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        self.timer_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(5))
                .await;
            this.update(cx, |app, cx| app.start_refresh(cx)).ok();
        }));
    }
}

/// Fill per-volume `UsageData` from `/system/df`. `GET /volumes` omits usage
/// figures; `/system/df` calculates them, so merging by name gives Volumes
/// `SIZE`/`IN USE` and lets Prune count unused volumes.
fn merge_volume_usage(volumes: &mut [berth_engine::dto::Volume], df: &berth_engine::df::Df) {
    let Some(df_volumes) = &df.volumes else {
        return;
    };
    let by_name: std::collections::HashMap<&str, &berth_engine::dto::UsageData> = df_volumes
        .iter()
        .filter_map(|v| v.usage_data.as_ref().map(|usage| (v.name.as_str(), usage)))
        .collect();
    if by_name.is_empty() {
        return;
    }
    for volume in volumes {
        if let Some(usage) = by_name.get(volume.name.as_str()) {
            volume.usage_data = Some((*usage).clone());
        }
    }
}

#[cfg(test)]
#[path = "app_engine_tests.rs"]
mod tests;
