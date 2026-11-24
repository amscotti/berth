//! The readings the Overview's cards give: what a partial sample says, and
//! where the memory graph's ceiling comes from.

use super::*;

fn runtime(info_read: bool, mem_total: u64) -> berth_engine::discovery::Runtime {
    berth_engine::discovery::Runtime {
        info_read,
        info: berth_engine::dto::Info {
            mem_total,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn snapshot(sampled: &[Option<bool>]) -> crate::state::Snapshot {
    // One up container per entry, measured where the entry says so.
    let containers = sampled
        .iter()
        .map(|_| berth_engine::dto::Container {
            state: "running".into(),
            ..Default::default()
        })
        .collect();
    let stats = sampled
        .iter()
        .map(|measured| berth_engine::stats::Sample {
            sampled: measured.is_some(),
            cpu_known: measured.is_some(),
            cpu: 10.0,
            mem_known: measured.is_some(),
            mem: 1024,
            ..Default::default()
        })
        .collect();
    crate::state::Snapshot {
        containers,
        stats,
        ..Default::default()
    }
}

#[test]
fn a_partial_sample_says_how_partial_it_is_and_shows_what_it_measured() {
    // Two of three measured: the figures are the two, and the note says
    // so, in the same words for both.
    let sample = sample(
        &snapshot(&[Some(true), Some(true), None]),
        true,
        4,
        Some(4000),
    );
    assert_eq!(sample.cpu.figure, "5.0%");
    assert_eq!(sample.memory.figure, "2.0 KiB");
    assert_eq!(sample.cpu.note, "partial sample · 2 of 3");
    assert_eq!(sample.memory.note, "partial sample · 2 of 3");
    assert!(sample.cpu.partial && sample.memory.partial);
}

#[test]
fn a_complete_sample_is_stated_plainly() {
    let sample = sample(&snapshot(&[Some(true), Some(true)]), true, 4, Some(4000));
    assert_eq!(sample.cpu.figure, "5.0%");
    assert!(!sample.cpu.partial);
    assert_eq!(sample.cpu.note, "of 4 cores");
    assert_eq!(sample.memory.note, "of 3.9 KiB");
}

#[test]
fn nothing_measured_is_unknown_not_zero() {
    // An engine that measured nothing, or is not answering: a dash, and
    // no note that would make the dash into a reading.
    for answering in [true, false] {
        let sample = sample(&snapshot(&[None, None]), answering, 4, Some(4000));
        assert_eq!(sample.cpu.figure, "—");
        assert_eq!(sample.memory.figure, "—");
        assert_eq!(sample.cpu.note, "");
        assert!(!sample.cpu.partial);
    }
}

#[test]
fn the_memory_ceiling_is_the_engine_total_and_nothing_else() {
    assert_eq!(
        memory_ceiling(Some(&runtime(true, 4_000_000_000))),
        Some(4.0e9)
    );
    // No engine, an engine that didn't answer /info, or one that reported
    // no total: the ceiling is unknown, not one.
    assert_eq!(memory_ceiling(None), None);
    assert_eq!(memory_ceiling(Some(&runtime(false, 4_000_000_000))), None);
    assert_eq!(memory_ceiling(Some(&runtime(true, 0))), None);
}

#[test]
fn an_engine_fact_the_engine_left_empty_is_a_dash() {
    assert_eq!(fact(""), "—");
    assert_eq!(fact("overlayfs"), "overlayfs");
}

#[test]
fn an_engine_that_said_nothing_about_itself_gets_one_line_not_three_empty_ones() {
    let mut runtime = runtime(false, 0);
    runtime.kind = berth_engine::discovery::Kind::Lima;
    runtime.version.version = "29.9.2".into();
    runtime.version.api_version = "1.56".into();
    let snapshot = crate::state::Snapshot::default();
    let lines = engine_facts(&runtime, &berth_engine::dto::Info::default(), &snapshot);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0], "Lima 29.9.2 · API 1.56 · storage —");
    assert_eq!(lines[1], "The engine's own facts were not read.");
    assert!(lines[2].starts_with("0 containers"), "{}", lines[2]);

    // Read, with fields the engine left out: those are dashes, and the
    // lines still say the rest.
    let mut info = berth_engine::dto::Info {
        driver: "overlayfs".into(),
        operating_system: "Ubuntu 26.04 LTS".into(),
        os_type: "linux".into(),
        architecture: "aarch64".into(),
        ..Default::default()
    };
    runtime.info_read = true;
    let lines = engine_facts(&runtime, &info, &snapshot);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[0], "Lima 29.9.2 · API 1.56 · storage overlayfs");
    assert_eq!(lines[1], "root —");
    assert_eq!(lines[2], "Ubuntu 26.04 LTS · linux/aarch64 · cgroups —");
    info.docker_root_dir = "/var/lib/docker".into();
    assert_eq!(
        engine_facts(&runtime, &info, &snapshot)[1],
        "root /var/lib/docker"
    );
}

#[test]
fn zero_running_containers_is_zero_when_connected_and_unknown_when_disconnected() {
    let empty = crate::state::Snapshot::default();
    let connected = sample(&empty, true, 4, Some(4000));
    assert_eq!(connected.cpu.figure, "0.0%");
    assert_eq!(connected.memory.figure, "0 B");
    assert!(!connected.cpu.partial && !connected.memory.partial);

    let offline = sample(&empty, false, 4, Some(4000));
    assert_eq!(offline.cpu.figure, "—");
    assert_eq!(offline.memory.figure, "—");
}

#[test]
fn missing_cpu_does_not_blank_out_known_memory() {
    let snap = crate::state::Snapshot {
        containers: vec![berth_engine::dto::Container {
            state: "running".into(),
            ..Default::default()
        }],
        stats: vec![berth_engine::stats::Sample {
            sampled: true,
            cpu_known: false,
            mem_known: true,
            mem: 2048,
            ..Default::default()
        }],
        ..Default::default()
    };
    let reading = sample(&snap, true, 4, Some(4096));
    assert_eq!(reading.cpu.figure, "—");
    assert_eq!(reading.memory.figure, "2.0 KiB");
    assert_eq!(reading.memory.note, "of 4.0 KiB");
}

#[test]
fn io_strip_shows_dashes_when_offline_or_unmeasured() {
    let snap = crate::state::Snapshot {
        containers: vec![berth_engine::dto::Container {
            state: "running".into(),
            ..Default::default()
        }],
        stats: vec![berth_engine::stats::Sample {
            sampled: true,
            net_rx: 512,
            net_tx: 256,
            blk_known: false,
            pids: None,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        io_figures(&snap, false),
        (
            "↓ — · ↑ —".to_owned(),
            "read — · wrote —".to_owned(),
            "—".to_owned()
        )
    );
    assert_eq!(
        io_figures(&snap, true),
        (
            "↓ 512 B · ↑ 256 B".to_owned(),
            "read — · wrote —".to_owned(),
            "—".to_owned()
        )
    );
    assert_eq!(
        io_figures(&crate::state::Snapshot::default(), true),
        (
            "↓ 0 B · ↑ 0 B".to_owned(),
            "read 0 B · wrote 0 B".to_owned(),
            "0".to_owned()
        )
    );
}
