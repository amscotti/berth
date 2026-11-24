//! The subset of the Docker Engine API berth reads. Field names are idiomatic
//! Rust; serde maps the engine's capitalized JSON keys.
//!
//! Every struct is `#[serde(default)]` and ignores unknown fields. A missing
//! field is a fact about the engine's version, not a parse failure.

use serde::{Deserialize, Serialize};

/// `/version`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Version {
    pub version: String,
    pub api_version: String,
    #[serde(rename = "MinAPIVersion")]
    pub min_api_version: String,
    pub os: String,
    pub arch: String,
    pub go_version: String,
    pub git_commit: String,
    pub kernel_version: String,
    pub build_time: String,
    pub components: Vec<Component>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Component {
    pub name: String,
    pub version: String,
    pub details: Option<serde_json::Value>,
}

impl Version {
    /// Podman's compat layer reports itself through the component list. The
    /// socket path cannot tell Podman's Docker-compatible API from Docker's.
    pub fn is_podman(&self) -> bool {
        self.components
            .iter()
            .any(|component| component.name.eq_ignore_ascii_case("Podman Engine"))
    }
}

/// `/info` — only the fields the rail, overview and discovery need.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Info {
    #[serde(rename = "ID")]
    pub id: String,
    pub name: String,
    pub server_version: String,
    pub operating_system: String,
    #[serde(rename = "OSType")]
    pub os_type: String,
    pub architecture: String,
    #[serde(rename = "DockerRootDir")]
    pub docker_root_dir: String,
    pub driver: String,
    pub logging_driver: String,
    pub cgroup_version: String,
    #[serde(rename = "NCPU")]
    pub ncpu: u32,
    #[serde(rename = "MemTotal")]
    pub mem_total: u64,
    pub containers: usize,
    pub containers_running: usize,
    pub containers_paused: usize,
    pub containers_stopped: usize,
    pub images: usize,
}

/// One published port binding.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Port {
    pub ip: String,
    pub private_port: u16,
    pub public_port: Option<u16>,
    #[serde(rename = "Type")]
    pub port_type: String,
}

/// A container list entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Container {
    #[serde(rename = "Id")]
    pub id: String,
    pub names: Vec<String>,
    pub image: String,
    #[serde(rename = "ImageID")]
    pub image_id: String,
    pub command: String,
    pub created: i64,
    pub ports: Vec<Port>,
    pub labels: std::collections::BTreeMap<String, String>,
    pub state: String,
    pub status: String,
    #[serde(rename = "HostConfig")]
    pub host_config: HostConfig,
    #[serde(rename = "NetworkSettings")]
    pub network_settings: NetworkSettings,
    pub mounts: Vec<Mount>,
}

/// `HostConfig`, as the list and the inspect document both carry it. The list
/// leaves most of it out, so every field defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct HostConfig {
    pub network_mode: String,
    pub port_bindings: Option<serde_json::Value>,
    pub restart_policy: Option<RestartPolicy>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct RestartPolicy {
    pub name: String,
    pub maximum_retry_count: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct NetworkSettings {
    pub networks: std::collections::BTreeMap<String, EndpointSettings>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct EndpointSettings {
    #[serde(rename = "NetworkID")]
    pub network_id: String,
    #[serde(rename = "IPAddress")]
    pub ip_address: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Mount {
    #[serde(rename = "Type")]
    pub mount_type: String,
    pub name: Option<String>,
    pub source: String,
    pub destination: String,
    pub mode: String,
    #[serde(rename = "RW")]
    pub rw: bool,
}

/// What can be done to a container in its current state. An action the engine
/// will refuse should not be on screen: it turns a clear state into an error
/// toast.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Actions {
    pub start: bool,
    pub stop: bool,
    pub restart: bool,
    pub pause: bool,
    pub resume: bool,
    /// A shell can be opened only into a container that is running and not
    /// paused: `exec` into a paused container hangs.
    pub terminal: bool,
}

impl Actions {
    pub fn for_state(state: &str) -> Self {
        match state {
            "running" => Self {
                stop: true,
                restart: true,
                pause: true,
                terminal: true,
                ..Default::default()
            },
            "paused" => Self {
                stop: true,
                resume: true,
                ..Default::default()
            },
            // Restarting can only be interrupted.
            "restarting" => Self {
                stop: true,
                ..Default::default()
            },
            "created" | "exited" | "dead" => Self {
                start: true,
                ..Default::default()
            },
            // `removing`, and anything an engine adds later: nothing.
            _ => Self::default(),
        }
    }
}

impl Container {
    /// The first name, without Docker's leading slash.
    pub fn name(&self) -> &str {
        self.names
            .first()
            .map(|name| name.trim_start_matches('/'))
            .unwrap_or(&self.id)
    }

    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }

    pub fn project(&self) -> Option<&str> {
        self.label("com.docker.compose.project")
            .or_else(|| self.label("io.podman.compose.project"))
            .or_else(|| self.label("io.kubernetes.pod.name"))
    }

    pub fn service(&self) -> Option<&str> {
        self.label("com.docker.compose.service")
            .or_else(|| self.label("io.podman.compose.service"))
    }

    pub fn is_running(&self) -> bool {
        self.state == "running"
    }

    pub fn is_paused(&self) -> bool {
        self.state == "paused"
    }

    pub fn is_up(&self) -> bool {
        self.is_running() || self.is_paused() || self.state == "restarting"
    }

    /// What the engine will accept for this container right now.
    pub fn actions(&self) -> Actions {
        Actions::for_state(&self.state)
    }

    /// The healthcheck verdict, when there is one. It rides at the end of the
    /// human `Status` string; `State` never carries it.
    pub fn health(&self) -> Option<&'static str> {
        if self.status.ends_with("(healthy)") {
            Some("healthy")
        } else if self.status.ends_with("(unhealthy)") {
            Some("unhealthy")
        } else if self.status.ends_with("(health: starting)") {
            Some("starting")
        } else {
            None
        }
    }

    /// Host-side TCP bindings, deduplicated by public port and sorted.
    pub fn published_ports(&self) -> Vec<HostBinding> {
        let mut out: Vec<HostBinding> = Vec::new();
        for port in &self.ports {
            let Some(public) = port.public_port else {
                continue;
            };
            if matches!(port.port_type.as_str(), "udp" | "sctp") {
                continue;
            }
            if out.iter().any(|binding| binding.port == public) {
                continue;
            }
            let ip = if matches!(port.ip.as_str(), "0.0.0.0" | "::") {
                String::new()
            } else {
                port.ip.clone()
            };
            out.push(HostBinding {
                port: public,
                private_port: port.private_port,
                ip,
            });
        }
        out.sort_by_key(|binding| binding.port);
        out
    }

    /// Published ports as `8080→80`; at most `max`, then `+n`.
    pub fn ports_text(&self, max: usize) -> String {
        let ports = self.published_ports();
        let mut out = String::new();
        for (index, binding) in ports.iter().take(max).enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{}→{}", binding.port, binding.private_port));
        }
        if ports.len() > max {
            out.push_str(&format!(" +{}", ports.len() - max));
        }
        out
    }

    /// True if this container was created from `image`.
    pub fn uses_image(&self, image: &Image) -> bool {
        if !self.image_id.is_empty() && self.image_id == image.id {
            return true;
        }
        if !self.image.is_empty() && self.image == image.tag() {
            return true;
        }
        image
            .repo_tags
            .as_ref()
            .is_some_and(|tags| tags.iter().any(|tag| tag == &self.image))
    }

    /// The mount entry referencing `volume_name`, if any.
    pub fn mount_for_volume(&self, volume_name: &str) -> Option<&Mount> {
        self.mounts.iter().find(|mount| {
            mount.name.as_deref() == Some(volume_name)
                || mount.source == volume_name
                || mount.source.contains(&format!("/volumes/{volume_name}/"))
        })
    }

    /// The IPv4 address (or empty string if attached without an IP) on `network`.
    pub fn network_ip(&self, network: &Network) -> Option<&str> {
        if let Some(endpoint) = self.network_settings.networks.get(&network.name) {
            return Some(endpoint.ip_address.as_str());
        }
        if !network.id.is_empty()
            && let Some(endpoint) = self
                .network_settings
                .networks
                .values()
                .find(|ep| ep.network_id == network.id)
        {
            return Some(endpoint.ip_address.as_str());
        }
        if network.name == "bridge" && self.host_config.network_mode == "default" {
            return Some("");
        }
        None
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostBinding {
    pub port: u16,
    pub private_port: u16,
    pub ip: String,
}

/// `/images/json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Image {
    #[serde(rename = "Id")]
    pub id: String,
    pub parent_id: String,
    pub repo_tags: Option<Vec<String>>,
    pub repo_digests: Option<Vec<String>>,
    pub created: i64,
    pub size: u64,
    pub shared_size: i64,
    pub containers: i64,
}

impl Image {
    pub fn tag(&self) -> &str {
        self.repo_tags
            .as_ref()
            .and_then(|tags| tags.first())
            .map(String::as_str)
            .unwrap_or("<none>:<none>")
    }

    pub fn dangling(&self) -> bool {
        self.repo_tags
            .as_ref()
            .is_none_or(|tags| tags.is_empty() || tags[0].starts_with("<none>"))
    }

    pub fn label(&self) -> &str {
        if self.dangling() {
            "untagged"
        } else {
            self.tag()
        }
    }
}

/// `/volumes`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: Option<String>,
    pub scope: String,
    pub usage_data: Option<UsageData>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct UsageData {
    pub size: i64,
    #[serde(rename = "RefCount")]
    pub ref_count: i64,
}

/// `/networks`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Network {
    pub name: String,
    #[serde(rename = "Id")]
    pub id: String,
    pub created: String,
    pub scope: String,
    pub driver: String,
    #[serde(rename = "EnableIPv6")]
    pub enable_ipv6: bool,
    pub internal: bool,
    pub attachable: bool,
    pub containers: Option<serde_json::Value>,
    pub labels: Option<serde_json::Value>,
}

impl Network {
    pub fn builtin(&self) -> bool {
        matches!(self.name.as_str(), "bridge" | "host" | "none")
    }

    pub fn attached(&self) -> usize {
        self.containers
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .map_or(0, serde_json::Map::len)
    }
}

/// One container's `/stats?stream=false` answer. Only the fields berth uses;
/// the string-keyed maps are left as values and reduced by `stats`.
/// A list endpoint wrapped in an object (`/volumes`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ListResponse<T> {
    pub volumes: Option<Vec<T>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub read: String,
    pub preread: String,
    pub pids_stats: PidsStats,
    pub cpu_stats: CpuStats,
    pub precpu_stats: CpuStats,
    pub memory_stats: MemoryStats,
    pub networks: std::collections::BTreeMap<String, NetworkStats>,
    pub blkio_stats: BlkioStats,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PidsStats {
    pub current: Option<u32>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CpuStats {
    pub cpu_usage: CpuUsage,
    pub system_cpu_usage: Option<u64>,
    pub online_cpus: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CpuUsage {
    pub total_usage: u64,
    pub percpu_usage: Vec<u64>,
    pub usage_in_kernelmode: u64,
    pub usage_in_usermode: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryStats {
    pub usage: Option<u64>,
    pub limit: Option<u64>,
    pub stats: std::collections::BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlkioStats {
    pub io_service_bytes_recursive: Option<Vec<BlkioEntry>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlkioEntry {
    pub major: u64,
    pub minor: u64,
    pub op: String,
    pub value: u64,
}

/// `/containers/{id}/top`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ContainerTop {
    pub titles: Vec<String>,
    pub processes: Vec<Vec<String>>,
}

/// `/containers/{id}/changes`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct FsChange {
    pub path: String,
    pub kind: u8,
}

/// `/images/{id}/history`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ImageLayer {
    #[serde(rename = "Id")]
    pub id: String,
    pub created: i64,
    pub created_by: String,
    pub size: u64,
    pub comment: String,
}

#[cfg(test)]
mod tests;
