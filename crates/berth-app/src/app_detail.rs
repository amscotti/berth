//! Inspector detail jobs.

use crate::app::*;
use crate::jobs::detail;
use gpui::Context;

impl BerthApp {
    // --- detail ------------------------------------------------------------

    pub(crate) fn start_detail(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.view.selected_id.clone() else {
            return;
        };
        let Some(client) = self.engine.client.clone() else {
            return;
        };
        self.inspector.detail_generation = self.inspector.detail_generation.wrapping_add(1);
        let generation = self.inspector.detail_generation;
        let request = detail::Request {
            socket: client.socket().to_owned(),
            api: client.api().to_owned(),
            id: id.clone(),
            generation,
            timestamps: self.inspector.logs_timestamps,
            extras: true,
        };
        self.inspector.logs_loading = true;
        self.inspector.inspect_loading = true;
        self.inspector.detail_task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { detail::load(request) })
                .await;
            this.update(cx, |app, cx| app.finish_detail(outcome, cx))
                .ok();
        }));
        self.redraw_inspector(cx);
    }

    /// Apply one detail read. A read from an older generation is simply
    /// dropped: a newer one owns the task slot. A read of the current
    /// generation releases the slot even when the selection has gone away
    /// under it, so nothing is left behind to hold the next read back.
    fn finish_detail(&mut self, outcome: detail::Outcome, cx: &mut Context<Self>) {
        if outcome.generation != self.inspector.detail_generation {
            return;
        }
        self.inspector.detail_task = None;
        let Some(selected) = self.view.selected_id.as_deref() else {
            return;
        };
        if selected != outcome.id {
            return;
        }
        self.inspector.logs_loading = false;
        self.inspector.inspect_loading = false;
        self.inspector.detail_top = outcome.top;
        self.inspector.detail_changes = outcome.changes;
        self.inspector.logs_text = outcome
            .logs
            .unwrap_or_else(|error| format!("could not read logs: {error}"));
        // Follow means the newest lines are on screen.
        if self.inspector.logs_follow {
            self.inspector.log_scroll.scroll_to_bottom();
        }
        self.inspector.inspect_text = match outcome.inspect {
            Ok(inspect) => {
                self.inspector.inspect_facts = Some(
                    berth_engine::detail::InspectFacts::from_detail(&inspect.detail),
                );
                Some(inspect.json)
            }
            Err(error) => Some(format!("could not read inspect: {error}")),
        };
        self.redraw_inspector(cx);
    }

    fn finish_image_detail(&mut self, outcome: detail::ImageOutcome, cx: &mut Context<Self>) {
        if outcome.generation != self.inspector.detail_generation {
            return;
        }
        self.inspector.detail_task = None;
        if self.view.selected_id.as_deref() != Some(outcome.id.as_str()) {
            return;
        }
        self.inspector.image_layers_loading = false;
        if let Ok(layers) = outcome.layers {
            self.inspector.image_layers = layers;
        }
        self.redraw_inspector(cx);
    }

    pub(crate) fn open_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.view.selected_id.clone() else {
            return;
        };
        let Some(container) = self.snapshot.selected_container(&id).cloned() else {
            return;
        };
        let Some(runtime) = self.engine.active_runtime().cloned() else {
            self.notify("no engine is selected", true, cx);
            return;
        };
        let socket = self
            .engine
            .socket
            .clone()
            .unwrap_or_else(|| runtime.socket.clone());
        let name = container.name().to_owned();
        self.terminal_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::terminal::open(runtime.kind, &socket, &name) })
                .await;
            this.update(cx, |app, cx| {
                app.terminal_task = None;
                if let Err(error) = result {
                    app.notify(&error, true, cx);
                }
            })
            .ok();
        }));
    }

    pub(crate) fn open_port(&mut self, host: &str, port: u16, cx: &mut Context<Self>) {
        let host = if host.is_empty() || host == "0.0.0.0" || host == "::" {
            "localhost"
        } else {
            host
        };
        let scheme = if port == 443 || port == 8443 {
            "https"
        } else {
            "http"
        };
        let url = if host.contains(':') {
            format!("{scheme}://[{host}]:{port}")
        } else {
            format!("{scheme}://{host}:{port}")
        };
        cx.open_url(&url);
    }

    pub(crate) fn reload_detail(&mut self, cx: &mut Context<Self>) {
        self.start_detail(cx);
    }

    /// Show one page of the inspector, reading what that page shows if it
    /// hasn't been read yet.
    pub(crate) fn show_inspector_tab(&mut self, tab: InspectorTab, cx: &mut Context<Self>) {
        self.inspector.tab = tab;
        let unread = match tab {
            InspectorTab::Facts => false,
            InspectorTab::Config => self.inspector.inspect_facts.is_none(),
            InspectorTab::Runtime => {
                self.inspector.detail_top.is_none() && self.inspector.detail_changes.is_empty()
            }
            InspectorTab::Inspect => self.inspector.inspect_text.is_none(),
        };
        if unread {
            self.start_detail(cx);
        }
        self.redraw_inspector(cx);
    }

    pub(crate) fn toggle_log_timestamps(&mut self, cx: &mut Context<Self>) {
        self.inspector.logs_timestamps = !self.inspector.logs_timestamps;
        self.start_detail(cx);
        self.redraw_inspector(cx);
    }

    pub(crate) fn copy_logs(&mut self, cx: &mut Context<Self>) {
        if !self.inspector.logs_text.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                self.inspector.logs_text.clone(),
            ));
            self.notify("Logs copied", false, cx);
        }
    }

    pub(crate) fn copy_run_command(&mut self, cx: &mut Context<Self>) {
        let Some(facts) = &self.inspector.inspect_facts else {
            return;
        };
        let Some(command) = &facts.run_cmd else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(command.clone()));
        if facts.run_redacted {
            self.notify("Run command copied · some env values redacted", false, cx);
        } else {
            self.notify("Run command copied", false, cx);
        }
    }

    pub(crate) fn copy_inspect(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = &self.inspector.inspect_text {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
            self.notify("Inspect JSON copied", false, cx);
        }
    }

    pub(crate) fn select_container_by_id(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(ix) = self.snapshot.containers.iter().position(|c| c.id == id) {
            self.view.tab = Tab::Containers;
            // The row has to be on screen: drop whatever was hiding it.
            self.set_filter(Filter::All, cx);
            self.clear_search(SearchKind::Containers, cx);
            self.select(ix, cx);
        }
    }

    pub(crate) fn clone_container_to_run(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.view.selected_id.clone() else {
            return;
        };
        let Some(container) = self.snapshot.selected_container(&id).cloned() else {
            return;
        };
        let ports = container
            .published_ports()
            .iter()
            .map(|p| {
                if p.ip.is_empty() {
                    format!("{}:{}", p.port, p.private_port)
                } else {
                    format!("{}:{}:{}", p.ip, p.port, p.private_port)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let (volumes, env, missing) = if let Some(facts) = &self.inspector.inspect_facts {
            let vols = facts
                .mounts
                .iter()
                .filter(|m| !m.source.is_empty() && !m.destination.is_empty())
                .map(|m| {
                    if m.rw {
                        format!("{}:{}", m.source, m.destination)
                    } else {
                        format!("{}:{}:ro", m.source, m.destination)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            // A redacted value is carried as an empty one: the copy has every
            // variable its source had, and the form says which need filling in.
            let mut missing = 0;
            let envs = facts
                .env
                .iter()
                .map(|(key, value, secret)| {
                    if *secret {
                        missing += 1;
                        format!("{key}=")
                    } else {
                        format!("{key}={value}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            (vols, envs, missing)
        } else {
            (String::new(), String::new(), 0)
        };
        let image = container.image.clone();
        let next_name = format!("{}-copy", container.name());
        self.view.tab = Tab::Images;
        self.clear_selection(cx);
        self.view.creation_open = true;
        self.view
            .run_image
            .update(cx, |input, cx| input.set_value(image, cx));
        self.view
            .run_name
            .update(cx, |input, cx| input.set_value(next_name, cx));
        self.view
            .run_ports
            .update(cx, |input, cx| input.set_value(ports, cx));
        self.view
            .run_volumes
            .update(cx, |input, cx| input.set_value(volumes, cx));
        self.view
            .run_env
            .update(cx, |input, cx| input.set_value(env, cx));
        self.view.run_env_missing = missing;
        cx.notify();
    }

    pub(crate) fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.view.selected_id = self.snapshot.containers.get(ix).map(|row| row.id.clone());
        self.view.selected_name = self
            .snapshot
            .containers
            .get(ix)
            .map(|row| row.name().to_owned())
            .unwrap_or_default();
        self.view.detail_kind = DetailKind::Container;
        self.reveal(SearchKind::Containers, ix);
        self.start_detail(cx);
        cx.notify();
    }

    pub(crate) fn select_image(&mut self, id: &str, name: &str, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.view.selected_id = Some(id.to_owned());
        self.view.selected_name = name.to_owned();
        self.view.detail_kind = DetailKind::Image;
        if let Some(client) = self.engine.client.clone() {
            self.inspector.detail_generation = self.inspector.detail_generation.wrapping_add(1);
            let generation = self.inspector.detail_generation;
            let socket = client.socket().to_owned();
            let api = client.api().to_owned();
            let id_owned = id.to_owned();
            self.inspector.image_layers_loading = true;
            self.inspector.detail_task = Some(cx.spawn(async move |this, cx| {
                let outcome = cx
                    .background_executor()
                    .spawn(async move { detail::load_image(socket, api, id_owned, generation) })
                    .await;
                this.update(cx, |app, cx| app.finish_image_detail(outcome, cx))
                    .ok();
            }));
        }
        if let Some(ix) = self.snapshot.images.iter().position(|image| image.id == id) {
            self.reveal(SearchKind::Images, ix);
        }
        cx.notify();
    }

    pub(crate) fn select_volume(&mut self, name: &str, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.view.selected_id = Some(name.to_owned());
        self.view.selected_name = name.to_owned();
        self.view.detail_kind = DetailKind::Volume;
        if let Some(ix) = self
            .snapshot
            .volumes
            .iter()
            .position(|volume| volume.name == name)
        {
            self.reveal(SearchKind::Volumes, ix);
        }
        cx.notify();
    }

    pub(crate) fn select_network(&mut self, id: &str, name: &str, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.view.selected_id = Some(id.to_owned());
        self.view.selected_name = name.to_owned();
        self.view.detail_kind = DetailKind::Network;
        // The inspector knows it by its id, or by its name where the engine gave none.
        let ix = self.snapshot.networks.iter().position(|network| {
            if network.id.is_empty() {
                network.name == id
            } else {
                network.id == id
            }
        });
        if let Some(ix) = ix {
            self.reveal(SearchKind::Networks, ix);
        }
        cx.notify();
    }

    pub(crate) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.view.selected_id = None;
        self.view.selected_name.clear();
        self.inspector.tab = InspectorTab::Facts;
        self.inspector.logs_text.clear();
        self.inspector.logs_loading = false;
        self.inspector.inspect_text = None;
        self.inspector.inspect_loading = false;
        self.inspector.inspect_facts = None;
        self.inspector.detail_top = None;
        self.inspector.detail_changes.clear();
        self.inspector.image_layers.clear();
        self.inspector.image_layers_loading = false;
        self.inspector.detail_task = None;
        self.inspector.detail_generation = self.inspector.detail_generation.wrapping_add(1);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Task, TestAppContext};
    use std::sync::Arc;

    fn outcome(id: &str, generation: u64) -> detail::Outcome {
        detail::Outcome {
            id: id.into(),
            generation,
            logs: Ok("hello\n".into()),
            inspect: Err("not read in this test".into()),
            top: None,
            changes: Vec::new(),
        }
    }

    /// A queued read that stands in for the real one.
    fn queued(cx: &mut Context<BerthApp>) -> Task<()> {
        cx.spawn(async move |_, _| ())
    }

    #[gpui::test]
    fn a_read_that_outlives_its_selection_releases_the_task_slot(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.select(0, cx);
            app.inspector.detail_task = Some(queued(cx));
            let generation = app.inspector.detail_generation;

            // The container goes away under the read, as a delete makes it.
            app.snapshot = Arc::new(crate::state::Snapshot::empty());
            app.reconcile_selection();
            assert!(app.view.selected_id.is_none());

            app.finish_detail(outcome("demo-1", generation), cx);
            assert!(
                app.inspector.detail_task.is_none(),
                "a finished read must not stay in the slot and hold the next one back"
            );
            assert!(
                app.inspector.logs_text.is_empty(),
                "and it put nothing on screen"
            );
        });
    }

    #[gpui::test]
    fn a_read_from_an_older_generation_leaves_the_newer_task_alone(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.select(0, cx);
            app.inspector.detail_task = Some(queued(cx));
            app.finish_detail(outcome("demo-1", app.inspector.detail_generation - 1), cx);
            assert!(
                app.inspector.detail_task.is_some(),
                "the older read doesn't own the slot"
            );
            assert!(app.inspector.logs_text.is_empty());
        });
    }

    #[gpui::test]
    fn a_finished_read_shows_its_logs(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.select(0, cx);
            app.inspector.logs_loading = true;
            let generation = app.inspector.detail_generation;
            let mut read = outcome("demo-1", generation);
            read.logs = Ok("first\nsecond\n".into());
            app.finish_detail(read, cx);
            assert!(!app.inspector.logs_loading);
            assert_eq!(app.inspector.logs_text, "first\nsecond\n");
        });
    }

    #[gpui::test]
    fn a_clone_keeps_every_variable_and_says_which_need_filling_in(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.select(0, cx);
            app.inspector.inspect_facts = Some(berth_engine::detail::InspectFacts {
                env: vec![
                    ("PORT".into(), "8080".into(), false),
                    ("API_KEY".into(), "<redacted>".into(), true),
                    ("TOKEN".into(), "<redacted>".into(), true),
                ],
                ..Default::default()
            });
            app.clone_container_to_run(cx);
            let env = app.view.run_env.read(cx).text().to_owned();
            assert_eq!(env, "PORT=8080, API_KEY=, TOKEN=");
            assert_eq!(app.view.run_env_missing, 2);
            assert_eq!(app.view.tab, Tab::Images);
            assert!(app.view.creation_open);
        });
        // The clone filled the field in itself; that must not retire the
        // note a moment later, once its own change event is delivered.
        cx.run_until_parked();
        app.read_with(&*cx, |app, _| assert_eq!(app.view.run_env_missing, 2));
        // The user's own edit does.
        cx.update(|window, cx| {
            let input = app.read(cx).view.run_env.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
        });
        cx.simulate_input("X");
        cx.run_until_parked();
        app.read_with(&*cx, |app, _| assert_eq!(app.view.run_env_missing, 0));
    }

    #[gpui::test]
    fn jumping_to_a_container_clears_whatever_was_hiding_it(cx: &mut TestAppContext) {
        let (app, cx) = crate::testkit::fixture(cx);
        app.update(cx, |app, cx| {
            app.view.tab = Tab::Disk;
            app.set_filter(Filter::Stopped, cx);
            app.view.lists[SearchKind::Containers as usize]
                .search
                .update(cx, |input, cx| input.set_value("nothing-matches", cx));
            app.select_container_by_id("demo-1", cx);
            assert_eq!(app.view.tab, Tab::Containers);
            assert_eq!(app.view.filter, Filter::All);
            assert_eq!(app.query(SearchKind::Containers), "");
            assert_eq!(app.view.selected_id.as_deref(), Some("demo-1"));
        });
    }
}
