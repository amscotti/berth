//! One engine mutation at a time: container actions, create, pull/run, prune
//! and machine commands. The worker owns every copied string and returns a
//! sentence for the rail toast.

use berth_engine::client::Client;
use berth_engine::discovery::Dialect;
use berth_engine::discovery::Kind;
use berth_engine::prune;

use crate::actions::{ContainerAction, Target};

use super::engine_ops::{self, EngineOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PruneOptions {
    pub containers: bool,
    pub images: bool,
    pub volumes: bool,
    pub networks: bool,
    pub build_cache: bool,
}

impl Default for PruneOptions {
    fn default() -> Self {
        Self {
            containers: true,
            images: true,
            volumes: false,
            networks: false,
            build_cache: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Request {
    Action {
        socket: String,
        api: String,
        target: Target,
    },
    CreateVolume {
        socket: String,
        api: String,
        name: String,
    },
    CreateNetwork {
        socket: String,
        api: String,
        name: String,
    },
    Pull {
        socket: String,
        api: String,
        reference: String,
    },
    Run {
        socket: String,
        api: String,
        image: String,
        name: String,
        ports: String,
        volumes: String,
        env: String,
    },
    Prune {
        socket: String,
        api: String,
        dialect: Dialect,
        options: PruneOptions,
    },
    EngineOp {
        op: EngineOp,
        kind: Kind,
        socket: String,
    },
    /// One action on each of the containers, in order.
    Bulk {
        socket: String,
        api: String,
        action: ContainerAction,
        ids: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub message: String,
    pub bad: bool,
}

impl Outcome {
    /// A failure the toast shows as an error.
    pub(super) fn bad(message: String) -> Self {
        Self { message, bad: true }
    }
}

pub fn run(request: Request) -> Outcome {
    match request {
        Request::Action {
            socket,
            api,
            target,
        } => action(&socket, &api, &target),
        Request::CreateVolume { socket, api, name } => {
            let client = Client::new(socket).with_api(api);
            match client.create_volume(&name) {
                Ok(()) => ok(format!("Created volume {name}")),
                Err(error) => bad(error.message()),
            }
        }
        Request::CreateNetwork { socket, api, name } => {
            let client = Client::new(socket).with_api(api);
            match client.create_network(&name) {
                Ok(()) => ok(format!("Created network {name}")),
                Err(error) => bad(error.message()),
            }
        }
        Request::Pull {
            socket,
            api,
            reference,
        } => {
            let client = Client::new(socket).with_api(api);
            match client.pull(&reference) {
                Ok(()) => ok(format!("Pulled {reference}")),
                Err(error) => bad(error.message()),
            }
        }
        Request::Run {
            socket,
            api,
            image,
            name,
            ports,
            volumes,
            env,
        } => run_container(&socket, &api, &image, &name, &ports, &volumes, &env),
        Request::Prune {
            socket,
            api,
            dialect,
            options,
        } => prune_stores(&socket, &api, dialect, &options),
        Request::EngineOp { op, kind, socket } => engine_ops::run(op, kind, &socket),
        Request::Bulk {
            socket,
            api,
            action,
            ids,
        } => bulk(&socket, &api, action, &ids),
    }
}

fn bulk(socket: &str, api: &str, action: ContainerAction, ids: &[String]) -> Outcome {
    let client = Client::new(socket).with_api(api);
    let total = ids.len();
    let mut done = 0;
    let mut first_error: Option<String> = None;
    for id in ids {
        match client.action(action.method(), &action.path(id)) {
            Ok(()) => done += 1,
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.message());
                }
            }
        }
    }
    if let Some(error) = first_error {
        return bad(format!("{} {done} of {total}: {error}", action.label()));
    }
    ok(format!("{} {done} of {total} containers", action.label()))
}

fn action(socket: &str, api: &str, target: &Target) -> Outcome {
    let client = Client::new(socket).with_api(api);
    let (method, path, verb) = target.parts();
    match client.action(method, &path) {
        Ok(()) => ok(verb.to_owned()),
        Err(error) => bad(error.message()),
    }
}

fn run_container(
    socket: &str,
    api: &str,
    image: &str,
    name: &str,
    ports: &str,
    volumes: &str,
    env: &str,
) -> Outcome {
    let client = Client::new(socket).with_api(api);
    let body = match container_body(image, name, ports, volumes, env) {
        Ok(body) => body,
        // A last line. A run is checked by the form, by the same rule, before
        // it is queued, so this only fires for a run that came another way.
        Err(what) => return bad(what),
    };
    match client.create_container(&body, name) {
        Ok(id) => match client.start_container(&id) {
            Ok(()) => ok(if name.is_empty() {
                format!("Started {image}")
            } else {
                format!("Started {name}")
            }),
            Err(error) => bad(error.message()),
        },
        Err(error) => bad(error.message()),
    }
}

/// The body of a create-container request. It is built only from a run the
/// form's own rule accepts, so the worker can't be lenient where the form is
/// strict: an address with no port to publish on would otherwise ask the
/// engine for any free port.
fn container_body(
    image: &str,
    name: &str,
    ports: &str,
    volumes: &str,
    env: &str,
) -> Result<Vec<u8>, String> {
    if let Some(failure) = berth_core::validation::run(image, name, ports, volumes, env) {
        return Err(berth_core::validation::describe(&failure));
    }

    let mut exposed = serde_json::Map::new();
    let mut bindings = serde_json::Map::new();
    for entry in ports
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let (rest, container_spec) = entry.rsplit_once(':').ok_or_else(|| unreadable(entry))?;
        let (raw_ip, host) = match rest.rsplit_once(':') {
            None => ("", rest),
            Some((ip, host)) if !ip.is_empty() => (ip, host),
            Some(_) => return Err(unreadable(entry)),
        };
        let host_ip = raw_ip
            .strip_prefix('[')
            .and_then(|ip| ip.strip_suffix(']'))
            .unwrap_or(raw_ip);
        let key = if container_spec.contains('/') {
            container_spec.to_owned()
        } else {
            format!("{container_spec}/tcp")
        };
        exposed.insert(key.clone(), serde_json::json!({}));
        bindings.insert(
            key,
            serde_json::json!([{ "HostIp": host_ip, "HostPort": host }]),
        );
    }

    let list = |text: &str| {
        text.split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let binds = list(volumes);
    let env = list(env);

    let mut host_config = serde_json::Map::new();
    if !binds.is_empty() {
        host_config.insert("Binds".into(), serde_json::json!(binds));
    }
    if !bindings.is_empty() {
        host_config.insert("PortBindings".into(), serde_json::Value::Object(bindings));
    }
    let body = serde_json::json!({
        "Image": image,
        "Env": env,
        "ExposedPorts": exposed,
        "HostConfig": host_config,
    });
    serde_json::to_vec(&body).map_err(|error| format!("The request could not be written: {error}"))
}

/// What to say about an entry the rule above accepted and this couldn't read:
/// the two disagree, which is a fault in berth, never in the run.
fn unreadable(entry: &str) -> String {
    format!(
        "berth couldn't read {entry:?} as a port, though its own rule accepted it. Nothing was run."
    )
}

fn prune_stores(socket: &str, api: &str, dialect: Dialect, options: &PruneOptions) -> Outcome {
    let client = Client::new(socket).with_api(api);
    let plan = prune::plan(
        options.containers,
        options.images,
        options.volumes,
        options.networks,
        options.build_cache,
        dialect,
    );
    let mut freed = 0u64;
    let mut failures = Vec::new();
    // Every step runs even when an earlier one fails: a store that can't be
    // pruned is no reason to leave the others full.
    for (kind, routes) in plan {
        for route in routes {
            match client.prune(route) {
                Ok(bytes) => prune::add_reclaimed(&mut freed, bytes),
                Err(error) => failures.push((kind.label(), error.message())),
            }
        }
    }
    if failures.is_empty() {
        return ok(format!("Reclaimed {}", berth_core::format::bytes(freed)));
    }
    // Name each store once, then the first error in full.
    let mut stores = Vec::new();
    for (store, _) in &failures {
        if !stores.contains(store) {
            stores.push(*store);
        }
    }
    bad(format!(
        "Reclaimed {} · {} failed: {}",
        berth_core::format::bytes(freed),
        stores.join(", "),
        failures[0].1
    ))
}

fn ok(message: String) -> Outcome {
    Outcome {
        message,
        bad: false,
    }
}

fn bad(message: String) -> Outcome {
    Outcome { message, bad: true }
}

#[cfg(test)]
mod tests {
    use super::*;
    use berth_engine::test_support::MockEngine;

    #[test]
    fn a_run_body_has_ports_env_and_binds() {
        let body =
            container_body("nginx", "web", "127.0.0.1:8080:80", "data:/data", "A=1").expect("body");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(value["Image"], "nginx");
        assert_eq!(
            value["HostConfig"]["PortBindings"]["80/tcp"][0]["HostIp"],
            "127.0.0.1"
        );
        assert_eq!(
            value["HostConfig"]["PortBindings"]["80/tcp"][0]["HostPort"],
            "8080"
        );
        assert_eq!(value["HostConfig"]["Binds"][0], "data:/data");
        assert_eq!(value["Env"][0], "A=1");
    }

    #[test]
    fn a_run_the_form_refuses_is_refused_here_too_in_the_same_words() {
        // The worker is a last line, not a second opinion: it applies the
        // form's rule, and says what the form says.
        let error = container_body("nginx", "", "8080", "", "").expect_err("no colon");
        assert_eq!(
            error,
            "Port 8080 needs host:container or ip:host:container."
        );
        // An address with no port to publish on. Read leniently, this would
        // ask the engine for any free port.
        let error = container_body("nginx", "", "127.0.0.1::80", "", "").expect_err("no host port");
        assert!(error.contains("needs host:container"), "{error}");
        let error = container_body("nginx", "bad name", "8080:80", "", "").expect_err("bad name");
        assert_eq!(
            error,
            "Container names are letters, digits, dot, dash and underscore."
        );
    }

    #[test]
    fn prune_runs_every_step_and_reports_what_it_freed_and_what_failed() {
        let engine = MockEngine::start();
        engine
            .on("POST /containers/prune")
            .ok(r#"{"SpaceReclaimed":100}"#)
            .on("POST /images/prune")
            .error(500, "image prune refused")
            .on("POST /volumes/prune")
            .ok(r#"{"SpaceReclaimed":50}"#)
            .on("POST /build/prune")
            .ok(r#"{"SpaceReclaimed":25}"#);
        let outcome = prune_stores(
            &engine.socket(),
            "v1.43",
            Dialect::Docker,
            &PruneOptions {
                containers: true,
                images: true,
                volumes: true,
                networks: false,
                build_cache: true,
            },
        );
        assert!(outcome.bad);
        assert!(
            outcome.message.contains("Reclaimed 175 B"),
            "{}",
            outcome.message
        );
        assert!(
            outcome.message.contains("unused images"),
            "{}",
            outcome.message
        );
        assert!(
            outcome.message.contains("image prune refused"),
            "{}",
            outcome.message
        );
        // The steps after the failure still ran.
        assert_eq!(engine.count("/volumes/prune"), 1);
        assert_eq!(engine.count("/build/prune"), 1);
    }

    #[test]
    fn a_batch_sends_its_one_action_to_every_container_and_says_how_many() {
        let engine = MockEngine::start();
        engine.on("POST /containers/").ok("");
        engine.on("DELETE /containers/").error(500, "it is running");
        let batch = |action: ContainerAction, ids: &[&str]| {
            run(Request::Bulk {
                socket: engine.socket(),
                api: "v1.43".into(),
                action,
                ids: ids.iter().map(|id| (*id).to_owned()).collect(),
            })
        };

        let outcome = batch(ContainerAction::Stop, &["web", "odd name"]);
        assert!(!outcome.bad, "{}", outcome.message);
        assert_eq!(outcome.message, "Stop 2 of 2 containers");
        assert_eq!(engine.count("/containers/web/stop"), 1);
        assert_eq!(engine.count("/containers/odd%20name/stop"), 1);

        // Every container is tried even once one has failed, and the count
        // says how far it got.
        let outcome = batch(ContainerAction::Delete, &["web", "db"]);
        assert!(outcome.bad);
        assert_eq!(
            outcome.message,
            "Delete 0 of 2: engine returned 500: it is running"
        );
        assert_eq!(engine.count("/containers/web?force=1"), 1);
        assert_eq!(engine.count("/containers/db?force=1"), 1);
        assert_eq!(engine.count("/containers/"), 4, "nothing else was sent");
    }

    #[test]
    fn a_prune_outcome_names_the_reclaimed_bytes() {
        let options = PruneOptions {
            containers: false,
            images: false,
            volumes: false,
            networks: false,
            build_cache: false,
        };
        let outcome = prune_stores("missing.sock", "v1.43", Dialect::Docker, &options);
        // No plan means no request and a zero-byte sentence.
        assert!(!outcome.bad);
        assert!(outcome.message.contains("0 B"));
    }
}
