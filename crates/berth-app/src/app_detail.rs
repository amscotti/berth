//! Inspector detail jobs.

use crate::app::*;
use crate::app_render::pretty_inspect;
use crate::jobs::detail;
use gpui::Context;

impl BerthApp {
    // --- detail ------------------------------------------------------------

    pub(crate) fn start_detail(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id.clone() else {
            return;
        };
        let Some(client) = self.engine.client.clone() else {
            return;
        };
        self.detail_generation = self.detail_generation.wrapping_add(1);
        let generation = self.detail_generation;
        let request = detail::Request {
            socket: client.socket().to_owned(),
            api: client.api().to_owned(),
            id: id.clone(),
            generation,
            timestamps: self.logs_timestamps,
            extras: true,
        };
        self.logs_loading = true;
        self.inspect_loading = true;
        self.detail_task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { detail::load(request) })
                .await;
            this.update(cx, |app, cx| app.finish_detail(outcome, cx))
                .ok();
        }));
        cx.notify();
    }

    fn finish_detail(&mut self, outcome: detail::Outcome, cx: &mut Context<Self>) {
        if outcome.generation != self.detail_generation
            || self.selected_id.as_deref() != Some(outcome.id.as_str())
        {
            return;
        }
        self.detail_task = None;
        self.logs_loading = false;
        self.inspect_loading = false;
        self.detail_top = outcome.top;
        self.detail_changes = outcome.changes;
        self.logs_text = outcome
            .logs
            .unwrap_or_else(|error| format!("could not read logs: {error}"));
        self.inspect_text = match outcome.inspect {
            Ok(detail) => {
                self.detail_lifetime = Some(berth_engine::detail::Lifetime::from_detail(&detail));
                Some(pretty_inspect(&detail))
            }
            Err(error) => Some(format!("could not read inspect: {error}")),
        };
        cx.notify();
    }

    fn finish_image_detail(&mut self, outcome: detail::ImageOutcome, cx: &mut Context<Self>) {
        if outcome.generation != self.detail_generation
            || self.selected_id.as_deref() != Some(outcome.id.as_str())
        {
            return;
        }
        self.detail_task = None;
        self.image_layers_loading = false;
        if let Ok(layers) = outcome.layers {
            self.image_layers = layers;
        }
        cx.notify();
    }

    pub(crate) fn open_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id.clone() else {
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

    pub(crate) fn toggle_log_timestamps(&mut self, cx: &mut Context<Self>) {
        self.logs_timestamps = !self.logs_timestamps;
        self.start_detail(cx);
        cx.notify();
    }

    pub(crate) fn copy_logs(&mut self, cx: &mut Context<Self>) {
        if !self.logs_text.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(self.logs_text.clone()));
            self.notify("Logs copied", false, cx);
        }
    }

    pub(crate) fn copy_run_command(&mut self, cx: &mut Context<Self>) {
        let Some(lifetime) = &self.detail_lifetime else {
            return;
        };
        let Some(command) = &lifetime.run_cmd else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(command.clone()));
        if lifetime.run_redacted {
            self.notify("Run command copied · some env values redacted", false, cx);
        } else {
            self.notify("Run command copied", false, cx);
        }
    }

    pub(crate) fn copy_inspect(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = &self.inspect_text {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
            self.notify("Inspect JSON copied", false, cx);
        }
    }

    pub(crate) fn select_container_by_id(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(ix) = self.snapshot.containers.iter().position(|c| c.id == id) {
            self.tab = Tab::Containers;
            self.filter = Filter::All;
            self.select(ix, cx);
        }
    }

    pub(crate) fn clone_container_to_run(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id.clone() else {
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
        let (volumes, env) = if let Some(life) = &self.detail_lifetime {
            let vols = life
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
            let envs = life
                .env
                .iter()
                .filter(|(_, _, secret)| !*secret)
                .map(|(k, v, _)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ");
            (vols, envs)
        } else {
            (String::new(), String::new())
        };
        let image = container.image.clone();
        let next_name = format!("{}-copy", container.name());
        self.tab = Tab::Images;
        self.clear_selection(cx);
        self.creation_open = true;
        self.run_image
            .update(cx, |input, cx| input.set_value(image, cx));
        self.run_name
            .update(cx, |input, cx| input.set_value(next_name, cx));
        self.run_ports
            .update(cx, |input, cx| input.set_value(ports, cx));
        self.run_volumes
            .update(cx, |input, cx| input.set_value(volumes, cx));
        self.run_env
            .update(cx, |input, cx| input.set_value(env, cx));
        cx.notify();
    }

    pub(crate) fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.selected = Some(ix);
        self.selected_id = self.snapshot.containers.get(ix).map(|row| row.id.clone());
        self.selected_name = self
            .snapshot
            .containers
            .get(ix)
            .map(|row| row.name().to_owned())
            .unwrap_or_default();
        self.detail_kind = DetailKind::Container;
        self.start_detail(cx);
        cx.notify();
    }

    pub(crate) fn select_image(&mut self, id: &str, name: &str, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.selected_id = Some(id.to_owned());
        self.selected_name = name.to_owned();
        self.detail_kind = DetailKind::Image;
        if let Some(client) = self.engine.client.clone() {
            self.detail_generation = self.detail_generation.wrapping_add(1);
            let generation = self.detail_generation;
            let socket = client.socket().to_owned();
            let api = client.api().to_owned();
            let id_owned = id.to_owned();
            self.image_layers_loading = true;
            self.detail_task = Some(cx.spawn(async move |this, cx| {
                let outcome = cx
                    .background_executor()
                    .spawn(async move { detail::load_image(socket, api, id_owned, generation) })
                    .await;
                this.update(cx, |app, cx| app.finish_image_detail(outcome, cx))
                    .ok();
            }));
        }
        cx.notify();
    }

    pub(crate) fn select_volume(&mut self, name: &str, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.selected_id = Some(name.to_owned());
        self.selected_name = name.to_owned();
        self.detail_kind = DetailKind::Volume;
        cx.notify();
    }

    pub(crate) fn select_network(&mut self, id: &str, name: &str, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        self.selected_id = Some(id.to_owned());
        self.selected_name = name.to_owned();
        self.detail_kind = DetailKind::Network;
        cx.notify();
    }

    pub(crate) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.selected_id = None;
        self.selected_name.clear();
        self.inspector_tab = InspectorTab::Facts;
        self.logs_text.clear();
        self.logs_loading = false;
        self.inspect_text = None;
        self.inspect_loading = false;
        self.detail_lifetime = None;
        self.detail_top = None;
        self.detail_changes.clear();
        self.image_layers.clear();
        self.image_layers_loading = false;
        self.detail_task = None;
        self.detail_generation = self.detail_generation.wrapping_add(1);
        cx.notify();
    }
}
