//! Finding the engines a machine actually has.
//!
//! Nothing hardcodes one socket as "the" engine. Docker Desktop, dockerd,
//! Colima, Lima, Podman machine and rootless Podman leave sockets in different
//! places, and several can be alive at once; every existing candidate is probed
//! and the reachable ones are what the app offers.

use std::path::{Path, PathBuf};

use crate::client::Client;
use crate::dto::{Info, Version};
use crate::transport::PROBE_TIMEOUT;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    DockerDesktop,
    OrbStack,
    Rancher,
    #[default]
    Dockerd,
    Colima,
    Lima,
    PodmanMachine,
    Podman,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::DockerDesktop => "Docker Desktop",
            Kind::OrbStack => "OrbStack",
            Kind::Rancher => "Rancher Desktop",
            Kind::Dockerd => "Dockerd",
            Kind::Colima => "Colima",
            Kind::Lima => "Lima",
            Kind::PodmanMachine => "Podman machine",
            Kind::Podman => "Podman",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    #[default]
    Docker,
    Podman,
}

#[derive(Clone, Debug, Default)]
pub struct Runtime {
    pub name: String,
    pub kind: Kind,
    pub dialect: Dialect,
    pub socket: String,
    pub api: String,
    pub reachable: bool,
    pub error_text: String,
    pub version: Version,
    pub info: Info,
    /// `/info` answered, so its counts are real rather than defaults.
    pub info_read: bool,
}

/// The socket the user chose last time comes first, then environment, then the
/// known app layouts, then the fixed system paths.
pub fn candidates(
    preferred: &str,
    home: &str,
    xdg_data_home: &str,
    xdg_runtime: &str,
    uid: u32,
) -> Vec<String> {
    let mut list = Vec::new();
    if !preferred.is_empty() {
        add_if_exists(&mut list, preferred);
    }
    if let Ok(host) = std::env::var("DOCKER_HOST")
        && let Some(path) = host.strip_prefix("unix://")
    {
        add_if_exists(&mut list, path);
    }

    if !home.is_empty() {
        for suffix in [
            "/.docker/run/docker.sock",
            "/.docker/desktop/docker.sock",
            "/.orbstack/run/docker.sock",
            "/.rd/docker.sock",
            "/Library/Containers/com.docker.docker/Data/docker.sock",
            "/.colima/default/docker.sock",
            "/.colima/docker.sock",
            "/.lima/default/sock/docker.sock",
        ] {
            add_if_exists(&mut list, &format!("{home}{suffix}"));
        }
        add_podman_runtime_sockets(&mut list, home);
        add_podman_machines(&mut list, home, xdg_data_home);
        add_lima_instances(&mut list, home);
    }

    let runtime = if xdg_runtime.is_empty() {
        format!("/run/user/{uid}")
    } else {
        xdg_runtime.to_owned()
    };
    add_if_exists(&mut list, &format!("{runtime}/podman/podman.sock"));

    for path in [
        "/var/run/docker.sock",
        "/run/podman/podman.sock",
        "/var/run/podman/podman.sock",
    ] {
        add_if_exists(&mut list, path);
    }
    list
}

fn add_if_exists(list: &mut Vec<String>, path: &str) {
    if path.is_empty() || !Path::new(path).exists() {
        return;
    }
    let canonical = std::fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .into_owned();
    add(list, &canonical);
}

fn add(list: &mut Vec<String>, path: &str) {
    if !path.is_empty() && !list.iter().any(|existing| same_socket(existing, path)) {
        list.push(path.to_owned());
    }
}

/// Resolve symlinks so one engine is not listed twice under two aliases.
pub fn same_socket(a: &str, b: &str) -> bool {
    let ra = std::fs::canonicalize(a).unwrap_or_else(|_| PathBuf::from(a));
    let rb = std::fs::canonicalize(b).unwrap_or_else(|_| PathBuf::from(b));
    ra == rb
}

fn add_podman_runtime_sockets(list: &mut Vec<String>, home: &str) {
    let root = format!("{home}/.local/share/containers/podman/machine");
    scan_podman_api_sockets(list, Path::new(&root));
}

fn scan_podman_api_sockets(list: &mut Vec<String>, dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                for sub in sub_entries.flatten() {
                    let sub_path = sub.path();
                    if sub_path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().ends_with("-api.sock"))
                    {
                        add_if_exists(list, &sub_path.to_string_lossy());
                    }
                }
            }
        } else if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("-api.sock"))
        {
            add_if_exists(list, &path.to_string_lossy());
        }
    }
}

fn add_podman_machines(list: &mut Vec<String>, home: &str, xdg_data_home: &str) {
    let root = if xdg_data_home.is_empty() {
        format!("{home}/.local/share/containers/podman/machine")
    } else {
        format!("{xdg_data_home}/containers/podman/machine")
    };
    // Podman 5 keeps named runtime sockets under the machine root or a
    // provider subdirectory (applehv, qemu, libkrun).
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let sock = path.join("podman.sock");
            if sock.exists() {
                add_if_exists(list, &sock.to_string_lossy());
            }
            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                for sub in sub_entries.flatten() {
                    let sub_path = sub.path();
                    if sub_path.is_dir() {
                        let sub_sock = sub_path.join("podman.sock");
                        if sub_sock.exists() {
                            add_if_exists(list, &sub_sock.to_string_lossy());
                        }
                    }
                }
            }
        } else if path.file_name().is_some_and(|name| name == "podman.sock") {
            add_if_exists(list, &path.to_string_lossy());
        }
    }
}

fn add_lima_instances(list: &mut Vec<String>, home: &str) {
    let root = format!("{home}/.lima");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "_config" || !entry.path().is_dir() {
            continue;
        }
        let sock = entry.path().join("sock/docker.sock");
        if sock.exists() {
            add_if_exists(list, &sock.to_string_lossy());
        }
    }
}

/// Name the engine from its socket path. The socket cannot tell Podman's
/// Docker-compatible API from Docker's; `/version` components do that later.
pub fn kind_of(path: &str) -> Kind {
    if path.contains(".docker/run")
        || path.contains(".docker/desktop")
        || path.contains("com.docker.docker")
    {
        Kind::DockerDesktop
    } else if path.contains(".orbstack") {
        Kind::OrbStack
    } else if path.contains("/.rd/") {
        Kind::Rancher
    } else if path.contains(".colima") {
        Kind::Colima
    } else if path.contains(".lima") {
        Kind::Lima
    } else if is_podman_machine_socket(path) {
        Kind::PodmanMachine
    } else if path.contains("podman") {
        Kind::Podman
    } else {
        Kind::Dockerd
    }
}

pub fn is_podman_machine_socket(path: &str) -> bool {
    path.contains("containers/podman/machine")
        || path
            .rsplit('/')
            .next()
            .is_some_and(|base| base.ends_with("-api.sock"))
}

/// Podman 5 names its socket `<machine>-api.sock` or places `podman.sock` inside
/// a `<machine>/` subdirectory; the unnamed 4.x layout sits directly in `machine/`.
pub fn podman_machine_name(path: &str) -> Option<&str> {
    let mut parts = path.rsplit('/');
    let base = parts.next()?;
    if let Some(name) = base
        .strip_suffix("-api.sock")
        .filter(|name| !name.is_empty())
    {
        return Some(name);
    }
    if base == "podman.sock" {
        let parent = parts.next()?;
        if !parent.is_empty()
            && !matches!(
                parent,
                "machine" | "podman" | "applehv" | "qemu" | "libkrun" | "wsl" | "hyperv"
            )
        {
            return Some(parent);
        }
    }
    None
}

pub fn is_lima_instance(path: &str) -> bool {
    path.contains("/.lima/") && !path.contains("/.lima/_config/")
}

fn display_name(kind: Kind, path: &str) -> String {
    match kind {
        Kind::Lima => {
            let name = path
                .split("/.lima/")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .unwrap_or("default");
            format!("Lima · {name}")
        }
        Kind::PodmanMachine => match podman_machine_name(path) {
            Some(name) => format!("Podman machine · {name}"),
            None => "Podman machine".to_owned(),
        },
        _ => kind.label().to_owned(),
    }
}

/// Probe one socket. A failure is a runtime with `reachable = false` and the
/// reason, not an error that hides the other candidates.
pub fn probe(socket: &str) -> Runtime {
    let kind = kind_of(socket);
    let mut runtime = Runtime {
        name: display_name(kind, socket),
        kind,
        socket: socket.to_owned(),
        ..Default::default()
    };
    let mut client = Client::new(socket).with_timeout(PROBE_TIMEOUT);
    let version = match client.version() {
        Ok(version) => version,
        Err(error) => {
            runtime.error_text = error.message();
            return runtime;
        }
    };
    client.adopt_api(&version);
    runtime.dialect = if version.is_podman() || matches!(kind, Kind::Podman | Kind::PodmanMachine) {
        Dialect::Podman
    } else {
        Dialect::Docker
    };
    runtime.api = client.api().to_owned();
    let info = client.info().unwrap_or_default();
    runtime.info_read = !info.id.is_empty() || info.ncpu > 0 || !info.server_version.is_empty();
    runtime.version = version;
    runtime.info = info;
    runtime.reachable = true;
    runtime
}

/// Probe every candidate. The preferred socket is first. Sequential is fine
/// for a handful of paths; the UI runs this on a worker.
pub fn scan(preferred: &str) -> Vec<Runtime> {
    let home = std::env::var("HOME").unwrap_or_default();
    let xdg_data = std::env::var("XDG_DATA_HOME").unwrap_or_default();
    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    let uid = unsafe { libc::getuid() };
    candidates(preferred, &home, &xdg_data, &xdg_runtime, uid)
        .into_iter()
        .map(|socket| probe(&socket))
        .collect()
}

/// Engine identity for a snapshot. Docker uses the daemon id; Podman's compat
/// `/info` returns a fresh id per request, so Podman uses the socket path.
pub fn identity(runtime: &Runtime) -> String {
    match runtime.dialect {
        Dialect::Docker if !runtime.info.id.is_empty() => runtime.info.id.clone(),
        _ => runtime.socket.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_socket() -> PathBuf {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("berth-discovery-{}-{id}.sock", std::process::id()))
    }

    #[test]
    fn socket_paths_name_the_engine() {
        assert_eq!(
            kind_of("/Users/me/.orbstack/run/docker.sock"),
            Kind::OrbStack
        );
        assert_eq!(
            kind_of("/Users/me/.colima/default/docker.sock"),
            Kind::Colima
        );
        assert_eq!(
            kind_of("/Users/me/.lima/docker/sock/docker.sock"),
            Kind::Lima
        );
        assert_eq!(kind_of("/var/run/docker.sock"), Kind::Dockerd);
        assert_eq!(
            kind_of("/Users/me/.local/share/containers/podman/machine/podman.sock"),
            Kind::PodmanMachine
        );
    }

    #[test]
    fn podman_machine_names_are_extracted() {
        assert_eq!(
            podman_machine_name("/run/user/501/containers/podman/machine/berth-api.sock"),
            Some("berth")
        );
        assert_eq!(
            podman_machine_name(
                "/Users/me/.local/share/containers/podman/machine/dev-vm/podman.sock"
            ),
            Some("dev-vm")
        );
        assert_eq!(
            podman_machine_name("/Users/me/.local/share/containers/podman/machine/podman.sock"),
            None
        );
        assert_eq!(podman_machine_name("/var/run/docker.sock"), None);
        assert!(is_lima_instance("/home/me/.lima/docker/sock/docker.sock"));
        assert!(!is_lima_instance("/home/me/.lima/_config/sock/docker.sock"));
    }

    #[test]
    fn candidates_keep_the_preferred_socket_first_and_dedupe() {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("berth-cand-{}-{id}", std::process::id()));
        std::fs::create_dir_all(dir.join(".docker/run")).expect("dir");
        std::fs::create_dir_all(dir.join(".lima/docker/sock")).expect("dir");
        let preferred = dir.join(".docker/run/docker.sock");
        let lima = dir.join(".lima/docker/sock/docker.sock");
        for path in [&preferred, &lima] {
            let _ = std::os::unix::net::UnixListener::bind(path).expect("bind");
        }
        let list = candidates(
            &preferred.to_string_lossy(),
            &dir.to_string_lossy(),
            "",
            "",
            501,
        );
        assert_eq!(
            list[0],
            std::fs::canonicalize(&preferred).unwrap().to_string_lossy()
        );
        assert!(list.iter().any(|path| path.contains(".lima/docker")));
        // The host may also have /var/run/docker.sock; only the sockets under
        // this temporary HOME prove candidate discovery and deduplication.
        let root = std::fs::canonicalize(&dir).expect("canonical home");
        let root = root.to_string_lossy();
        assert_eq!(
            list.iter()
                .filter(|path| path.starts_with(root.as_ref()))
                .count(),
            2
        );
    }

    #[test]
    fn a_missing_socket_is_reported_not_dropped() {
        let path = temp_socket();
        let runtime = probe(&path.to_string_lossy());
        assert!(!runtime.reachable);
        assert!(!runtime.error_text.is_empty());
    }

    #[test]
    fn probe_reads_version_and_info() {
        let path = temp_socket();
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            for body in [
                br#"{"Version":"29.1.3","ApiVersion":"1.56","Components":[]}"#.as_slice(),
                br#"{"ID":"daemon-1","NCPU":4,"MemTotal":1024,"ContainersRunning":1}"#.as_slice(),
            ] {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut request = [0u8; 4096];
                let _ = std::io::Read::read(&mut stream, &mut request);
                let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(body);
            }
        });
        let runtime = probe(&path.to_string_lossy());
        assert!(runtime.reachable);
        assert_eq!(runtime.api, "v1.56");
        assert_eq!(runtime.version.version, "29.1.3");
        assert_eq!(runtime.info.ncpu, 4);
        assert_eq!(identity(&runtime), "daemon-1");
    }
}
