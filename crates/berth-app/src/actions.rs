//! What a person can do to a container, to an image, a volume or a network,
//! typed: each knows the request it makes, what a button calls it and what
//! the toast calls it once it has happened, in this one place.
//!
//! A confirmation holds one of these (`Pending`). Nothing between a button
//! and the engine passes a method, a path or a verb along as a string.

use berth_engine::client::Client;
use berth_engine::discovery::Kind;

use crate::jobs::engine_ops::EngineOp;
use crate::jobs::mutation::Request;

/// An action on one container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContainerAction {
    Resume,
    Pause,
    Start,
    Stop,
    Restart,
    Delete,
}

impl ContainerAction {
    /// What the bulk bar offers, in the order it offers them.
    pub(crate) const BULK: [ContainerAction; 4] = [
        ContainerAction::Start,
        ContainerAction::Restart,
        ContainerAction::Stop,
        ContainerAction::Delete,
    ];

    /// What a button calls it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            ContainerAction::Resume => "Resume",
            ContainerAction::Pause => "Pause",
            ContainerAction::Start => "Start",
            ContainerAction::Stop => "Stop",
            ContainerAction::Restart => "Restart",
            ContainerAction::Delete => "Delete",
        }
    }

    /// What the toast calls it once it has happened: "Stopped".
    pub(crate) fn verb(self) -> &'static str {
        match self {
            ContainerAction::Resume => "Resumed",
            ContainerAction::Pause => "Paused",
            ContainerAction::Start => "Started",
            ContainerAction::Stop => "Stopped",
            ContainerAction::Restart => "Restarted",
            ContainerAction::Delete => "Delete",
        }
    }

    /// Delete destroys something: the one that asks first, and the one
    /// drawn in the danger colour.
    pub(crate) fn destroys(self) -> bool {
        matches!(self, ContainerAction::Delete)
    }

    /// The method of its request: DELETE for the one that destroys, POST
    /// for the rest.
    pub(crate) fn method(self) -> &'static str {
        if self.destroys() { "DELETE" } else { "POST" }
    }

    /// What it is doing, to name it in a sentence: "deleting".
    pub(crate) fn doing(self) -> &'static str {
        match self {
            ContainerAction::Resume => "resuming",
            ContainerAction::Pause => "pausing",
            ContainerAction::Start => "starting",
            ContainerAction::Stop => "stopping",
            ContainerAction::Restart => "restarting",
            ContainerAction::Delete => "deleting",
        }
    }

    /// Where its request goes for the container `id`.
    pub(crate) fn path(self, id: &str) -> String {
        let id = berth_engine::client::encode(id);
        match self {
            ContainerAction::Resume => format!("/containers/{id}/unpause"),
            ContainerAction::Pause => format!("/containers/{id}/pause"),
            ContainerAction::Start => format!("/containers/{id}/start"),
            ContainerAction::Stop => format!("/containers/{id}/stop"),
            ContainerAction::Restart => format!("/containers/{id}/restart"),
            ContainerAction::Delete => format!("/containers/{id}?force=1"),
        }
    }
}

/// Removing one image, volume or network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Removal {
    Image,
    Volume,
    Network,
}

impl Removal {
    /// What the confirmation, and the toast, call it. An image is deleted,
    /// as the engine's own command says; a volume or a network is removed.
    pub(crate) fn verb(self) -> &'static str {
        match self {
            Removal::Image => "Delete",
            Removal::Volume | Removal::Network => "Remove",
        }
    }

    /// What it is doing, to name it in a sentence: "removing volume".
    pub(crate) fn doing(self) -> &'static str {
        match self {
            Removal::Image => "removing image",
            Removal::Volume => "removing volume",
            Removal::Network => "removing network",
        }
    }

    /// Where its request goes for the thing's `id`, which is a name for a
    /// volume or a network.
    pub(crate) fn path(self, id: &str) -> String {
        let id = berth_engine::client::encode(id);
        match self {
            Removal::Image => format!("/images/{id}?force=1"),
            Removal::Volume => format!("/volumes/{id}"),
            Removal::Network => format!("/networks/{id}"),
        }
    }
}

/// One thing a request is about: a container to act on, or an image, a
/// volume or a network to remove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Container {
        action: ContainerAction,
        id: String,
        /// What a confirmation calls it: "Delete web?"
        name: String,
    },
    Removal {
        what: Removal,
        id: String,
        name: String,
    },
}

impl Target {
    /// What a confirmation asks.
    pub(crate) fn title(&self) -> String {
        match self {
            Target::Container { action, name, .. } => format!("{} {name}?", action.label()),
            Target::Removal { what, name, .. } => format!("{} {name}?", what.verb()),
        }
    }

    /// The method, the path and the verb of its request.
    pub(crate) fn parts(&self) -> (&'static str, String, &'static str) {
        match self {
            Target::Container { action, id, .. } => {
                (action.method(), action.path(id), action.verb())
            }
            Target::Removal { what, id, .. } => ("DELETE", what.path(id), what.verb()),
        }
    }

    /// The request it is, to send to the engine at `client`.
    pub(crate) fn request(&self, client: &Client) -> Request {
        Request::Action {
            socket: client.socket().to_owned(),
            api: client.api().to_owned(),
            target: self.clone(),
        }
    }
}

/// What a confirmation holds until it is confirmed.
#[derive(Clone, Debug)]
pub(crate) enum Pending {
    /// One request about one thing.
    One(Target),
    /// An action on every container selected.
    Batch {
        action: ContainerAction,
        count: usize,
    },
    /// A command to the machine behind an engine.
    Engine {
        op: EngineOp,
        kind: Kind,
        socket: String,
    },
}

impl Pending {
    /// What the confirmation asks.
    pub(crate) fn title(&self) -> String {
        match self {
            Pending::One(target) => target.title(),
            Pending::Batch { action, count } => format!("{} {count} selected?", action.label()),
            Pending::Engine { op, kind, .. } => format!("{} {}?", op.label(), kind.label()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Client {
        Client::new("/tmp/docker.sock").with_api("v1.43")
    }

    fn asked(target: &Target) -> (String, String, &'static str, String, &'static str) {
        match target.request(&client()) {
            Request::Action {
                socket,
                api,
                target,
            } => {
                let (method, path, verb) = target.parts();
                (socket, api, method, path, verb)
            }
            other => panic!("one thing is one request, not {other:?}"),
        }
    }

    #[test]
    fn an_action_is_one_request_in_the_words_it_was_always_asked_in() {
        let cases = [
            (
                ContainerAction::Resume,
                "POST",
                "/containers/web/unpause",
                "Resumed",
            ),
            (
                ContainerAction::Pause,
                "POST",
                "/containers/web/pause",
                "Paused",
            ),
            (
                ContainerAction::Start,
                "POST",
                "/containers/web/start",
                "Started",
            ),
            (
                ContainerAction::Stop,
                "POST",
                "/containers/web/stop",
                "Stopped",
            ),
            (
                ContainerAction::Restart,
                "POST",
                "/containers/web/restart",
                "Restarted",
            ),
            (
                ContainerAction::Delete,
                "DELETE",
                "/containers/web?force=1",
                "Delete",
            ),
        ];
        for (action, method, path, verb) in cases {
            let target = Target::Container {
                action,
                id: "web".into(),
                name: "web".into(),
            };
            assert_eq!(
                asked(&target),
                (
                    "/tmp/docker.sock".into(),
                    "v1.43".into(),
                    method,
                    path.into(),
                    verb
                )
            );
        }
    }

    #[test]
    fn a_removal_is_one_delete_request() {
        let cases = [
            (Removal::Image, "a1b2c3", "/images/a1b2c3?force=1", "Delete"),
            (Removal::Volume, "shop-data", "/volumes/shop-data", "Remove"),
            (Removal::Network, "back", "/networks/back", "Remove"),
        ];
        for (what, id, path, verb) in cases {
            let target = Target::Removal {
                what,
                id: id.into(),
                name: id.into(),
            };
            assert_eq!(
                asked(&target),
                (
                    "/tmp/docker.sock".into(),
                    "v1.43".into(),
                    "DELETE",
                    path.into(),
                    verb
                )
            );
        }
    }

    #[test]
    fn an_id_that_is_a_name_is_sent_encoded() {
        assert_eq!(
            ContainerAction::Stop.path("odd name"),
            "/containers/odd%20name/stop"
        );
        assert_eq!(
            ContainerAction::Delete.path("abc"),
            "/containers/abc?force=1"
        );
    }

    #[test]
    fn a_confirmation_names_what_it_costs() {
        let web = || Target::Container {
            action: ContainerAction::Delete,
            id: "web".into(),
            name: "web".into(),
        };
        assert_eq!(Pending::One(web()).title(), "Delete web?");
        assert_eq!(
            Pending::One(Target::Removal {
                what: Removal::Volume,
                id: "shop-data".into(),
                name: "shop-data".into(),
            })
            .title(),
            "Remove shop-data?"
        );
        assert_eq!(
            Pending::Batch {
                action: ContainerAction::Stop,
                count: 3
            }
            .title(),
            "Stop 3 selected?"
        );
        assert_eq!(
            Pending::Engine {
                op: EngineOp::Restart,
                kind: Kind::Lima,
                socket: "/tmp/x.sock".into(),
            }
            .title(),
            "Restart Lima?"
        );
    }

    #[test]
    fn only_delete_destroys() {
        for action in ContainerAction::BULK {
            assert_eq!(action.destroys(), action == ContainerAction::Delete);
            assert_eq!(action.method() == "DELETE", action.destroys());
        }
        assert!(!ContainerAction::Resume.destroys());
        assert!(!ContainerAction::Pause.destroys());
    }
}
