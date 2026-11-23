//! The fixture: a snapshot, an engine, and hooks that put the app into a
//! state, for tests and for the headless screenshots.
//!
//! None of this is in a release build. It compiles for tests, and behind
//! `test-driver`, the feature the screenshot and performance examples are
//! built with.

use berth_core::format;
use berth_engine::df::{Df, Usage};
use berth_engine::discovery::{Dialect, Kind, Runtime};
use berth_engine::dto::{Container, Image, Info, Network, Port, UsageData, Version, Volume};
use berth_engine::events::{Actor, Event};
use berth_engine::network_counts::Counts;
use berth_engine::stats::Sample;
use berth_ui::theme::Tokens;
use gpui::Context;

use crate::app::{BerthApp, CreateKind, InspectorTab, SearchKind, Tab};
use crate::settings;
use crate::state::{HostSample, Snapshot};

impl BerthApp {
    /// An app on the fixture, as the screenshots and the tests start one. The
    /// running app always starts empty and waits for a real snapshot.
    pub fn new_fixture(tokens: Tokens, cx: &mut Context<Self>) -> Self {
        let mut app = Self::with_settings(
            tokens,
            fixture_snapshot(),
            settings::Data::default(),
            None,
            cx,
        );
        app.host = fixture_host();
        app.engine.runtimes = vec![fixture_runtime()];
        app.engine.identity = Some(berth_engine::discovery::identity(&app.engine.runtimes[0]));
        app.engine.last_update = format::now();
        app.engine.loading = false;
        app.engine.df = app.snapshot.df.clone();
        app.engine.df_at = app.snapshot.df_at;
        for step in 0..24 {
            app.engine.history.push(berth_core::history::Sample {
                cpu: 4.0 + (step as f32 * 0.7) % 9.0,
                cpu_known: true,
                mem: (4.2 + (step as f32 * 0.06) % 0.8) * 1024.0 * 1024.0,
                mem_known: true,
                running: if step > 12 { 2.0 } else { 1.0 },
                disk: (103.0 + step as f32 * 0.05) * 1024.0 * 1024.0,
                host_cpu: 42.0 + step as f32,
                host_mem: 43.0 * 1024.0 * 1024.0 * 1024.0,
            });
        }
        app
    }

    /// An app on a snapshot of the caller's, for the performance pass.
    pub fn with_fixture(tokens: Tokens, snapshot: Snapshot, cx: &mut Context<Self>) -> Self {
        Self::with_settings(tokens, snapshot, settings::Data::default(), None, cx)
    }

    /// Show a page, as the screenshot pass does to reach each of them.
    pub fn show_tab(&mut self, tab: Tab) {
        self.view.tab = tab;
    }

    // --- review hooks: put the app into a state, without a pointer ---------

    /// The cleanup sheet.
    pub fn show_prune(&mut self, cx: &mut Context<Self>) {
        self.open_prune(cx);
    }

    pub fn show_bulk(&mut self, cx: &mut Context<Self>) {
        self.toggle_bulk(cx);
    }

    /// Put `text` in the containers search, as if it had been typed.
    pub fn show_search_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.view.lists[SearchKind::Containers as usize]
            .search
            .update(cx, |input, cx| input.set_value(text, cx));
        cx.notify();
    }

    /// The cleanup sheet with only options ticked that have nothing left.
    pub fn show_cleanup_with_nothing_to_do(&mut self, cx: &mut Context<Self>) {
        self.open_prune(cx);
        self.toggle_prune_option(1, cx);
        self.toggle_prune_option(4, cx);
    }

    pub fn show_create(&mut self, kind: CreateKind, cx: &mut Context<Self>) {
        self.open_create(kind, cx);
    }

    pub fn show_toast_for_screenshot(&mut self, cx: &mut Context<Self>) {
        self.notify("Deleted berth-demo-web", false, cx);
    }

    pub fn confirm_first_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(container) = self.snapshot.containers.first() {
            self.confirm_action(
                "DELETE".into(),
                format!("/containers/{}?force=1", container.id),
                "Delete".into(),
                container.name().to_owned(),
                cx,
            );
        }
    }

    pub fn select_fixture_container(&mut self, tab: InspectorTab, cx: &mut Context<Self>) {
        self.select(0, cx);
        self.inspector.tab = tab;
        self.inspector.logs_loading = false;
        self.inspector.inspect_loading = false;
        self.inspector.logs_text =
            "2026-11-07 15:20:01 [notice] 1#1: nginx/1.27.2\n2026-11-07 15:20:01 [notice] 1#1: start worker processes\n172.18.0.1 - - [07/Nov/2026:15:21:10 +0000] \"GET / HTTP/1.1\" 200 615".into();
        self.inspector.inspect_facts = Some(berth_engine::detail::InspectFacts {
            started_at: Some(berth_core::format::now() - 18 * 3600),
            restarts: 0,
            exit_code: None,
            oom_killed: false,
            restart_policy: Some("unless-stopped".into()),
            health_log: None,
            command: Some("nginx -g 'daemon off;'".into()),
            workdir: Some("/usr/share/nginx/html".into()),
            env: vec![
                ("NGINX_HOST".into(), "localhost".into(), false),
                ("NGINX_PORT".into(), "80".into(), false),
                ("API_SECRET_TOKEN".into(), "<redacted>".into(), true),
            ],
            mounts: vec![berth_engine::detail::MountSummary {
                kind: "volume".into(),
                source: "shop-data".into(),
                destination: "/var/www/html".into(),
                rw: true,
            }],
            ips: vec![("shop_default".into(), "172.18.0.2".into())],
            // What the engine crate makes of this container's inspect
            // document; its own test holds the same line.
            run_cmd: Some(
                concat!(
                    "docker run -d --name berth-demo-web --restart unless-stopped",
                    " --network shop_default -w /usr/share/nginx/html",
                    " --entrypoint /docker-entrypoint.sh -p 8080:80",
                    " -v shop-data:/var/www/html",
                    " -e NGINX_HOST=localhost -e NGINX_PORT=80",
                    " -e 'API_SECRET_TOKEN=<redacted>'",
                    " --label com.docker.compose.project=shop",
                    " --label com.docker.compose.service=web",
                    " nginx:alpine nginx -g 'daemon off;'"
                )
                .into(),
            ),
            run_redacted: true,
        });
        self.inspector.detail_top = Some(berth_engine::dto::ContainerTop {
            titles: vec!["PID".into(), "USER".into(), "TIME".into(), "COMMAND".into()],
            processes: vec![
                vec![
                    "1042".into(),
                    "root".into(),
                    "0:00".into(),
                    "nginx: master process nginx -g daemon off;".into(),
                ],
                vec![
                    "1089".into(),
                    "nginx".into(),
                    "0:02".into(),
                    "nginx: worker process".into(),
                ],
            ],
        });
        self.inspector.detail_changes = vec![
            berth_engine::dto::FsChange {
                path: "/var/cache/nginx/client_temp".into(),
                kind: 1,
            },
            berth_engine::dto::FsChange {
                path: "/etc/nginx/conf.d/default.conf".into(),
                kind: 0,
            },
            berth_engine::dto::FsChange {
                path: "/run/nginx.pid".into(),
                kind: 1,
            },
        ];
        cx.notify();
    }

    pub fn select_fixture_image(&mut self, cx: &mut Context<Self>) {
        if let Some(image) = self.snapshot.images.first().cloned() {
            self.select_image(&image.id, image.label(), cx);
            self.inspector.image_layers_loading = false;
            self.inspector.image_layers = vec![
                berth_engine::dto::ImageLayer {
                    id: "sha256:a1".into(),
                    created: berth_core::format::now() - 86_400 * 12,
                    created_by: "/bin/sh -c #(nop) CMD [\"nginx\" \"-g\" \"daemon off;\"]".into(),
                    size: 0,
                    comment: String::new(),
                },
                berth_engine::dto::ImageLayer {
                    id: "sha256:a2".into(),
                    created: berth_core::format::now() - 86_400 * 12,
                    created_by: "/bin/sh -c apk add --no-cache nginx-mod-http-js".into(),
                    size: 40 * 1024 * 1024,
                    comment: String::new(),
                },
                berth_engine::dto::ImageLayer {
                    id: "sha256:a3".into(),
                    created: berth_core::format::now() - 86_400 * 30,
                    created_by: "/bin/sh -c #(nop) ADD file:alpine-minirootfs-3.22.tar.gz in /"
                        .into(),
                    size: 8 * 1024 * 1024,
                    comment: String::new(),
                },
            ];
            cx.notify();
        }
    }
}

/// A snapshot of `count` containers for the performance pass.
pub fn state_for_perf(count: usize) -> Snapshot {
    fixture_snapshot_with(count)
}

/// A snapshot with a little of everything: two running containers in a
/// Compose project, a volume they mount, images (one left behind by a
/// build), networks, events, and the disk figures that go with them.
pub fn fixture_snapshot() -> Snapshot {
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
    // Every store measured, as a current engine reports them. The volume
    // figure matches the fixture's one volume.
    let df = Df {
        image_usage: Some(Usage {
            total_size: 103 * 1024 * 1024,
            reclaimable: 13 * 1024 * 1024,
            ..Default::default()
        }),
        container_usage: Some(Usage::default()),
        volume_usage: Some(Usage {
            total_size: 32 * 1024 * 1024,
            ..Default::default()
        }),
        build_cache_usage: Some(Usage::default()),
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
        // Left behind by a build: no container uses it, so it is the image
        // the disk figures call reclaimable.
        Image {
            id: "sha256:3333333333333333333333333333333333333333333333333333333333333333".into(),
            repo_tags: None,
            created: format::now() - 86_400 * 3,
            size: 13 * 1024 * 1024,
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
    }
}

/// The machine's figures as the fixture's host card shows them.
fn fixture_host() -> HostSample {
    HostSample {
        mem_total: 64 * 1024 * 1024 * 1024,
        mem_used: 43 * 1024 * 1024 * 1024 + 400 * 1024 * 1024,
        cpu_percent: 48.9,
        cpu_known: true,
        engine_rss: 44 * 1024 * 1024,
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
    snapshot.projects.clear();
    snapshot
}

pub fn fixture_runtime() -> Runtime {
    Runtime {
        name: "Lima · lima-berth-docker".into(),
        kind: Kind::Lima,
        dialect: Dialect::Docker,
        socket: "/home/me/.lima/berth-docker/sock/docker.sock".into(),
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
