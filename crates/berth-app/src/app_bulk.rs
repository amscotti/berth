//! Container checkbox mode and its batched actions.

use crate::app::*;
use crate::jobs::mutation::Request;
use gpui::Context;

impl BerthApp {
    pub(crate) fn toggle_bulk(&mut self, cx: &mut Context<Self>) {
        self.bulk_enabled = !self.bulk_enabled;
        self.bulk_selected.clear();
        cx.notify();
    }

    pub(crate) fn bulk_toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.bulk_selected.remove(id) {
            self.bulk_selected.insert(id.to_owned());
        }
        cx.notify();
    }

    pub(crate) fn bulk_select_all(&mut self, cx: &mut Context<Self>) {
        self.bulk_selected = self
            .visible_rows()
            .into_iter()
            .filter_map(|index| self.snapshot.containers.get(index))
            .map(|container| container.id.clone())
            .collect();
        cx.notify();
    }

    pub(crate) fn bulk_clear(&mut self, cx: &mut Context<Self>) {
        self.bulk_selected.clear();
        cx.notify();
    }

    pub(crate) fn bulk_request(&mut self, verb: &str, cx: &mut Context<Self>) {
        if self.bulk_selected.is_empty() {
            return;
        }
        if verb == "Delete" {
            self.pending = Some(Pending {
                method: "BULK".into(),
                path: "delete".into(),
                verb: verb.into(),
                name: format!("{} selected", self.bulk_selected.len()),
            });
            cx.notify();
            return;
        }
        self.bulk_execute(verb, cx);
    }

    pub(crate) fn project_action(&mut self, project: &str, start: bool, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .snapshot
            .containers
            .iter()
            .filter(|container| container.project() == Some(project))
            .filter(|container| container.is_up() != start)
            .map(|container| container.id.clone())
            .collect();
        if !ids.is_empty() {
            self.execute_batch(ids, if start { "Start" } else { "Stop" }, cx);
        }
    }

    pub(crate) fn project_restart(&mut self, project: &str, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .snapshot
            .containers
            .iter()
            .filter(|container| container.project() == Some(project) && container.is_up())
            .map(|container| container.id.clone())
            .collect();
        if !ids.is_empty() {
            self.execute_batch(ids, "Restart", cx);
        }
    }

    pub(crate) fn bulk_execute(&mut self, verb: &str, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.bulk_selected.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        if self.execute_batch(ids, verb, cx) {
            self.bulk_selected.clear();
            cx.notify();
        }
    }

    fn execute_batch(&mut self, ids: Vec<String>, verb: &str, cx: &mut Context<Self>) -> bool {
        if self.mutation_task.is_some() {
            self.notify("wait for the current engine operation to finish", true, cx);
            return false;
        }
        let Some(client) = self.engine.client.clone() else {
            self.notify("no engine is selected", true, cx);
            return false;
        };
        let suffix = match verb {
            "Start" => "/start",
            "Stop" => "/stop",
            "Restart" => "/restart",
            "Delete" => "?force=1",
            _ => return false,
        };
        let method = if verb == "Delete" { "DELETE" } else { "POST" };
        let actions = ids
            .into_iter()
            .map(|id| {
                let encoded = berth_engine::client::encode(&id);
                (method.to_owned(), format!("/containers/{encoded}{suffix}"))
            })
            .collect::<Vec<_>>();
        self.start_mutation(
            Request::Bulk {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                actions,
                verb: verb.to_owned(),
            },
            cx,
        );
        true
    }
}
