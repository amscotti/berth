//! The container row's action column: what a row offers, and the pills it
//! offers it with.
//!
//! A row offers what the container's state allows. An action the engine would
//! refuse isn't put on screen to be clicked and fail; `Actions` is the one
//! table the row, the inspector and the runtime cards all read.

use gpui::prelude::*;
use gpui::{Context, IntoElement, SharedString, div, px};

use berth_engine::dto::{Actions, Container};
use berth_ui::theme::Tokens;

use crate::actions::ContainerAction;
use crate::app::BerthApp;
use crate::ui::kit::button;

/// One action a row offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowAction {
    pub(crate) action: ContainerAction,
    pub(crate) id: String,
    /// Where its pill sits in the row. Fixed for each action, so a pill is
    /// the same element whether or not the others are offered, and two pills
    /// on one row are told apart.
    pub(crate) slot: u8,
}

/// What a row offers for a container: what its state allows, then Delete,
/// which is always offered unless the container is already on its way out.
pub(crate) fn row_actions(id: &str, actions: Actions, removable: bool) -> Vec<RowAction> {
    let mut offered = Vec::new();
    let mut offer = |action, slot| {
        offered.push(RowAction {
            action,
            id: id.to_owned(),
            slot,
        });
    };
    if actions.resume {
        offer(ContainerAction::Resume, 5);
    }
    if actions.start {
        offer(ContainerAction::Start, 3);
    }
    if actions.stop {
        offer(ContainerAction::Stop, 0);
    }
    if actions.restart {
        offer(ContainerAction::Restart, 1);
    }
    if removable {
        offer(ContainerAction::Delete, 2);
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
        let name = name.clone();
        let button = button(
            SharedString::from(format!("action-{id}-{}", action.slot)),
            action.action.label(),
            tokens,
        )
        .small()
        // The table is the tab stop. Enter opens the row, and its inspector
        // offers every action in it; Tab must not cross a hundred rows.
        .tab_stop(false);
        // Deleting destroys: it is the one action drawn in the danger colour.
        let button = if action.action.destroys() {
            button.danger_wash()
        } else {
            button
        };
        cell = cell.child(button.on_click(cx.listener(move |this, _, _, cx| {
            // The press is for the action, not for opening the row.
            cx.stop_propagation();
            this.act_on(action.action, &action.id, &name, cx);
        })));
    }
    cell
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
            .map(|action| action.action.label())
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
                .map(|action| action.action.destroys())
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(
            actions[0].action.path(&actions[0].id),
            "/containers/abc/stop"
        );
        assert_eq!(
            actions[2].action.path(&actions[2].id),
            "/containers/abc?force=1"
        );
        assert_eq!(actions[2].action.method(), "DELETE");
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
                .map(|action| action.action.destroys())
                .collect::<Vec<_>>(),
            [false, false, true]
        );
    }
}
