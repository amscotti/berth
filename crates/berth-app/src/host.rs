//! The machine's own figures: memory, CPU and the engine's helper processes.
//!
//! Sampling runs on the refresh worker. `sysinfo` gives the same three answers
//! on macOS and Linux; the parsers below are kept pure so the arithmetic is
//! tested without a host.

use std::sync::Mutex;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::state::HostSample;

/// The process names that make up the engine's host-side footprint. A VM's
/// guest RAM is not here; that belongs to the containers.
/// Stored lowercase to avoid allocating per process during filtering.
const ENGINE_PROCESSES: &[&str] = &[
    "docker",
    "dockerd",
    "docker desktop",
    "com.docker.backend",
    "com.docker.virtualization",
    "containerd",
    "podman",
    "gvproxy",
    "vpnkit",
    "colima",
    "limactl",
    "qemu-system",
    "virtiofsd",
];

static HOST_SYSTEM: Mutex<Option<System>> = Mutex::new(None);

pub fn sample() -> HostSample {
    let mut guard = HOST_SYSTEM
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let first_sample = guard.is_none();
    let system = guard.get_or_insert_with(System::new);

    system.refresh_memory();
    system.refresh_cpu_all();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new().with_memory(),
    );

    let total = system.total_memory();
    let used = system.used_memory();
    let cpu = system.global_cpu_usage();
    let engine_rss = system
        .processes()
        .values()
        .filter(|process| {
            let name = process.name().as_encoded_bytes();
            // Containerd shims are excluded from engine helpers so they are not counted twice.
            !contains_ignore_ascii_case(name, b"containerd-shim")
                && ENGINE_PROCESSES
                    .iter()
                    .any(|marker| contains_ignore_ascii_case(name, marker.as_bytes()))
        })
        .map(sysinfo::Process::memory)
        .sum();

    HostSample {
        mem_total: total,
        mem_used: used,
        cpu_percent: cpu,
        cpu_known: !first_sample && cpu.is_finite() && cpu >= 0.0,
        engine_rss,
    }
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Parse the two fields of `/proc/meminfo` the host card needs.
#[allow(dead_code)]
pub fn parse_meminfo(text: &str) -> (u64, u64) {
    let mut total = 0;
    let mut available = 0;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let Some(value) = parts.next().and_then(|value| value.parse::<u64>().ok()) else {
            continue;
        };
        match key {
            "MemTotal:" => total = value.saturating_mul(1024),
            "MemAvailable:" => available = value.saturating_mul(1024),
            _ => {}
        }
    }
    (total, total.saturating_sub(available))
}

/// Parse `/proc/stat`'s aggregate line into (busy, total) ticks.
#[allow(dead_code)]
pub fn parse_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let ticks: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|value| value.parse::<u64>().ok())
        .collect();
    if ticks.len() < 4 {
        return None;
    }
    let total: u64 = ticks.iter().sum();
    let idle = ticks[3].saturating_add(ticks.get(4).copied().unwrap_or(0));
    Some((total.saturating_sub(idle), total))
}

#[allow(dead_code)]
pub fn cpu_percent(previous: (u64, u64), now: (u64, u64)) -> Option<f32> {
    let busy = now.0.checked_sub(previous.0)?;
    let total = now.1.checked_sub(previous.1)?;
    (total > 0).then_some((busy as f32 / total as f32) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_reads_total_and_available() {
        let text = "MemTotal:       16384000 kB\nMemFree: 1000 kB\nMemAvailable:    8192000 kB\n";
        assert_eq!(parse_meminfo(text), (16384000 * 1024, 8192000 * 1024));
    }

    #[test]
    fn stat_reads_the_aggregate_line() {
        let text = "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 2 3 4\n";
        assert_eq!(parse_stat(text), Some((200, 1000)));
    }

    #[test]
    fn a_cpu_delta_is_a_percentage() {
        let percent = cpu_percent((100, 1000), (200, 2000)).expect("delta");
        assert!((percent - 10.0).abs() < 0.001);
        assert_eq!(cpu_percent((200, 2000), (100, 1000)), None);
    }
}
