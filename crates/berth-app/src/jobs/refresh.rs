//! One complete engine pass. The worker builds a whole `Snapshot` and returns
//! it; the UI thread swaps it in one step. A core failure aborts the pass and
//! leaves the previous snapshot on screen.

use berth_core::format;
use berth_engine::client::{Client, ClientError};
use berth_engine::df::Df;
use berth_engine::discovery::Dialect;
use berth_engine::events::{Event, merge};
use berth_engine::network_counts::Counts;
use berth_engine::projects::summaries;
use berth_engine::stats::Sample;

use crate::state::Snapshot;

/// How far back to read events on the first pass, in seconds, before there is
/// a cursor to continue from. The engine keeps its most recent events, so this
/// shows what happened just before berth was opened.
pub const EVENTS_BACKFILL: i64 = 5 * 60;

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
    /// The machine's own figures, read on the same pass. Kept beside the
    /// snapshot, not in it, so the next sample doesn't rebuild the snapshot.
    pub host: crate::state::HostSample,
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
    let stats = sample_stats(&client, &containers, request.cores);

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
    // `since` is a time on the engine's clock (see `events::cursor`), carried
    // over from the last pass. `until` is on the host's; the client samples
    // the stream, so a clock that has drifted only costs the sample window.
    let since = if request.events_since > 0 {
        request.events_since
    } else {
        now - EVENTS_BACKFILL
    };
    let mut events = Vec::new();
    let mut events_error = false;
    match client.events(since, now.max(since + 1)) {
        Ok(fresh) => events = fresh,
        Err(_) => events_error = true,
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
    };

    Ok(Outcome {
        snapshot,
        host: crate::host::sample(),
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
) -> Vec<Sample> {
    let running: Vec<(usize, String)> = containers
        .iter()
        .enumerate()
        .filter(|(_, container)| container.is_up())
        .map(|(index, container)| (index, container.id.clone()))
        .collect();
    let mut stats = vec![Sample::default(); containers.len()];
    if running.is_empty() {
        return stats;
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
    stats
}

/// Merge a fresh event interval into the previous ring, newest first.
pub fn merge_events(previous: &[Event], fresh: &[Event]) -> Vec<Event> {
    merge(previous, fresh, 200)
}

#[cfg(test)]
mod tests {
    use super::*;
    use berth_engine::test_support::{MockEngine, Reply};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        let engine = crate::testkit::scripted_engine();
        let outcome = load(request(engine.socket())).expect("pass");
        assert_eq!(outcome.snapshot.containers.len(), 1);
        assert!(outcome.snapshot.stats_complete());
        assert_eq!(outcome.snapshot.stat(0).pids, Some(5));
        assert_eq!(outcome.generation, 3);
        let df = outcome.df.expect("df requested").expect("df ok");
        assert_eq!(df.reclaimable(), 40);
        assert_eq!(engine.count("/stats"), 1);
        assert_eq!(engine.count("/system/df"), 1);
    }

    #[test]
    fn the_events_cursor_follows_the_engine_clock() {
        // The engine's clock reads a minute behind the host's. Like a real
        // engine, the mock answers with every event stamped at or after
        // `since`. A second event happens after the first pass.
        let host_now = format::now();
        // Times on the engine's clock, in seconds as `since` is. The event
        // itself carries nanoseconds.
        let older = host_now - 60;
        let newer = host_now - 30;
        let event = |seconds: i64, name: &str| {
            format!(
                r#"{{"status":"start","id":"{name}","Type":"container","Action":"start","Actor":{{"ID":"{name}","Attributes":{{"name":"{name}"}}}},"time":{seconds},"timeNano":{}}}"#,
                seconds * 1_000_000_000
            )
        };
        let requests = Arc::new(AtomicUsize::new(0));
        let engine = crate::testkit::scripted_engine_with(Reply::With(Arc::new(move |request| {
            let since = request
                .path
                .split("since=")
                .nth(1)
                .and_then(|rest| rest.split('&').next())
                .and_then(|since| since.parse::<i64>().ok())
                .unwrap_or(0);
            let first = requests.fetch_add(1, Ordering::SeqCst) == 0;
            let mut events = Vec::new();
            if since <= older {
                events.push(event(older, "web"));
            }
            if since <= newer && !first {
                events.push(event(newer, "api"));
            }
            Reply::ok(events.join("\n"))
        })));
        fn names(events: &[Event]) -> Vec<&str> {
            events.iter().map(|event| event.name()).collect()
        }

        // The first pass backfills far enough to reach the older event.
        let first = load(request(engine.socket())).expect("pass");
        assert!(!first.snapshot.events_error);
        assert_eq!(names(&first.snapshot.events), ["web"]);

        // With the host's clock as the cursor, the engine is asked for events
        // from a time it hasn't reached, and the newer one is never returned.
        let mut lost = request(engine.socket());
        lost.events_since = first.snapshot.last_update;
        let lost = load(lost).expect("pass");
        assert!(lost.snapshot.events.is_empty());

        // With the engine's own clock it arrives.
        let mut next = request(engine.socket());
        next.events_since = berth_engine::events::cursor(&first.snapshot.events, 0);
        let second = load(next).expect("pass");
        // Merged into the ring as a commit would: newest first, no duplicate.
        let ring = merge_events(&first.snapshot.events, &second.snapshot.events);
        assert_eq!(names(&ring), ["api", "web"]);
    }

    #[test]
    fn an_events_stream_the_engine_holds_open_is_sampled_not_failed() {
        // An engine whose clock is behind the host's never reaches `until`.
        let engine = crate::testkit::scripted_engine_with(Reply::Hang);
        let started = std::time::Instant::now();
        let outcome = load(request(engine.socket())).expect("a held stream is not a failed pass");
        assert!(!outcome.snapshot.events_error);
        assert!(outcome.snapshot.events.is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    #[test]
    fn a_core_failure_aborts_the_pass() {
        let engine = MockEngine::start();
        engine.on("*").error(500, "engine broke");
        let error = load(request(engine.socket())).expect_err("must fail");
        assert!(error.message.contains("engine broke"));
        assert!(!error.engine_dead);
        assert_eq!(engine.paths().len(), 1);
    }

    #[test]
    fn stats_workers_are_bounded_at_sixty_four() {
        assert_eq!(worker_count(1), 1);
        assert_eq!(worker_count(64), 64);
        assert_eq!(worker_count(100), 64);
        assert_eq!(worker_count(0), 1);
    }
}
