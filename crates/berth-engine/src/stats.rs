//! `/containers/{id}/stats` reduction: CPU, working-set memory, network and
//! block totals, PIDs. `precpu_stats` is what makes CPU% a rate; with
//! `one-shot=true` it arrives empty and the formula becomes a lifetime average.

use crate::dto::Stats;

/// One container's figures, from a single stats sample. `sampled` is false
/// until a stats call succeeded: the defaults are not a measurement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub cpu: f64,
    pub cpu_known: bool,
    pub mem: u64,
    pub mem_known: bool,
    pub limit: u64,
    pub net_rx: u64,
    pub net_tx: u64,
    pub blk_read: u64,
    pub blk_write: u64,
    pub blk_known: bool,
    pub pids: Option<u32>,
    pub sampled: bool,
}

impl Sample {
    pub fn from_stats(stats: &Stats) -> Self {
        Self::from_stats_with_cores(stats, 1)
    }

    pub fn from_stats_with_cores(stats: &Stats, fallback_cores: u32) -> Self {
        let net = net_totals(stats);
        let disk = disk_totals(stats);
        let cpu = cpu_percent_with_cores(stats, fallback_cores);
        Self {
            cpu: cpu.unwrap_or(0.0),
            cpu_known: cpu.is_some(),
            mem: mem_used(stats),
            mem_known: stats.memory_stats.usage.is_some(),
            limit: mem_limit(stats),
            net_rx: net.0,
            net_tx: net.1,
            blk_read: disk.0,
            blk_write: disk.1,
            blk_known: stats.blkio_stats.io_service_bytes_recursive.is_some(),
            pids: pids_current(stats),
            sampled: true,
        }
    }
}

/// Whether a previous sample exists and did not move backwards.
pub fn cpu_known(stats: &Stats) -> bool {
    let now = &stats.cpu_stats;
    let pre = &stats.precpu_stats;
    now.system_cpu_usage.unwrap_or(0) > 0
        && pre.system_cpu_usage.unwrap_or(0) > 0
        && now.cpu_usage.total_usage >= pre.cpu_usage.total_usage
        && now.system_cpu_usage.unwrap_or(0) > pre.system_cpu_usage.unwrap_or(0)
}

/// Percent of one core, the figure `docker stats` prints.
pub fn cpu_percent(stats: &Stats) -> Option<f64> {
    cpu_percent_with_cores(stats, 1)
}

pub fn cpu_percent_with_cores(stats: &Stats, fallback_cores: u32) -> Option<f64> {
    if !cpu_known(stats) {
        return None;
    }
    let total = stats.cpu_stats.cpu_usage.total_usage;
    let pre = stats.precpu_stats.cpu_usage.total_usage;
    let system = stats.cpu_stats.system_cpu_usage.unwrap_or(0);
    let pre_system = stats.precpu_stats.system_cpu_usage.unwrap_or(0);
    if system <= pre_system {
        return None;
    }
    if total == pre {
        return Some(0.0);
    }
    let mut cores = stats.cpu_stats.online_cpus.unwrap_or(0);
    if cores == 0 {
        cores = stats.cpu_stats.cpu_usage.percpu_usage.len() as u32;
    }
    if cores == 0 {
        cores = fallback_cores.max(1);
    }
    let cpu_delta = (total - pre) as f64;
    let system_delta = (system - pre_system) as f64;
    Some(cpu_delta / system_delta * cores as f64 * 100.0)
}

/// Working set: usage minus the kernel's reclaimable file cache.
pub fn mem_used(stats: &Stats) -> u64 {
    let usage = stats.memory_stats.usage.unwrap_or(0);
    let Some(cache) = stats
        .memory_stats
        .stats
        .get("inactive_file")
        .or_else(|| stats.memory_stats.stats.get("total_inactive_file"))
        .or_else(|| stats.memory_stats.stats.get("cache"))
    else {
        return usage;
    };
    if *cache >= usage { 0 } else { usage - cache }
}

pub fn mem_limit(stats: &Stats) -> u64 {
    const MAX_SANE_LIMIT: u64 = 1 << 60;
    stats
        .memory_stats
        .limit
        .filter(|&limit| limit < MAX_SANE_LIMIT)
        .unwrap_or(0)
}

/// Bytes received and sent, summed over every interface.
pub fn net_totals(stats: &Stats) -> (u64, u64) {
    stats
        .networks
        .values()
        .fold((0u64, 0u64), |(rx, tx), iface| {
            (
                rx.saturating_add(iface.rx_bytes),
                tx.saturating_add(iface.tx_bytes),
            )
        })
}

/// Bytes read and written through the block layer. Other ops are not bytes.
pub fn disk_totals(stats: &Stats) -> (u64, u64) {
    let Some(entries) = stats.blkio_stats.io_service_bytes_recursive.as_ref() else {
        return (0, 0);
    };
    entries.iter().fold((0u64, 0u64), |(read, write), entry| {
        if entry.op.eq_ignore_ascii_case("read") {
            (read.saturating_add(entry.value), write)
        } else if entry.op.eq_ignore_ascii_case("write") {
            (read, write.saturating_add(entry.value))
        } else {
            (read, write)
        }
    })
}

pub fn pids_current(stats: &Stats) -> Option<u32> {
    stats.pids_stats.current
}

/// The containers' CPU share of the whole machine. Null unless every running
/// container has a real measurement: a partial sum is a lower bound.
pub fn aggregate_cpu_percent(
    containers: &[crate::dto::Container],
    stats: &[Sample],
    cores: f32,
) -> Option<f32> {
    let mut sum = 0.0f32;
    for (index, container) in containers.iter().enumerate() {
        if !container.is_up() {
            continue;
        }
        let sample = stats.get(index)?;
        if !sample.sampled || !sample.cpu_known {
            return None;
        }
        sum += sample.cpu as f32;
    }
    Some(sum / cores.max(1.0))
}

/// True only when every up container has a sample.
pub fn all_sampled(containers: &[crate::dto::Container], stats: &[Sample]) -> bool {
    containers.iter().enumerate().all(|(index, container)| {
        !container.is_up() || stats.get(index).is_some_and(|sample| sample.sampled)
    })
}

/// Container memory for the overview and menu bar, saturating.
pub fn memory_used(containers: &[crate::dto::Container], stats: &[Sample]) -> u64 {
    containers
        .iter()
        .enumerate()
        .filter(|(index, container)| {
            container.is_up() && stats.get(*index).is_some_and(|sample| sample.sampled)
        })
        .fold(0u64, |total, (index, _)| {
            total.saturating_add(stats[index].mem)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{Container, CpuStats, CpuUsage, MemoryStats, NetworkStats, PidsStats};

    fn stats(now: u64, pre: u64, system: u64, pre_system: u64, cores: u32) -> Stats {
        Stats {
            cpu_stats: CpuStats {
                cpu_usage: CpuUsage {
                    total_usage: now,
                    ..Default::default()
                },
                system_cpu_usage: Some(system),
                online_cpus: Some(cores),
            },
            precpu_stats: CpuStats {
                cpu_usage: CpuUsage {
                    total_usage: pre,
                    ..Default::default()
                },
                system_cpu_usage: Some(pre_system),
                online_cpus: Some(cores),
            },
            ..Default::default()
        }
    }

    #[test]
    fn cpu_uses_the_previous_sample() {
        let sample = stats(3_000_000, 1_000_000, 40_000_000, 20_000_000, 4);
        assert!(cpu_known(&sample));
        assert!((cpu_percent(&sample).unwrap() - 40.0).abs() < 0.001);
    }

    #[test]
    fn a_one_shot_sample_has_no_cpu() {
        let mut sample = stats(3_000_000, 0, 40_000_000, 0, 4);
        sample.precpu_stats.cpu_usage.total_usage = 0;
        assert!(!cpu_known(&sample));
        assert_eq!(cpu_percent(&sample), None);
    }

    #[test]
    fn a_counter_that_went_backwards_is_unknown() {
        let sample = stats(1_000_000, 3_000_000, 40_000_000, 20_000_000, 4);
        assert!(!cpu_known(&sample));
        let zero_sys_delta = stats(1_000_000, 1_000_000, 20_000_000, 20_000_000, 4);
        assert!(!cpu_known(&zero_sys_delta));
        assert_eq!(cpu_percent(&zero_sys_delta), None);
    }

    #[test]
    fn fallback_cores_and_unlimited_memory_are_handled() {
        let mut sample = stats(3_000_000, 1_000_000, 40_000_000, 20_000_000, 0);
        sample.memory_stats.limit = Some(u64::MAX);
        assert!((cpu_percent_with_cores(&sample, 8).unwrap() - 80.0).abs() < 0.001);
        assert_eq!(mem_limit(&sample), 0);
    }

    #[test]
    fn memory_is_the_working_set_with_fallbacks() {
        let mut sample = stats(0, 0, 0, 0, 1);
        sample.memory_stats = MemoryStats {
            usage: Some(1000),
            limit: Some(2000),
            stats: [("inactive_file".to_owned(), 200)].into_iter().collect(),
        };
        assert_eq!(mem_used(&sample), 800);
        assert_eq!(mem_limit(&sample), 2000);

        sample.memory_stats.stats = [("total_inactive_file".to_owned(), 100)]
            .into_iter()
            .collect();
        assert_eq!(mem_used(&sample), 900);

        sample.memory_stats.stats = [("cache".to_owned(), 400)].into_iter().collect();
        assert_eq!(mem_used(&sample), 600);

        sample.memory_stats.stats = [("cache".to_owned(), 2000)].into_iter().collect();
        assert_eq!(mem_used(&sample), 0);
    }

    #[test]
    fn network_and_disk_totals_sum_and_saturate() {
        let mut sample = stats(0, 0, 0, 0, 1);
        sample.networks = [
            (
                "eth0".to_owned(),
                NetworkStats {
                    rx_bytes: 10,
                    tx_bytes: 20,
                },
            ),
            (
                "eth1".to_owned(),
                NetworkStats {
                    rx_bytes: 5,
                    tx_bytes: 6,
                },
            ),
        ]
        .into_iter()
        .collect();
        assert_eq!(net_totals(&sample), (15, 26));

        sample.blkio_stats = crate::dto::BlkioStats {
            io_service_bytes_recursive: Some(vec![
                crate::dto::BlkioEntry {
                    op: "Read".into(),
                    value: 100,
                    ..Default::default()
                },
                crate::dto::BlkioEntry {
                    op: "Write".into(),
                    value: 200,
                    ..Default::default()
                },
                crate::dto::BlkioEntry {
                    op: "Sync".into(),
                    value: 999,
                    ..Default::default()
                },
            ]),
        };
        assert_eq!(disk_totals(&sample), (100, 200));
    }

    #[test]
    fn pids_are_unknown_when_the_controller_is_off() {
        let mut sample = stats(0, 0, 0, 0, 1);
        assert_eq!(pids_current(&sample), None);
        sample.pids_stats = PidsStats {
            current: Some(5),
            limit: None,
        };
        assert_eq!(pids_current(&sample), Some(5));
    }

    #[test]
    fn aggregates_require_every_running_container() {
        let containers = vec![
            Container {
                state: "running".into(),
                ..Default::default()
            },
            Container {
                state: "running".into(),
                ..Default::default()
            },
        ];
        let mut stats = vec![
            Sample {
                sampled: true,
                cpu: 10.0,
                cpu_known: true,
                mem: 5,
                ..Default::default()
            },
            Sample {
                sampled: true,
                cpu: 10.0,
                cpu_known: true,
                mem: 7,
                ..Default::default()
            },
        ];
        assert_eq!(aggregate_cpu_percent(&containers, &stats, 4.0), Some(5.0));
        assert_eq!(memory_used(&containers, &stats), 12);
        assert!(all_sampled(&containers, &stats));

        stats[1].sampled = false;
        assert_eq!(aggregate_cpu_percent(&containers, &stats, 4.0), None);
        assert!(!all_sampled(&containers, &stats));
        assert_eq!(memory_used(&containers, &stats), 5);
    }
}
