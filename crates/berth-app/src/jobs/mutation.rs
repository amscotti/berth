//! One engine mutation at a time: container actions, create, pull/run, prune
//! and machine commands. The worker owns every copied string and returns a
//! sentence for the rail toast.

use berth_engine::client::Client;
use berth_engine::discovery::Kind;
use berth_engine::prune::{self, Engine};

use super::engine_ops;

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
        method: String,
        path: String,
        verb: String,
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
        engine: Engine,
        options: PruneOptions,
    },
    EngineOp {
        verb: String,
        kind: Kind,
        socket: String,
    },
    Bulk {
        socket: String,
        api: String,
        actions: Vec<(String, String)>,
        verb: String,
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
            method,
            path,
            verb,
        } => action(&socket, &api, &method, &path, &verb),
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
            engine,
            options,
        } => prune_engine(&socket, &api, engine, &options),
        Request::EngineOp { verb, kind, socket } => engine_ops::run(&verb, kind, &socket),
        Request::Bulk {
            socket,
            api,
            actions,
            verb,
        } => bulk(&socket, &api, &actions, &verb),
    }
}

fn bulk(socket: &str, api: &str, actions: &[(String, String)], verb: &str) -> Outcome {
    let client = Client::new(socket).with_api(api);
    let total = actions.len();
    let mut done = 0;
    let mut first_error: Option<String> = None;
    for (method, path) in actions {
        match client.action(method, path) {
            Ok(()) => done += 1,
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.message());
                }
            }
        }
    }
    if let Some(error) = first_error {
        return bad(format!("{verb} {done} of {total}: {error}"));
    }
    ok(format!("{verb} {done} of {total} containers"))
}

fn action(socket: &str, api: &str, method: &str, path: &str, verb: &str) -> Outcome {
    let client = Client::new(socket).with_api(api);
    match client.action(method, path) {
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
    let Ok(body) = container_body(image, ports, volumes, env) else {
        return bad("could not build the container request".to_owned());
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

fn container_body(image: &str, ports: &str, volumes: &str, env: &str) -> Result<Vec<u8>, ()> {
    let mut exposed = serde_json::Map::new();
    let mut bindings = serde_json::Map::new();
    if !ports.trim().is_empty() {
        for entry in ports.split(',') {
            let entry = entry.trim();
            let Some((rest, container_spec)) = entry.rsplit_once(':') else {
                return Err(());
            };
            let (raw_ip, host) = match rest.rsplit_once(':') {
                None => ("", rest),
                Some((ip, host)) if !ip.is_empty() => (ip, host),
                Some(_) => return Err(()),
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
    }

    let binds: Vec<String> = if volumes.trim().is_empty() {
        Vec::new()
    } else {
        volumes
            .split(',')
            .map(|entry| entry.trim().to_owned())
            .filter(|entry| !entry.is_empty())
            .collect()
    };
    let env: Vec<String> = if env.trim().is_empty() {
        Vec::new()
    } else {
        env.split(',')
            .map(|entry| entry.trim().to_owned())
            .filter(|entry| !entry.is_empty())
            .collect()
    };

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
    serde_json::to_vec(&body).map_err(|_| ())
}

fn prune_engine(socket: &str, api: &str, engine: Engine, options: &PruneOptions) -> Outcome {
    let client = Client::new(socket).with_api(api);
    let plan = prune::plan(
        options.containers,
        options.images,
        options.volumes,
        options.networks,
        options.build_cache,
        engine,
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
        let body = container_body("nginx", "127.0.0.1:8080:80", "data:/data", "A=1").expect("body");
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
    fn an_invalid_port_is_refused_before_a_request() {
        assert!(container_body("nginx", "8080", "", "").is_err());
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
        let outcome = prune_engine(
            &engine.socket(),
            "v1.43",
            Engine::Docker,
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
    fn a_prune_outcome_names_the_reclaimed_bytes() {
        let options = PruneOptions {
            containers: false,
            images: false,
            volumes: false,
            networks: false,
            build_cache: false,
        };
        let outcome = prune_engine("missing.sock", "v1.43", Engine::Docker, &options);
        // No plan means no request and a zero-byte sentence.
        assert!(!outcome.bad);
        assert!(outcome.message.contains("0 B"));
    }
}
