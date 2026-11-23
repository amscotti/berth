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

    /// Find an engine and start reading it.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.start_discovery(cx);
    }

    pub(crate) fn start_discovery(&mut self, cx: &mut Context<Self>) {
        if self.discovery_task.is_some() {
            // A scan is running. This one goes next instead of being dropped.
            self.rescan_wanted = true;
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
        } else {
            self.adopt(runtimes, cx);
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
    fn adopt(&mut self, runtimes: Vec<berth_engine::discovery::Runtime>, cx: &mut Context<Self>) {
        use berth_engine::discovery::identity;
        let connected = self.engine.client.is_some();
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
            self.engine.install(&runtime);
            self.start_refresh(cx);
        } else {
            self.engine.client = None;
            self.engine.error = "no engine answering".to_owned();
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

        if snapshot.events_error {
            snapshot.events = self.snapshot.events.clone();
        } else if !snapshot.events.is_empty() {
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

#[cfg(test)]
mod tests {
    use berth_core::history::Sample;
    use berth_engine::df::Df;
    use gpui::TestAppContext;

    /// The fixture engine, on a socket that exists nowhere: the tasks a scan
    /// or a refresh spawns must never reach this machine's real engine.
    fn runtime() -> berth_engine::discovery::Runtime {
        let mut runtime = crate::fixture::fixture_runtime();
        runtime.socket = "/nowhere/berth-test.sock".into();
        runtime
    }

    /// The machine's own figures aren't the engine's. A new sample lands
    /// beside the snapshot and leaves the snapshot on screen exactly as it is.
    #[gpui::test]
    fn a_host_sample_does_not_rebuild_the_snapshot(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        let on_screen = app.read_with(&*cx, |app, _| app.snapshot.clone());
        app.update(cx, |app, cx| app.sample_host_async(cx));
        cx.run_until_parked();
        app.read_with(&*cx, |app, _| {
            assert!(app.host_task.is_none(), "the sample finished");
            assert!(
                std::sync::Arc::ptr_eq(&app.snapshot, &on_screen),
                "the snapshot was not rebuilt for the host's figures"
            );
            // The fixture's 64 GiB gave way to what this machine really has.
            assert!(app.host.mem_total > 0, "the sample landed");
        });
    }

    #[gpui::test]
    fn finding_the_same_engine_again_keeps_what_was_learned_from_it(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            let runtime = runtime();
            app.adopt_runtimes(vec![runtime.clone()], app.engine.generation, cx);
            let generation = app.engine.generation;
            app.engine.history.push(Sample::default());
            app.engine.df = Some(Df::default());
            app.engine.events_since = 42;

            // The same daemon, found again by a rescan, now with a silent
            // neighbour: nothing starts over.
            let silent = berth_engine::discovery::Runtime {
                socket: "/nowhere/other.sock".into(),
                ..Default::default()
            };
            app.adopt_runtimes(vec![silent, runtime], generation, cx);
            assert_eq!(app.engine.generation, generation);
            assert_eq!(app.engine.history.len(), 1);
            assert!(app.engine.df.is_some());
            assert_eq!(app.engine.events_since, 42);
            assert_eq!(
                app.engine.active_index(),
                Some(1),
                "the list may reorder; the engine stays"
            );
        });
    }

    /// The engine being read is known by identity. One that has left the list
    /// is no longer the engine, whatever has taken its place in the list.
    #[gpui::test]
    fn an_engine_that_left_the_list_is_no_longer_the_active_one(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            let runtime = runtime();
            app.adopt_runtimes(vec![runtime.clone()], app.engine.generation, cx);
            assert_eq!(
                app.engine.active_runtime().map(|it| &it.socket),
                Some(&runtime.socket)
            );

            // A scan that finds nothing answering, and not the engine either.
            let stranger = berth_engine::discovery::Runtime {
                socket: "/nowhere/stranger.sock".into(),
                ..Default::default()
            };
            app.adopt_runtimes(vec![stranger], app.engine.generation, cx);
            assert!(app.engine.client.is_none(), "nothing is being read");
            assert_eq!(
                app.engine.active_index(),
                None,
                "the stranger that took its place is not the engine"
            );
            assert!(app.engine.active_runtime().is_none());
        });
    }

    #[gpui::test]
    fn a_different_engine_starts_over_and_a_returning_one_reconnects(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            let runtime = runtime();
            app.adopt_runtimes(vec![runtime.clone()], app.engine.generation, cx);
            app.engine.history.push(Sample::default());

            // Another daemon behind the same socket is a new engine.
            let mut other = runtime.clone();
            other.info.id = "daemon-2".into();
            app.adopt_runtimes(vec![other], app.engine.generation, cx);
            assert!(app.engine.history.is_empty());

            // The same daemon, after it had gone away and the client was
            // dropped: what was learned before it went is not trusted.
            app.engine.history.push(Sample::default());
            app.engine.client = None;
            app.adopt_runtimes(vec![runtime], app.engine.generation, cx);
            assert!(app.engine.client.is_some(), "it reconnects");
            assert!(app.engine.history.is_empty(), "and starts over");
        });
    }

    #[gpui::test]
    fn a_rescan_asked_for_during_a_scan_runs_after_it(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.start_discovery(cx);
            assert!(app.discovery_task.is_some());
            app.start_discovery(cx);
            app.rescan(cx);
            assert!(
                app.rescan_wanted,
                "the later requests are kept, not dropped"
            );
            // The engine being read is not put on hold for the scan.
            assert_eq!(
                app.snapshot.containers.len(),
                2,
                "the snapshot on screen is untouched"
            );
        });
    }
}
