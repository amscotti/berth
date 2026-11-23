//! The immutable snapshot the window paints and the small amount of engine
//! state around it. A refresh builds a complete `Snapshot`; commit swaps it in
//! one step. The previous snapshot stays alive through the `Arc` while the
//! current frame may still reference it.

use std::sync::Arc;

use berth_core::format;
use berth_core::history::History;
use berth_engine::client::Client;
use berth_engine::df::Df;
use berth_engine::discovery::{Dialect, Runtime};
use berth_engine::dto::{Container, Image, Network, Volume};
use berth_engine::events::Event;
use berth_engine::network_counts::Counts;
use berth_engine::projects::Summary;
use berth_engine::stats::{
    Sample, aggregate_cpu_percent, all_memory_known, all_sampled, memory_used,
};

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub containers: Vec<Container>,
    pub stats: Vec<Sample>,
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

    /// True only when every up container answered, and with a working set.
    pub fn stats_complete(&self) -> bool {
        all_sampled(&self.containers, &self.stats)
            && all_memory_known(&self.containers, &self.stats)
    }

    /// What the engine's stores hold, or `None` when no store has been
    /// measured. A measured empty engine is `Some(0)`, not unknown.
    pub fn store_bytes(&self) -> Option<i64> {
        let stores = self.df.as_ref()?.stores();
        (!stores.is_unmeasured()).then_some(stores.total())
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

/// The machine's own figures, sampled alongside the engine. They aren't the
/// engine's, so they are kept beside the snapshot rather than inside it: a
/// new sample must not rebuild the snapshot, which is the engine's answer.
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
    pub socket: Option<String>,
    pub api: Option<String>,
    pub dialect: Dialect,
    pub client: Option<Client>,
    pub generation: u64,
    /// The engine being read, by identity rather than by place in the list:
    /// it is looked up in `runtimes` whenever it is asked for, so a rescan
    /// that reorders the list, or drops the engine, can't point berth at
    /// another one.
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
    /// Where the engine being read is in the list, if it is still there.
    pub fn active_index(&self) -> Option<usize> {
        let identity = self.identity.as_ref()?;
        self.runtimes
            .iter()
            .position(|runtime| berth_engine::discovery::identity(runtime) == *identity)
    }

    pub fn active_runtime(&self) -> Option<&Runtime> {
        let index = self.active_index()?;
        self.runtimes.get(index)
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

pub type SharedSnapshot = Arc<Snapshot>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Tab;

    #[test]
    fn a_hundred_container_fixture_is_complete() {
        let snapshot = crate::fixture::fixture_snapshot_with(100);
        assert_eq!(snapshot.containers.len(), 100);
        assert_eq!(snapshot.stats.len(), 100);
        assert_eq!(snapshot.running(), 100);
        assert!(snapshot.stats_complete());
        assert_eq!(snapshot.containers[99].name(), "perf-99");
    }

    #[test]
    fn store_figures_tell_an_unmeasured_engine_from_an_empty_one() {
        let mut snapshot = crate::fixture::fixture_snapshot();
        let total = 103 * 1024 * 1024 + 32 * 1024 * 1024;
        assert_eq!(snapshot.store_bytes(), Some(total));
        assert_eq!(Tab::Disk.figure(&snapshot, 1), "135 MiB");

        // Nothing measured: unknown, not zero.
        snapshot.df = Some(Df::default());
        assert_eq!(snapshot.store_bytes(), None);
        assert_eq!(Tab::Disk.figure(&snapshot, 1), "—");

        // Measured, and empty: that is a real zero.
        snapshot.df = Some(Df {
            image_usage: Some(berth_engine::df::Usage::default()),
            ..Default::default()
        });
        assert_eq!(snapshot.store_bytes(), Some(0));
        assert_eq!(Tab::Disk.figure(&snapshot, 1), "0 B");

        snapshot.df = None;
        assert_eq!(snapshot.store_bytes(), None);
    }
}
