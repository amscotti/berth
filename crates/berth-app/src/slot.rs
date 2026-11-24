//! The one engine mutation in flight. Container actions, a batch, create,
//! pull and run, prune, and machine commands all take the same slot: a
//! second request is refused, and the refusal says what it is waiting for.

use std::time::{Duration, Instant};

use berth_engine::discovery::Kind;
use gpui::Task;

use crate::actions::{ContainerAction, Target};
use crate::jobs::engine_ops::EngineOp;
use crate::jobs::mutation::Request;
use crate::view::Sheet;

/// Which engine mutation is running: named, so a sentence can say what it
/// is, and so the sheet waiting on it can be told apart from another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MutationKind {
    /// An action on one container, or removing an image, a volume or a
    /// network.
    One(Target),
    /// An action on each of the containers, in order.
    Batch {
        action: ContainerAction,
        count: usize,
    },
    CreateVolume {
        name: String,
    },
    CreateNetwork {
        name: String,
    },
    Pull {
        reference: String,
    },
    Run {
        image: String,
    },
    Prune,
    /// A command to the machine behind an engine.
    EngineOp {
        op: EngineOp,
        kind: Kind,
    },
}

impl MutationKind {
    /// What `request` is.
    pub(crate) fn of(request: &Request) -> Self {
        match request {
            Request::Action { target, .. } => MutationKind::One(target.clone()),
            Request::CreateVolume { name, .. } => MutationKind::CreateVolume { name: name.clone() },
            Request::CreateNetwork { name, .. } => {
                MutationKind::CreateNetwork { name: name.clone() }
            }
            Request::Pull { reference, .. } => MutationKind::Pull {
                reference: reference.clone(),
            },
            Request::Run { image, .. } => MutationKind::Run {
                image: image.clone(),
            },
            Request::Prune { .. } => MutationKind::Prune,
            Request::Bulk { action, ids, .. } => MutationKind::Batch {
                action: *action,
                count: ids.len(),
            },
            Request::EngineOp { op, kind, .. } => MutationKind::EngineOp {
                op: *op,
                kind: *kind,
            },
        }
    }

    /// What is happening, to name it in a sentence: "deleting web".
    pub(crate) fn doing(&self) -> String {
        match self {
            MutationKind::One(Target::Container { action, name, .. }) => {
                format!("{} {name}", action.doing())
            }
            MutationKind::One(Target::Removal { what, name, .. }) => {
                format!("{} {name}", what.doing())
            }
            MutationKind::Batch { action, count } => format!(
                "{} {}",
                action.doing(),
                berth_core::format::count(*count, "container", "containers")
            ),
            MutationKind::CreateVolume { name } => format!("creating volume {name}"),
            MutationKind::CreateNetwork { name } => format!("creating network {name}"),
            MutationKind::Pull { reference } => format!("pulling {reference}"),
            MutationKind::Run { image } => format!("running {image}"),
            MutationKind::Prune => "cleaning up".to_owned(),
            MutationKind::EngineOp { op, kind } => format!("{} {}", op.doing(), kind.label()),
        }
    }

    /// Whether `sheet` is waiting on this one. A sheet closes when its own
    /// request succeeds, and the sheet that is open by then has to be the one
    /// that asked: another sheet opened over it is left alone.
    pub(crate) fn frees(&self, sheet: &Sheet) -> bool {
        matches!(
            (self, sheet),
            (
                MutationKind::CreateVolume { .. },
                Sheet::Create {
                    kind: crate::app::CreateKind::Volume,
                    ..
                }
            ) | (
                MutationKind::CreateNetwork { .. },
                Sheet::Create {
                    kind: crate::app::CreateKind::Network,
                    ..
                }
            ) | (MutationKind::Prune, Sheet::Prune(_))
        )
    }

    /// The sentence a second request is refused with. It names what is
    /// running, and how long it has been once that is worth saying.
    pub(crate) fn refusal(&self, running_for: Duration) -> String {
        let doing = self.doing();
        if running_for >= Duration::from_secs(1) {
            format!(
                "still {doing} ({} s so far); wait for it to finish",
                running_for.as_secs()
            )
        } else {
            format!("still {doing}; wait for it to finish")
        }
    }
}

/// The slot one engine mutation occupies at a time.
#[derive(Default)]
pub(crate) enum MutationSlot {
    #[default]
    Idle,
    Running {
        kind: MutationKind,
        /// When it was asked for.
        since: Instant,
        /// Dropping it cancels the request.
        task: Task<()>,
    },
}

impl MutationSlot {
    /// Whether a mutation is running.
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, MutationSlot::Running { .. })
    }

    /// Free the slot once its mutation has finished, and say what ran, so
    /// that what was waiting on it can be told apart from something else.
    /// `None` when nothing was running.
    pub(crate) fn finish(&mut self) -> Option<MutationKind> {
        match std::mem::replace(self, MutationSlot::Idle) {
            // The request has arrived, so its handle is let go here. Dropping
            // it any sooner is what would have cancelled it.
            MutationSlot::Running { kind, task, .. } => {
                drop(task);
                Some(kind)
            }
            MutationSlot::Idle => None,
        }
    }

    /// What a second request is refused with, or `None` when nothing is
    /// running.
    pub(crate) fn refusal(&self) -> Option<String> {
        match self {
            MutationSlot::Idle => None,
            MutationSlot::Running { kind, since, .. } => Some(kind.refusal(since.elapsed())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(action: ContainerAction) -> MutationKind {
        MutationKind::One(Target::Container {
            action,
            id: "web".into(),
            name: "web".into(),
        })
    }

    #[test]
    fn every_mutation_is_named_for_a_sentence() {
        let cases = [
            (target(ContainerAction::Delete), "deleting web"),
            (target(ContainerAction::Stop), "stopping web"),
            (target(ContainerAction::Restart), "restarting web"),
            (
                MutationKind::One(Target::Removal {
                    what: crate::actions::Removal::Volume,
                    id: "shop-data".into(),
                    name: "shop-data".into(),
                }),
                "removing volume shop-data",
            ),
            (
                MutationKind::Batch {
                    action: ContainerAction::Stop,
                    count: 3,
                },
                "stopping 3 containers",
            ),
            (
                MutationKind::Batch {
                    action: ContainerAction::Start,
                    count: 1,
                },
                "starting 1 container",
            ),
            (
                MutationKind::CreateVolume {
                    name: "data".into(),
                },
                "creating volume data",
            ),
            (
                MutationKind::CreateNetwork {
                    name: "back".into(),
                },
                "creating network back",
            ),
            (
                MutationKind::Pull {
                    reference: "nginx:alpine".into(),
                },
                "pulling nginx:alpine",
            ),
            (
                MutationKind::Run {
                    image: "alpine:3.22".into(),
                },
                "running alpine:3.22",
            ),
            (MutationKind::Prune, "cleaning up"),
            (
                MutationKind::EngineOp {
                    op: EngineOp::Stop,
                    kind: Kind::Lima,
                },
                "stopping Lima",
            ),
        ];
        for (kind, doing) in cases {
            assert_eq!(kind.doing(), doing);
            assert_eq!(
                kind.refusal(Duration::ZERO),
                format!("still {doing}; wait for it to finish")
            );
        }
    }

    #[test]
    fn a_refusal_says_how_long_once_that_is_worth_saying() {
        let kind = MutationKind::Prune;
        assert_eq!(
            kind.refusal(Duration::from_millis(400)),
            "still cleaning up; wait for it to finish"
        );
        assert_eq!(
            kind.refusal(Duration::from_secs(30)),
            "still cleaning up (30 s so far); wait for it to finish"
        );
    }

    #[test]
    fn a_sheet_is_freed_only_by_its_own_request() {
        let create = MutationKind::CreateVolume {
            name: "data".into(),
        };
        assert!(create.frees(&Sheet::Create {
            kind: crate::app::CreateKind::Volume,
            attempted: true
        }));
        assert!(!create.frees(&Sheet::Create {
            kind: crate::app::CreateKind::Network,
            attempted: true
        }));
        let create_net = MutationKind::CreateNetwork {
            name: "back".into(),
        };
        assert!(create_net.frees(&Sheet::Create {
            kind: crate::app::CreateKind::Network,
            attempted: true
        }));
        assert!(!create_net.frees(&Sheet::Create {
            kind: crate::app::CreateKind::Volume,
            attempted: true
        }));
        assert!(!create.frees(&Sheet::Prune(Default::default())));
        assert!(!MutationKind::Prune.frees(&Sheet::Create {
            kind: crate::app::CreateKind::Volume,
            attempted: false
        }));
        assert!(MutationKind::Prune.frees(&Sheet::Prune(Default::default())));
        // Nothing of the page's work keeps a sheet waiting.
        assert!(!target(ContainerAction::Stop).frees(&Sheet::Prune(Default::default())));
    }

    #[test]
    fn a_request_names_its_own_kind() {
        let socket = String::from("/tmp/docker.sock");
        let api = String::from("v1.43");
        let stop = Target::Container {
            action: ContainerAction::Stop,
            id: "web".into(),
            name: "web".into(),
        };
        let cases = [
            (
                Request::Action {
                    socket: socket.clone(),
                    api: api.clone(),
                    target: stop.clone(),
                },
                target(ContainerAction::Stop),
            ),
            (
                Request::CreateVolume {
                    socket: socket.clone(),
                    api: api.clone(),
                    name: "data".into(),
                },
                MutationKind::CreateVolume {
                    name: "data".into(),
                },
            ),
            (
                Request::Prune {
                    socket: socket.clone(),
                    api: api.clone(),
                    dialect: berth_engine::discovery::Dialect::Docker,
                    options: crate::jobs::mutation::PruneOptions::default(),
                },
                MutationKind::Prune,
            ),
            (
                Request::EngineOp {
                    op: EngineOp::Restart,
                    kind: Kind::Colima,
                    socket: socket.clone(),
                },
                MutationKind::EngineOp {
                    op: EngineOp::Restart,
                    kind: Kind::Colima,
                },
            ),
        ];
        for (request, kind) in cases {
            assert_eq!(MutationKind::of(&request), kind);
        }
    }
}
