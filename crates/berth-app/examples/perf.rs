//! 100-container headless render budget. Run:
//!   cargo run -p berth-app --example perf

use std::sync::Arc;
use std::time::Instant;

use berth_app::BerthApp;
use berth_ui::theme;
use gpui::{AppContext, HeadlessAppContext, px, size};

fn main() -> anyhow::Result<()> {
    let platform = gpui_platform::current_platform(true);
    let text_system = platform.text_system();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(berth_app::Assets), || {
            gpui_platform::current_headless_renderer()
        });
    let snapshot = berth_app::state_for_perf(100);
    let handle = cx.open_window(size(px(1280.0), px(760.0)), |_window, cx| {
        cx.new(|cx| BerthApp::with_snapshot(theme::light(), snapshot, true, cx))
    })?;
    cx.run_until_parked();
    let started = Instant::now();
    let image = cx.capture_screenshot(handle.into())?;
    let elapsed = started.elapsed();
    image.save("docs/ui/perf-100-containers.png")?;
    println!("100-container frame: {elapsed:?}");
    if elapsed.as_secs_f32() > 2.0 {
        anyhow::bail!("100-container frame exceeded 2 s");
    }
    Ok(())
}
