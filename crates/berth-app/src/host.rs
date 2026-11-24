//! The machine's own figures: memory, CPU and the engine's helper processes.
//!
//! Sampling runs on the refresh worker. `sysinfo` gives the same three
//! answers on macOS and Linux, so there is nothing here that reads a file in
//! `/proc` by hand.

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
