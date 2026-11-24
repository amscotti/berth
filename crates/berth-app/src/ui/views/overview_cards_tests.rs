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
