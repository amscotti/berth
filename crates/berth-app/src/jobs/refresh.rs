//! One complete engine pass. The worker builds a whole `Snapshot` and returns
//! it; the UI thread swaps it in one step. A core failure aborts the pass and
//! leaves the previous snapshot on screen.

use std::time::Duration;

use berth_core::format;
use berth_engine::client::{Client, ClientError};
use berth_engine::df::Df;
use berth_engine::discovery::Dialect;
use berth_engine::events::{Event, merge};
use berth_engine::network_counts::Counts;
use berth_engine::projects::summaries;
use berth_engine::stats::{Sample, all_sampled};

use crate::state::Snapshot;

/// What the worker needs. Everything is copied; no state is shared.
#[derive(Clone, Debug)]
pub struct Request {
    pub socket: String,
    pub api: String,
    pub dialect: Dialect,
    pub generation: u64,
    pub events_since: i64,
    pub want_df: bool,
    pub cores: u32,
}

#[derive(Debug)]
pub struct Outcome {
    pub snapshot: Snapshot,
    /// `None` when the disk figures were not due; `Some(Err(..))` keeps the
    /// previous document and records why.
    pub df: Option<Result<Df, String>>,
    /// The engine generation this pass was started for.
    pub generation: u64,
}

#[derive(Debug)]
pub struct RefreshError {
    pub message: String,
    pub engine_dead: bool,
    pub generation: u64,
}

impl RefreshError {
    fn core(error: ClientError, generation: u64) -> Self {
        let engine_dead = matches!(
            &error,
            ClientError::Transport(transport) if transport.is_connection_lost()
        );
        Self {
            message: error.message(),
            engine_dead,
            generation,
        }
    }
}

pub fn load(request: Request) -> Result<Outcome, RefreshError> {
    let generation = request.generation;
    let client = Client::new(request.socket)
        .with_api(request.api)
        .with_dialect(request.dialect);

    let containers = client
        .containers()
        .map_err(|error| RefreshError::core(error, generation))?;
    let (stats, stats_complete) = sample_stats(&client, &containers, request.cores);

    let images = client
        .images()
        .map_err(|error| RefreshError::core(error, generation))?;
    let volumes = client
        .volumes()
        .map_err(|error| RefreshError::core(error, generation))?;
    let networks = client
        .networks()
        .map_err(|error| RefreshError::core(error, generation))?;

    let now = format::now();
    let mut events = Vec::new();
    let mut events_error = false;
    if request.events_since > 0 {
        match client.events(request.events_since, now) {
            Ok(fresh) => events = fresh,
            Err(_) => events_error = true,
        }
    }

    let df = if request.want_df {
        Some(client.df().map_err(|error| error.message()))
    } else {
        None
    };

    let projects = summaries(&containers);
    let network_counts = Counts::from_containers(&containers);
    let snapshot = Snapshot {
        containers,
        stats,
        stats_complete,
        images,
        volumes,
        networks,
        projects,
        network_counts,
        events,
        events_error,
        error: String::new(),
        last_update: now,
        df: None,
        df_at: 0,
        df_error: String::new(),
        host: crate::host::sample(),
    };

    Ok(Outcome {
        snapshot,
        df,
        generation: request.generation,
    })
}

/// Ask `/stats` for every running container at once, capped at 64 workers.
/// `/stats?stream=false` holds each request for two collection cycles; serial
/// sampling would make a pass cost two seconds per container.
/// The most `/stats` requests one pass may keep in flight. The engine holds a
/// `stream=false` request for two collection cycles (~2s); 64 workers keep 64
/// containers inside one cycle and 128 inside two.
const MAX_STATS_WORKERS: usize = 64;

fn worker_count(running: usize) -> usize {
    running.clamp(1, MAX_STATS_WORKERS)
}

fn sample_stats(
    client: &Client,
    containers: &[berth_engine::dto::Container],
    cores: u32,
) -> (Vec<Sample>, bool) {
    let running: Vec<(usize, String)> = containers
        .iter()
        .enumerate()
        .filter(|(_, container)| container.is_up())
        .map(|(index, container)| (index, container.id.clone()))
        .collect();
    let mut stats = vec![Sample::default(); containers.len()];
    if running.is_empty() {
        return (stats, true);
    }

    let workers = worker_count(running.len());
    let chunk_size = running.len().div_ceil(workers);
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for chunk in running.chunks(chunk_size) {
            let chunk = chunk.to_vec();
            let client = client.clone();
            handles.push(scope.spawn(move || {
                chunk
                    .into_iter()
                    .map(|(index, id)| {
                        let sample = client
                            .stats(&id)
                            .map(|stats| Sample::from_stats_with_cores(&stats, cores))
                            .unwrap_or_default();
                        (index, sample)
                    })
                    .collect::<Vec<_>>()
            }));
        }
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect::<Vec<_>>()
    });

    for (index, sample) in results {
        stats[index] = sample;
    }
    let complete = all_sampled(containers, &stats);
    (stats, complete)
}

/// Merge a fresh event interval into the previous ring, newest first.
pub fn merge_events(previous: &[Event], fresh: &[Event]) -> Vec<Event> {
    merge(previous, fresh, 200)
}

#[allow(dead_code)]
const _REFRESH_PERIOD: Duration = Duration::from_secs(5);

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn server(
        body_by_path: impl Fn(&str) -> (u16, &'static str) + Send + 'static,
        requests: usize,
    ) -> String {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("berth-refresh-{}-{id}.sock", std::process::id()));
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            for _ in 0..requests {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0u8; 4096];
                let n = stream.read(&mut request).unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..n]);
                let request_path = text
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let (status, body) = body_by_path(request_path);
                let head = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
            }
        });
        path.to_string_lossy().into_owned()
    }

    fn request(socket: String) -> Request {
        Request {
            socket,
            api: "v1.43".into(),
            dialect: Dialect::Docker,
            generation: 3,
            events_since: 0,
            want_df: true,
            cores: 4,
        }
    }

    #[test]
    fn one_pass_builds_a_complete_snapshot() {
        let socket = server(
            |path| {
                if path.contains("/containers/json") {
                    (
                        200,
                        r#"[{"Id":"a","Names":["/web"],"Image":"nginx","State":"running","Status":"Up 1 hour"}]"#,
                    )
                } else if path.contains("/stats") {
                    (
                        200,
                        r#"{"cpu_stats":{"cpu_usage":{"total_usage":3000000},"system_cpu_usage":40000000,"online_cpus":4},"precpu_stats":{"cpu_usage":{"total_usage":1000000},"system_cpu_usage":20000000},"memory_stats":{"usage":1000,"limit":2000,"stats":{"inactive_file":200}},"pids_stats":{"current":5}}"#,
                    )
                } else if path.contains("/images/json") {
                    (200, "[]")
                } else if path.contains("/volumes") {
                    (200, r#"{"Volumes":[]}"#)
                } else if path.contains("/networks") {
                    (200, "[]")
                } else if path.contains("/system/df") {
                    (
                        200,
                        r#"{"LayersSize":0,"ImageUsage":{"TotalSize":100,"Reclaimable":40}}"#,
                    )
                } else {
                    (404, "{}")
                }
            },
            6,
        );
        let outcome = load(request(socket)).expect("pass");
        assert_eq!(outcome.snapshot.containers.len(), 1);
        assert!(outcome.snapshot.stats_complete);
        assert_eq!(outcome.snapshot.stat(0).pids, Some(5));
        assert_eq!(outcome.generation, 3);
        let df = outcome.df.expect("df requested").expect("df ok");
        assert_eq!(df.reclaimable(), 40);
    }

    #[test]
    fn a_core_failure_aborts_the_pass() {
        let socket = server(|_| (500, r#"{"message":"engine broke"}"#), 1);
        let error = load(request(socket)).expect_err("must fail");
        assert!(error.message.contains("engine broke"));
        assert!(!error.engine_dead);
    }
    #[test]
    fn stats_workers_are_bounded_at_sixty_four() {
        assert_eq!(worker_count(1), 1);
        assert_eq!(worker_count(64), 64);
        assert_eq!(worker_count(100), 64);
        assert_eq!(worker_count(0), 1);
    }
}
