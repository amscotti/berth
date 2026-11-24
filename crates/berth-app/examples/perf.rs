//! Headless frame budget. Run:
//!   cargo run -p berth-app --release --example perf --features test-driver
//!
//! Draws the Containers page with 100, 500, 1,000 and 5,000 containers,
//! and times the frame a change costs: the shell announces one, and the
//! window is drawn. A frame over the budget fails the run, with the figure
//! that did it. `BERTH_PERF_ROWS=100,20000` measures other counts instead.
//!
//! The figure is the middle one of a few frames, after the first ones have
//! loaded the fonts. It is a figure for the machine it ran on.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use berth_app::BerthApp;
use berth_ui::theme;
use gpui::{AppContext, Entity, HeadlessAppContext, px, size};

/// One frame at 30 Hz. A change that costs more drops a frame.
const BUDGET: Duration = Duration::from_millis(33);
/// The row counts the gate holds. The last is the most the engine is asked
/// to list from a store.
const ROWS: [usize; 4] = [100, 500, 1_000, 5_000];
/// Frames drawn before any is measured.
const WARM_UP: usize = 3;
/// Frames measured. The middle one is the figure.
const MEASURED: usize = 9;

fn main() -> anyhow::Result<()> {
    let platform = gpui_platform::current_platform(true);
    let text_system = platform.text_system();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(berth_app::Assets), || {
            gpui_platform::current_headless_renderer()
        });
    let counts: Vec<usize> = match std::env::var("BERTH_PERF_ROWS") {
        Ok(list) => list
            .split(',')
            .map(|count| count.trim().parse::<usize>())
            .collect::<Result<_, _>>()
            .context("BERTH_PERF_ROWS is a list of row counts, as 100,5000")?,
        Err(_) => ROWS.to_vec(),
    };

    let mut over = Vec::new();
    for count in &counts {
        let frame = measure(&mut cx, *count)?;
        println!("{count} containers: {frame:?} a frame");
        if frame > BUDGET {
            over.push((count, frame));
        }
    }
    for (count, frame) in &over {
        eprintln!("{count} containers: {frame:?} a frame is over the {BUDGET:?} budget");
    }
    anyhow::ensure!(
        over.is_empty(),
        "{} of {} frames are over the budget",
        over.len(),
        counts.len()
    );
    Ok(())
}

/// What a change costs with `count` containers on screen: the middle frame
/// of a few, after the ones that loaded the fonts.
fn measure(cx: &mut HeadlessAppContext, count: usize) -> anyhow::Result<Duration> {
    let snapshot = berth_app::state_for_perf(count);
    let mut made: Option<Entity<BerthApp>> = None;
    let handle = cx.open_window(size(px(1280.0), px(760.0)), |_window, cx| {
        let app = cx.new(|cx| BerthApp::with_fixture(theme::light(), snapshot, cx));
        made = Some(app.clone());
        app
    })?;
    let app = made.expect("the window was built");
    cx.run_until_parked();

    let mut frames = Vec::with_capacity(WARM_UP + MEASURED);
    for _ in 0..WARM_UP + MEASURED {
        app.update(cx, |_, cx| cx.notify());
        frames.push(cx.update_window(handle.into(), |_, window, cx| {
            let started = Instant::now();
            let _ = window.draw(cx);
            started.elapsed()
        })?);
    }
    if count == ROWS[0] {
        cx.capture_screenshot(handle.into())?
            .save("docs/ui/perf-100-containers.png")?;
    }
    frames[WARM_UP..].sort();
    Ok(frames[WARM_UP + MEASURED / 2])
}
