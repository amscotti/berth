//! Container/image creation, mutation dispatch and cleanup sheets.

use crate::app::*;
use crate::jobs::mutation::{self, PruneOptions, Request};
use berth_core::validation;
use gpui::Context;

impl BerthApp {
    // --- creation and cleanup forms ----------------------------------------

    pub(crate) fn set_creation_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.creation_open = open;
        if !open {
            self.pull_attempted = false;
            self.run_attempted = false;
        }
        cx.notify();
    }

    pub(crate) fn start_pull(&mut self, cx: &mut Context<Self>) {
        let reference = self.pull_input.read(cx).text().trim().to_owned();
        if validation::pull(&reference).is_some() {
            self.pull_attempted = true;
            cx.notify();
            return;
        }
        self.pull_attempted = false;
        let Some(client) = self.engine.client.clone() else {
            self.notify("no engine is selected", true, cx);
            return;
        };
        self.start_mutation(
            Request::Pull {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                reference,
            },
            cx,
        );
    }

    pub(crate) fn start_run(&mut self, cx: &mut Context<Self>) {
        let image = self.run_image.read(cx).text().trim().to_owned();
        let name = self.run_name.read(cx).text().trim().to_owned();
        let ports = self.run_ports.read(cx).text().trim().to_owned();
        let volumes = self.run_volumes.read(cx).text().trim().to_owned();
        let env = self.run_env.read(cx).text().trim().to_owned();
        if validation::run(&image, &name, &ports, &volumes, &env).is_some() {
            self.run_attempted = true;
            cx.notify();
            return;
        }
        self.run_attempted = false;
        let Some(client) = self.engine.client.clone() else {
            self.notify("no engine is selected", true, cx);
            return;
        };
        self.start_mutation(
            Request::Run {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                image,
                name,
                ports,
                volumes,
                env,
            },
            cx,
        );
    }

    pub(crate) fn open_create(&mut self, kind: CreateKind, cx: &mut Context<Self>) {
        self.pending = None;
        self.prune_open = false;
        self.create_kind = Some(kind);
        self.create_attempted = false;
        self.create_name.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    pub(crate) fn close_create(&mut self, cx: &mut Context<Self>) {
        self.create_kind = None;
        self.create_attempted = false;
        cx.notify();
    }

    pub(crate) fn submit_create(&mut self, cx: &mut Context<Self>) {
        let Some(kind) = self.create_kind else {
            return;
        };
        let name = self.create_name.read(cx).text().trim().to_owned();
        let duplicate = kind == CreateKind::Volume
            && self
                .snapshot
                .volumes
                .iter()
                .any(|volume| volume.name == name);
        if name.len() < 2
            || name.len() > 128
            || !name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric())
            || !name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
            || duplicate
        {
            self.create_attempted = true;
            cx.notify();
            return;
        }
        self.create_attempted = false;
        let Some(client) = self.engine.client.clone() else {
            self.notify("no engine is selected", true, cx);
            return;
        };
        self.create_in_flight = true;
        let request = match kind {
            CreateKind::Volume => Request::CreateVolume {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                name,
            },
            CreateKind::Network => Request::CreateNetwork {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                name,
            },
        };
        self.start_mutation(request, cx);
    }

    pub(crate) fn open_prune(&mut self, cx: &mut Context<Self>) {
        self.pending = None;
        self.create_kind = None;
        self.prune_open = true;
        cx.notify();
    }

    pub(crate) fn close_prune(&mut self, cx: &mut Context<Self>) {
        self.prune_open = false;
        self.prune_options = PruneOptions::default();
        cx.notify();
    }

    pub(crate) fn run_prune(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.engine.client.clone() else {
            self.notify("no engine is selected", true, cx);
            return;
        };
        let engine = match self.engine.dialect {
            berth_engine::discovery::Dialect::Podman => berth_engine::prune::Engine::Podman,
            _ => berth_engine::prune::Engine::Docker,
        };
        self.prune_in_flight = true;
        self.start_mutation(
            Request::Prune {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                engine,
                options: self.prune_options.clone(),
            },
            cx,
        );
    }

    pub(crate) fn open_engine_op(
        &mut self,
        verb: &str,
        kind: berth_engine::discovery::Kind,
        socket: String,
        cx: &mut Context<Self>,
    ) {
        if verb == "start" {
            self.start_mutation(
                Request::EngineOp {
                    verb: verb.to_owned(),
                    kind,
                    socket,
                },
                cx,
            );
            return;
        }
        self.pending = Some(Pending {
            method: "ENGINE".to_owned(),
            path: socket,
            verb: verb.to_owned(),
            name: kind.label().to_owned(),
        });
        cx.notify();
    }

    // --- mutations ---------------------------------------------------------

    pub(crate) fn confirm_action(
        &mut self,
        method: String,
        path: String,
        verb: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        self.pending = Some(Pending {
            method,
            path,
            verb,
            name,
        });
        cx.notify();
    }

    pub(crate) fn resolve_pending(&mut self, ok: bool, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if !ok {
            cx.notify();
            return;
        }
        if pending.method == "BULK" {
            self.bulk_execute(&pending.verb, cx);
            return;
        }
        if pending.method == "ENGINE" {
            let kind = berth_engine::discovery::kind_of(&pending.path);
            self.start_mutation(
                Request::EngineOp {
                    verb: pending.verb,
                    kind,
                    socket: pending.path,
                },
                cx,
            );
            return;
        }
        self.run_action(pending.method, pending.path, pending.verb, cx);
    }

    pub(crate) fn run_action(
        &mut self,
        method: String,
        path: String,
        verb: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.engine.client.clone() else {
            self.notify("no engine is selected", true, cx);
            return;
        };
        self.start_mutation(
            Request::Action {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                method,
                path,
                verb,
            },
            cx,
        );
    }

    pub(crate) fn start_mutation(&mut self, request: Request, cx: &mut Context<Self>) {
        if self.mutation_task.is_some() {
            self.notify("wait for the current engine operation to finish", true, cx);
            return;
        }
        let generation = self.engine.generation;
        let is_engine_op = matches!(request, Request::EngineOp { .. });
        self.mutation_task = Some(cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { mutation::run(request) })
                .await;
            this.update(cx, |app, cx| {
                app.finish_mutation(outcome, generation, is_engine_op, cx)
            })
            .ok();
        }));
    }

    fn finish_mutation(
        &mut self,
        outcome: mutation::Outcome,
        generation: u64,
        is_engine_op: bool,
        cx: &mut Context<Self>,
    ) {
        self.mutation_task = None;
        if self.create_in_flight {
            self.create_in_flight = false;
            if !outcome.bad {
                self.create_kind = None;
            }
        }
        if self.prune_in_flight {
            self.prune_in_flight = false;
            if !outcome.bad {
                self.prune_open = false;
                self.prune_options = PruneOptions::default();
            }
        }
        if !is_engine_op && generation != self.engine.generation {
            cx.notify();
            return;
        }
        // Our own action already produces a toast; the engine's die/stop
        // events that follow must not produce a second one.
        self.quiet_notices_until = berth_core::format::now() + 3;
        self.notify(&outcome.message, outcome.bad, cx);
        if is_engine_op {
            self.start_discovery(cx);
        } else {
            self.start_refresh(cx);
        }
    }

    pub(crate) fn mutating(&self) -> bool {
        self.mutation_task.is_some() || self.refresh_task.is_some()
    }

    pub(crate) fn notify(&mut self, text: &str, bad: bool, cx: &mut Context<Self>) {
        self.toast = Some(Toast {
            text: text.to_owned(),
            bad,
        });
        cx.notify();
        self.toast_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(4))
                .await;
            this.update(cx, |app, cx| {
                app.toast = None;
                cx.notify();
            })
            .ok();
        }));
    }
}

pub(crate) fn create_name_error(
    kind: CreateKind,
    name: &str,
    volumes: &[berth_engine::dto::Volume],
) -> Option<String> {
    if name.is_empty() {
        return Some("Enter a name.".to_owned());
    }
    if name.len() < 2 {
        return Some("A name needs at least 2 characters.".to_owned());
    }
    if name.len() > 128 {
        return Some("A name can be at most 128 characters.".to_owned());
    }
    if !name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric())
    {
        return Some("Start the name with a letter or digit.".to_owned());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
    {
        return Some("Use only letters, digits, dot, dash or underscore.".to_owned());
    }
    if kind == CreateKind::Volume && volumes.iter().any(|volume| volume.name == name) {
        return Some(format!("A volume named {name} already exists."));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_names_are_validated_with_the_same_rules_as_the_sheet() {
        assert!(create_name_error(CreateKind::Volume, "", &[]).is_some());
        assert!(create_name_error(CreateKind::Volume, "a", &[]).is_some());
        assert!(create_name_error(CreateKind::Volume, "-bad", &[]).is_some());
        assert!(create_name_error(CreateKind::Volume, "bad name", &[]).is_some());
        assert!(create_name_error(CreateKind::Network, "shop-net", &[]).is_none());
        let volume = berth_engine::dto::Volume {
            name: "data".into(),
            ..Default::default()
        };
        assert!(create_name_error(CreateKind::Volume, "data", &[volume]).is_some());
    }
}
