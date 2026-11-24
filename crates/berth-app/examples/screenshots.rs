//! Headless render of every page, light and dark.
//!
//!   cargo run -p berth-app --example screenshots
//!
//! Writes PNGs to `docs/ui/`. This is the review path while the views are
//! being built; the real golden matrix becomes a serial CI job.
//!
//! As well as every page, it renders the states a page can be in: bulk
//! selection, each sheet, a toast, the inspector, and the keyboard's point of
//! focus, which only shows once a key has been pressed.

use std::sync::Arc;

use berth_app::{BerthApp, Tab};
use berth_ui::theme;
use gpui::{AppContext, HeadlessAppContext, Keystroke, px, size};

/// How many presses of Tab, from nothing focused, reach the Running filter:
/// the rail's ten pages, the page's action, the search and the All chip.
const TABS_TO_A_FILTER: usize = 14;

fn main() -> anyhow::Result<()> {
    let out = std::path::Path::new("docs/ui");
    std::fs::create_dir_all(out)?;

    let platform = gpui_platform::current_platform(true);
    let text_system = platform.text_system();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(berth_app::Assets), || {
            gpui_platform::current_headless_renderer()
        });
    // The keys the app binds answer here as they do in a window.
    cx.update(|cx| berth_commands::install_keymap(cx, berth_commands::Platform::current()));

    for (theme_name, tokens) in [("light", theme::light()), ("dark", theme::dark())] {
        let sizes: &[(u32, u32)] = if theme_name == "light" {
            &[(960, 600), (1280, 760), (1440, 900)]
        } else {
            &[(1280, 760)]
        };
        for (width, height) in sizes.iter().copied() {
            for tab in Tab::ALL {
                let handle =
                    cx.open_window(size(px(width as f32), px(height as f32)), |_window, cx| {
                        cx.new(|cx| {
                            let mut app = BerthApp::new_fixture(tokens, cx);
                            app.show_tab(tab);
                            app
                        })
                    })?;
                cx.run_until_parked();
                let image = cx.capture_screenshot(handle.into())?;
                let name = tab.label().to_ascii_lowercase().replace(' ', "-");
                let path = if width == 1280 && height == 760 {
                    out.join(format!("{name}-{theme_name}.png"))
                } else {
                    out.join(format!("{name}-{theme_name}-{width}x{height}.png"))
                };
                image.save(&path)?;
                println!("wrote {}", path.display());
            }
        }
    }
    for (name, setup) in [
        ("sheet-prune", 0u8),
        ("sheet-create-volume", 1),
        ("sheet-confirm-delete", 2),
        ("toast", 3),
        ("inspector-container", 4),
        ("inspector-config", 5),
        ("inspector-runtime", 6),
        ("inspector-image", 7),
        ("containers-bulk", 8),
        ("sheet-cleanup-nothing", 9),
        ("search-filled", 10),
        ("overview-partial", 11),
        ("overview-facts-missing", 12),
    ] {
        let handle = cx.open_window(size(px(1280.0), px(760.0)), |_window, cx| {
            cx.new(|cx| {
                let mut app = BerthApp::new_fixture(theme::light(), cx);
                app.show_tab(Tab::Containers);
                match setup {
                    0 => app.show_prune(cx),
                    1 => app.show_create(berth_app::CreateKind::Volume, cx),
                    2 => app.confirm_first_delete(cx),
                    3 => app.show_toast_for_screenshot(cx),
                    4 => app.select_fixture_container(berth_app::InspectorTab::Facts, cx),
                    5 => app.select_fixture_container(berth_app::InspectorTab::Config, cx),
                    6 => app.select_fixture_container(berth_app::InspectorTab::Runtime, cx),
                    7 => {
                        app.show_tab(Tab::Images);
                        app.select_fixture_image(cx);
                    }
                    8 => app.show_bulk(cx),
                    9 => app.show_cleanup_with_nothing_to_do(cx),
                    11 => {
                        app.show_tab(Tab::Overview);
                        app.show_partial_sample();
                    }
                    12 => app.show_engine_facts_missing(),
                    _ => app.show_search_text("web", cx),
                }
                app
            })
        })?;
        cx.run_until_parked();
        let image = cx.capture_screenshot(handle.into())?;
        let path = out.join(format!("{name}.png"));
        image.save(&path)?;
        println!("wrote {}", path.display());
    }
    // The focus ring, in each theme. It shows for the keyboard only, so the
    // keys are pressed as a person would press them.
    for (theme_name, tokens) in [("light", theme::light()), ("dark", theme::dark())] {
        let handle = cx.open_window(size(px(1280.0), px(760.0)), |_window, cx| {
            cx.new(|cx| {
                let mut app = BerthApp::new_fixture(tokens, cx);
                app.show_tab(Tab::Containers);
                app
            })
        })?;
        cx.run_until_parked();
        let tab = Keystroke::parse("tab").expect("`tab` is a keystroke");
        cx.update_window(handle.into(), |_, window, cx| {
            for _ in 0..TABS_TO_A_FILTER {
                window.dispatch_keystroke(tab.clone(), cx);
            }
        })?;
        cx.run_until_parked();
        let image = cx.capture_screenshot(handle.into())?;
        let path = out.join(format!("focus-tab-{theme_name}.png"));
        image.save(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
