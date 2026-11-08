//! The immutable snapshot the window paints and the small amount of engine
//! state around it. A refresh builds a complete `Snapshot`; commit swaps it in
//! one step. The previous snapshot stays alive through the `Arc` while the
//! current frame may still reference it.

use std::sync::Arc;

use berth_core::format;
use berth_core::history::History;
use berth_engine::client::Client;
use berth_engine::df::Df;
use berth_engine::discovery::{Dialect, Kind, Runtime};
use berth_engine::dto::{Container, Image, Info, Network, Port, UsageData, Version, Volume};
use berth_engine::events::{Actor, Event};
use berth_engine::network_counts::Counts;
use berth_engine::projects::Summary;
use berth_engine::stats::{Sample, aggregate_cpu_percent, all_sampled, memory_used};

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub containers: Vec<Container>,
    pub stats: Vec<Sample>,
    pub stats_complete: bool,
    pub images: Vec<Image>,
    pub volumes: Vec<Volume>,
    pub networks: Vec<Network>,
    pub projects: Vec<Summary>,
    pub network_counts: Counts,
    pub events: Vec<Event>,
    pub events_error: bool,
    pub error: String,
    pub last_update: i64,
    pub df: Option<Df>,
    pub df_at: i64,
    pub df_error: String,
    pub host: HostSample,
}

impl Snapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn stat(&self, index: usize) -> Sample {
        self.stats.get(index).copied().unwrap_or_default()
    }

    pub fn running(&self) -> usize {
        self.containers
            .iter()
            .filter(|container| container.is_up())
            .count()
    }

    pub fn container_memory(&self) -> u64 {
        memory_used(&self.containers, &self.stats)
    }

    pub fn container_cpu(&self, cores: f32) -> Option<f32> {
        aggregate_cpu_percent(&self.containers, &self.stats, cores)
    }

    pub fn stats_complete(&self) -> bool {
        all_sampled(&self.containers, &self.stats)
    }

    pub fn store_bytes(&self) -> Option<i64> {
        let df = self.df.as_ref()?;
        let total = df.stores().total();
        (total > 0).then_some(total)
    }

    pub fn df_reclaimable(&self) -> Option<i64> {
        self.df.as_ref().map(Df::reclaimable)
    }

    pub fn selected_container(&self, id: &str) -> Option<&Container> {
        self.containers.iter().find(|container| container.id == id)
    }

    pub fn selected_image(&self, id: &str) -> Option<&Image> {
        self.images.iter().find(|image| image.id == id)
    }

    pub fn selected_volume(&self, name: &str) -> Option<&Volume> {
        self.volumes.iter().find(|volume| volume.name == name)
    }

    pub fn selected_network(&self, id: &str) -> Option<&Network> {
        self.networks
            .iter()
            .find(|network| network.id == id || network.name == id)
    }
}

/// The machine's own figures, sampled alongside the engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostSample {
    pub mem_total: u64,
    pub mem_used: u64,
    pub cpu_percent: f32,
    pub cpu_known: bool,
    pub engine_rss: u64,
}

#[derive(Clone, Debug, Default)]
pub struct EngineState {
    pub runtimes: Vec<Runtime>,
    pub active: usize,
    pub socket: Option<String>,
    pub api: Option<String>,
    pub dialect: Dialect,
    pub client: Option<Client>,
    pub generation: u64,
    pub identity: Option<String>,
    pub preferred_socket: Option<String>,
    pub loading: bool,
    pub error: String,
    pub last_update: i64,
    pub history: History,
    pub events_since: i64,
    pub df: Option<Df>,
    pub df_at: i64,
    pub df_error: String,
}

impl EngineState {
    pub fn active_runtime(&self) -> Option<&Runtime> {
        self.runtimes.get(self.active)
    }

    pub fn reachable(&self) -> usize {
        self.runtimes
            .iter()
            .filter(|runtime| runtime.reachable)
            .count()
    }

    pub fn engine_label(&self) -> String {
        self.active_runtime()
            .map(|runtime| runtime.name.clone())
            .unwrap_or_else(|| "no engine found".to_owned())
    }

    pub fn df_due(&self) -> bool {
        self.df_at == 0 || format::now() - self.df_at >= 30
    }

    pub fn install(&mut self, runtime: &Runtime) {
        self.generation = self.generation.wrapping_add(1);
        self.socket = Some(runtime.socket.clone());
        self.api = Some(runtime.api.clone());
        self.dialect = runtime.dialect;
        self.identity = Some(berth_engine::discovery::identity(runtime));
        self.client = Some(
            Client::new(runtime.socket.clone())
                .with_api(runtime.api.clone())
                .with_dialect(runtime.dialect),
        );
        self.error.clear();
        self.last_update = 0;
        self.events_since = 0;
        self.df = None;
        self.df_at = 0;
        self.df_error.clear();
        self.history = History::default();
    }
}

/// A snapshot for headless screenshots and layout tests. It is never used by
/// the running app; `BERTH_FIXTURE`/the example opts in explicitly.
pub fn fixture_snapshot() -> Snapshot {
    use berth_engine::stats::Sample;

    let containers = vec![
        Container {
            id: "demo-1".into(),
            names: vec!["/berth-demo-web".into()],
            image: "nginx:alpine".into(),
            image_id: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                .into(),
            command: "nginx -g 'daemon off;'".into(),
            state: "running".into(),
            status: "Up 18 hours".into(),
            labels: [
                ("com.docker.compose.project".to_owned(), "shop".to_owned()),
                ("com.docker.compose.service".to_owned(), "web".to_owned()),
            ]
            .into_iter()
            .collect(),
            ports: vec![
                Port {
                    ip: "0.0.0.0".into(),
                    private_port: 80,
                    public_port: Some(8080),
                    port_type: "tcp".into(),
                },
                Port {
                    ip: "::".into(),
                    private_port: 80,
                    public_port: Some(8080),
                    port_type: "tcp".into(),
                },
            ],
            mounts: vec![berth_engine::dto::Mount {
                mount_type: "volume".into(),
                name: Some("shop-data".into()),
                source: "shop-data".into(),
                destination: "/var/www/html".into(),
                mode: "rw".into(),
                rw: true,
            }],
            network_settings: berth_engine::dto::NetworkSettings {
                networks: [(
                    "shop_default".to_owned(),
                    berth_engine::dto::EndpointSettings {
                        network_id: "net-shop_default".into(),
                        ip_address: "172.18.0.2".into(),
                    },
                )]
                .into_iter()
                .collect(),
            },
            ..Default::default()
        },
        Container {
            id: "demo-2".into(),
            names: vec!["/burner".into()],
            image: "alpine:3.22".into(),
            image_id: "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                .into(),
            command: "/bin/sh -c 'while true; do :; done'".into(),
            state: "running".into(),
            status: "Up 1 hour".into(),
            labels: [
                ("com.docker.compose.project".to_owned(), "shop".to_owned()),
                ("com.docker.compose.service".to_owned(), "api".to_owned()),
            ]
            .into_iter()
            .collect(),
            network_settings: berth_engine::dto::NetworkSettings {
                networks: [(
                    "shop_default".to_owned(),
                    berth_engine::dto::EndpointSettings {
                        network_id: "net-shop_default".into(),
                        ip_address: "172.18.0.3".into(),
                    },
                )]
                .into_iter()
                .collect(),
            },
            ..Default::default()
        },
    ];
    let stats = vec![
        Sample {
            cpu: 0.0,
            cpu_known: true,
            mem: 4_928_307,
            mem_known: true,
            limit: 4_080_218_112,
            net_rx: 21_708,
            net_tx: 3_584,
            blk_read: 4_096,
            blk_write: 12_288,
            blk_known: true,
            pids: Some(5),
            sampled: true,
        },
        Sample {
            cpu: 99.7,
            cpu_known: true,
            mem: 364_544,
            mem_known: true,
            limit: 4_080_218_112,
            net_rx: 2_765,
            net_tx: 126,
            blk_read: 0,
            blk_write: 0,
            blk_known: true,
            pids: Some(1),
            sampled: true,
        },
    ];
    let df = Df {
        image_usage: Some(berth_engine::df::Usage {
            total_size: 103 * 1024 * 1024,
            reclaimable: 13 * 1024 * 1024,
            ..Default::default()
        }),
        ..Default::default()
    };
    let images = vec![
        Image {
            id: "sha256:1111111111111111111111111111111111111111111111111111111111111111".into(),
            repo_tags: Some(vec!["nginx:alpine".into()]),
            created: format::now() - 86_400 * 12,
            size: 48 * 1024 * 1024,
            ..Default::default()
        },
        Image {
            id: "sha256:2222222222222222222222222222222222222222222222222222222222222222".into(),
            repo_tags: Some(vec!["alpine:3.22".into()]),
            created: format::now() - 86_400 * 30,
            size: 8 * 1024 * 1024,
            ..Default::default()
        },
    ];
    let volumes = vec![Volume {
        name: "shop-data".into(),
        driver: "local".into(),
        mountpoint: "/var/lib/docker/volumes/shop-data/_data".into(),
        scope: "local".into(),
        usage_data: Some(UsageData {
            size: 32 * 1024 * 1024,
            ref_count: 1,
        }),
        ..Default::default()
    }];
    let networks = ["bridge", "host", "none", "shop_default"]
        .into_iter()
        .map(|name| Network {
            name: name.into(),
            id: format!("net-{name}"),
            driver: if name == "shop_default" {
                "bridge"
            } else {
                name
            }
            .into(),
            scope: "local".into(),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let events = vec![
        Event {
            kind: "container".into(),
            action: "start".into(),
            time: format::now() - 40,
            time_nano: (format::now() - 40) * 1_000_000_000,
            actor: Actor {
                id: "demo-1".into(),
                attributes: [
                    ("name".to_owned(), "berth-demo-web".to_owned()),
                    ("image".to_owned(), "nginx:alpine".to_owned()),
                ]
                .into_iter()
                .collect(),
            },
            ..Default::default()
        },
        Event {
            kind: "container".into(),
            action: "die".into(),
            time: format::now() - 80,
            time_nano: (format::now() - 80) * 1_000_000_000,
            actor: Actor {
                id: "demo-1".into(),
                attributes: [
                    ("name".to_owned(), "berth-demo-web".to_owned()),
                    ("image".to_owned(), "nginx:alpine".to_owned()),
                    ("exitCode".to_owned(), "0".to_owned()),
                ]
                .into_iter()
                .collect(),
            },
            ..Default::default()
        },
    ];
    let projects = berth_engine::projects::summaries(&containers);
    let network_counts = Counts::from_containers(&containers);
    Snapshot {
        containers,
        stats,
        stats_complete: true,
        images,
        volumes,
        networks,
        projects,
        network_counts,
        events,
        events_error: false,
        error: String::new(),
        last_update: format::now(),
        df: Some(df),
        df_at: format::now(),
        df_error: String::new(),
        host: HostSample {
            mem_total: 64 * 1024 * 1024 * 1024,
            mem_used: 43 * 1024 * 1024 * 1024 + 400 * 1024 * 1024,
            cpu_percent: 48.9,
            cpu_known: true,
            engine_rss: 44 * 1024 * 1024,
        },
    }
}

/// A larger fixture for the performance pass: the same two shapes repeated.
pub fn fixture_snapshot_with(count: usize) -> Snapshot {
    let base = fixture_snapshot();
    let mut snapshot = base.clone();
    snapshot.containers.clear();
    snapshot.stats.clear();
    for index in 0..count {
        let mut container = base.containers[index % base.containers.len()].clone();
        container.id = format!("perf-{index}");
        container.names = vec![format!("/perf-{index}")];
        container.labels.remove("com.docker.compose.project");
        snapshot.containers.push(container);
        snapshot.stats.push(base.stats[index % base.stats.len()]);
    }
    snapshot.stats_complete = true;
    snapshot.projects.clear();
    snapshot
}

pub fn fixture_runtime() -> Runtime {
    Runtime {
        name: "Lima · lima-berth-docker".into(),
        kind: Kind::Lima,
        dialect: Dialect::Docker,
        socket: "/Users/ascotti/.lima/berth-docker/sock/docker.sock".into(),
        api: "v1.56".into(),
        reachable: true,
        error_text: String::new(),
        version: Version {
            version: "29.9.2".into(),
            api_version: "1.56".into(),
            ..Default::default()
        },
        info: Info {
            id: "fixture-daemon".into(),
            ncpu: 4,
            mem_total: 3 * 1024 * 1024 * 1024 + 800 * 1024 * 1024,
            containers: 2,
            containers_running: 2,
            driver: "overlayfs".into(),
            operating_system: "Ubuntu 26.04 LTS".into(),
            os_type: "linux".into(),
            architecture: "aarch64".into(),
            docker_root_dir: "/var/lib/docker".into(),
            cgroup_version: "2".into(),
            ..Default::default()
        },
        info_read: true,
    }
}

pub type SharedSnapshot = Arc<Snapshot>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hundred_container_fixture_is_complete() {
        let snapshot = fixture_snapshot_with(100);
        assert_eq!(snapshot.containers.len(), 100);
        assert_eq!(snapshot.stats.len(), 100);
        assert_eq!(snapshot.running(), 100);
        assert!(snapshot.stats_complete());
        assert_eq!(snapshot.containers[99].name(), "perf-99");
    }
}
