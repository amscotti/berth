//! Discovery and refresh orchestration.

use crate::app::*;
use crate::jobs::refresh;
use gpui::Context;
use std::sync::Arc;

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
        let title = format!("{} up · {cpu} · {mem}", self.snapshot.running());
        crate::tray::set_title(tray, &title);
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.fixture {
            return;
        }
        self.start_discovery(cx);
    }

    pub(crate) fn start_discovery(&mut self, cx: &mut Context<Self>) {
        if self.discovery_task.is_some() {
            return;
        }
        self.engine.loading = true;
        self.engine.error.clear();
        cx.notify();
        let generation = self.engine.generation;
        let preferred = std::env::var("BERTH_SOCKET")
            .ok()
            .or_else(|| self.engine.preferred_socket.clone())
            .unwrap_or_default();
        self.discovery_task = Some(cx.spawn(async move |this, cx| {
            let runtimes = cx
                .background_executor()
                .spawn(async move { berth_engine::discovery::scan(&preferred) })
                .await;
            this.update(cx, |app, cx| app.adopt_runtimes(runtimes, generation, cx))
                .ok();
        }));
    }

    fn adopt_runtimes(
        &mut self,
        runtimes: Vec<berth_engine::discovery::Runtime>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        self.discovery_task = None;
        self.engine.loading = false;
        if generation != self.engine.generation && self.engine.client.is_some() {
            // A newer engine was chosen while this scan was in flight. Do not
            // let the old scan overwrite the runtime list the new choice uses.
            cx.notify();
            return;
        }
        self.engine.runtimes = runtimes;
        if let Some(index) = self
            .engine
            .runtimes
            .iter()
            .position(|runtime| runtime.reachable)
        {
            self.engine.active = index;
            let runtime = self.engine.runtimes[index].clone();
            self.engine.install(&runtime);
            self.start_refresh(cx);
        } else {
            self.engine.client = None;
            self.engine.error = "no engine answering".to_owned();
            self.sample_host_async(cx);
            self.schedule_discovery(cx);
        }
        cx.notify();
    }

    fn sample_host_async(&mut self, cx: &mut Context<Self>) {
        self.host_task = Some(cx.spawn(async move |this, cx| {
            let host = cx
                .background_executor()
                .spawn(async { crate::host::sample() })
                .await;
            this.update(cx, |app, cx| {
                app.host_task = None;
                let mut snapshot = (*app.snapshot).clone();
                snapshot.host = host;
                app.snapshot = Arc::new(snapshot);
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
            this.update(cx, |app, cx| app.start_discovery(cx)).ok();
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

    fn finish_refresh(
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
        let mut snapshot = outcome.snapshot;
        let previous_events = self.snapshot.events.clone();
        let first_pass = self.engine.last_update == 0;
        match outcome.df {
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

        if snapshot.events_error {
            snapshot.events = self.snapshot.events.clone();
        } else if !snapshot.events.is_empty() {
            snapshot.events = refresh::merge_events(&self.snapshot.events, &snapshot.events);
        }
        if !snapshot.events_error {
            self.engine.events_since = snapshot.last_update;
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
            host_cpu: snapshot.host.cpu_percent,
            host_mem: snapshot.host.mem_used as f32,
        });

        self.engine.error = snapshot.error.clone();
        self.engine.last_update = snapshot.last_update;
        let fresh_events = snapshot.events.clone();
        self.snapshot = Arc::new(snapshot);
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
        if self.detail_kind == DetailKind::Container
            && self.selected_id.is_some()
            && self.logs_follow
            && self.detail_task.is_none()
        {
            self.start_detail(cx);
        }
        self.update_tray_title();
        cx.notify();
    }

    fn reconcile_selection(&mut self) {
        self.bulk_selected.retain(|id| {
            self.snapshot
                .containers
                .iter()
                .any(|container| &container.id == id)
        });
        let Some(id) = self.selected_id.clone() else {
            self.selected = None;
            return;
        };
        match self.detail_kind {
            DetailKind::Container => {
                self.selected = self
                    .snapshot
                    .containers
                    .iter()
                    .position(|container| container.id == id);
                if self.selected.is_none() {
                    self.selected_id = None;
                }
            }
            DetailKind::Image => {
                if self.snapshot.selected_image(&id).is_none() {
                    self.selected_id = None;
                }
            }
            DetailKind::Volume => {
                if self.snapshot.selected_volume(&id).is_none() {
                    self.selected_id = None;
                }
            }
            DetailKind::Network => {
                if self.snapshot.selected_network(&id).is_none() {
                    self.selected_id = None;
                }
            }
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
