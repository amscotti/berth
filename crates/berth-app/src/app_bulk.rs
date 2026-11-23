//! Container checkbox mode and its batched actions.

use crate::actions::{ContainerAction, Pending};
use crate::app::*;
use crate::jobs::mutation::Request;
use gpui::Context;

impl BerthApp {
    pub(crate) fn toggle_bulk(&mut self, cx: &mut Context<Self>) {
        self.view.bulk_enabled = !self.view.bulk_enabled;
        self.view.bulk_selected.clear();
        self.redraw_page(cx);
    }

    pub(crate) fn bulk_toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.view.bulk_selected.remove(id) {
            self.view.bulk_selected.insert(id.to_owned());
        }
        self.redraw_page(cx);
    }

    pub(crate) fn bulk_select_all(&mut self, cx: &mut Context<Self>) {
        self.view.bulk_selected = self
            .visible_rows()
            .into_iter()
            .filter_map(|index| self.snapshot.containers.get(index))
            .map(|container| container.id.clone())
            .collect();
        self.redraw_page(cx);
    }

    pub(crate) fn bulk_clear(&mut self, cx: &mut Context<Self>) {
        self.view.bulk_selected.clear();
        self.redraw_page(cx);
    }

    pub(crate) fn bulk_request(&mut self, action: ContainerAction, cx: &mut Context<Self>) {
        if self.view.bulk_selected.is_empty() {
            return;
        }
        if action.destroys() {
            self.confirm(
                Pending::Batch {
                    action,
                    count: self.view.bulk_selected.len(),
                },
                cx,
            );
            return;
        }
        self.bulk_execute(action, cx);
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
            self.execute_batch(
                ids,
                if start {
                    ContainerAction::Start
                } else {
                    ContainerAction::Stop
                },
                cx,
            );
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
            self.execute_batch(ids, ContainerAction::Restart, cx);
        }
    }

    /// `action` on every container selected.
    pub(crate) fn bulk_execute(&mut self, action: ContainerAction, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.view.bulk_selected.iter().cloned().collect();
        if ids.is_empty() {
            return;
        }
        if self.execute_batch(ids, action, cx) {
            self.view.bulk_selected.clear();
            cx.notify();
        }
    }

    fn execute_batch(
        &mut self,
        ids: Vec<String>,
        action: ContainerAction,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(client) = self.engine.client.clone() else {
            self.notify("no engine is selected", true, cx);
            return false;
        };
        self.start_mutation(
            Request::Bulk {
                socket: client.socket().to_owned(),
                api: client.api().to_owned(),
                action,
                ids,
            },
            cx,
        )
    }
}
