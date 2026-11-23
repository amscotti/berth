//! The container row's action column: what a row offers, and the pills it
//! offers it with.
//!
//! A row offers what the container's state allows. An action the engine would
//! refuse isn't put on screen to be clicked and fail; `Actions` is the one
//! table the row, the inspector and the runtime cards all read.

use gpui::prelude::*;
use gpui::{Context, IntoElement, Role, div, px};

use berth_engine::dto::{Actions, Container};

use crate::app::BerthApp;
use berth_ui::theme::Tokens;
use berth_ui::typography as type_scale;

/// One action a row offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowAction {
    pub label: &'static str,
    /// Tells two pills on one row apart in the element tree.
    pub slot: u8,
    pub danger: bool,
    pub method: &'static str,
    pub path: String,
    /// The verb the toast uses once it has happened: "Stopped", "Restarted".
    pub past: &'static str,
    /// Whether it asks first.
    pub confirm: bool,
}

/// What a row offers for a container: what its state allows, then Delete,
/// which is always offered unless the container is already on its way out.
pub(crate) fn row_actions(id: &str, actions: Actions, removable: bool) -> Vec<RowAction> {
    let mut offered = Vec::new();
    let mut offer = |label, slot, danger, method, suffix, past, confirm| {
        offered.push(RowAction {
            label,
            slot,
            danger,
            method,
            path: format!("/containers/{id}{suffix}"),
            past,
            confirm,
        });
    };
    if actions.resume {
        offer("Resume", 5, false, "POST", "/unpause", "Resumed", false);
    }
    if actions.start {
        offer("Start", 3, false, "POST", "/start", "Started", false);
    }
    if actions.stop {
        offer("Stop", 0, false, "POST", "/stop", "Stopped", false);
    }
    if actions.restart {
        offer("Restart", 1, false, "POST", "/restart", "Restarted", false);
    }
    if removable {
        offer("Delete", 2, true, "DELETE", "?force=1", "Delete", true);
    }
    offered
}

/// The actions column of one container row.
pub(crate) fn action_cell(
    container: &Container,
    width: f32,
    tokens: Tokens,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let id = container.id.clone();
    let name = container.name().to_owned();
    let actions = row_actions(&id, container.actions(), container.state != "removing");

    let mut cell = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .w(px(width))
        .gap(px(4.0));
    for action in actions {
        cell = cell.child(action_pill(
            action.label,
            &id,
            action.slot,
            action.danger,
            tokens,
            action.method,
            action.path,
            action.past,
            action.confirm,
            &name,
            cx,
        ));
    }
    cell
}

#[allow(clippy::too_many_arguments)]
fn action_pill(
    label: &'static str,
    container_id: &str,
    slot: u8,
    danger: bool,
    tokens: Tokens,
    method: &'static str,
    path: String,
    verb: &'static str,
    confirm: bool,
    name: &str,
    cx: &mut Context<BerthApp>,
) -> impl IntoElement {
    let color = if danger {
        tokens.danger_text()
    } else {
        tokens.text
    };
    let click_name = name.to_owned();
    let method = method.to_owned();
    div()
        .id(gpui::SharedString::from(format!(
            "action-{container_id}-{slot}"
        )))
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .px(px(8.0))
        .rounded(px(12.0))
        .bg(if danger {
            tokens.danger_text().alpha(0.10)
        } else {
            tokens.surface_hover
        })
        .role(Role::Button)
        .aria_label(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            if confirm {
                this.confirm_action(
                    method.clone(),
                    path.clone(),
                    verb.to_owned(),
                    click_name.clone(),
                    cx,
                );
            } else {
                this.run_action(method.clone(), path.clone(), verb.to_owned(), cx);
            }
        }))
        .child(
            div()
                .text_size(px(type_scale::SMALL_BUTTON))
                .text_color(color)
                .child(label),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(state: &str) -> Vec<&'static str> {
        let container = Container {
            state: state.into(),
            ..Default::default()
        };
        row_actions("c", container.actions(), state != "removing")
            .into_iter()
            .map(|action| action.label)
            .collect()
    }

    #[test]
    fn a_row_offers_only_what_its_state_allows() {
        assert_eq!(labels("running"), ["Stop", "Restart", "Delete"]);
        // A paused container can be resumed or stopped, not restarted.
        assert_eq!(labels("paused"), ["Resume", "Stop", "Delete"]);
        assert_eq!(labels("restarting"), ["Stop", "Delete"]);
        assert_eq!(labels("created"), ["Start", "Delete"]);
        assert_eq!(labels("exited"), ["Start", "Delete"]);
        // On its way out: nothing at all, not even Delete.
        assert_eq!(labels("removing"), Vec::<&str>::new());
    }

    #[test]
    fn only_delete_asks_first() {
        let container = Container {
            id: "abc".into(),
            state: "running".into(),
            ..Default::default()
        };
        let actions = row_actions(&container.id, container.actions(), true);
        assert_eq!(
            actions
                .iter()
                .map(|action| action.confirm)
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(actions[0].path, "/containers/abc/stop");
        assert_eq!(actions[2].path, "/containers/abc?force=1");
        assert_eq!(actions[2].method, "DELETE");
        // Every pill on the row has its own place in the element tree.
        let slots: std::collections::BTreeSet<u8> =
            actions.iter().map(|action| action.slot).collect();
        assert_eq!(slots.len(), actions.len());
    }

    #[test]
    fn a_pill_is_danger_coloured_only_for_delete() {
        let container = Container {
            state: "paused".into(),
            ..Default::default()
        };
        let actions = row_actions("c", container.actions(), true);
        assert_eq!(
            actions
                .iter()
                .map(|action| action.danger)
                .collect::<Vec<_>>(),
            [false, false, true]
        );
    }
}
