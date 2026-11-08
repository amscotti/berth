//! Headless render of every page, light and dark.
//!
//!   cargo run -p berth-app --example screenshots
//!
//! Writes PNGs to `docs/ui/`. This is the review path while the views are
//! being built; the real golden matrix becomes a serial CI job.

use std::sync::Arc;

use berth_app::{BerthApp, Tab};
use berth_ui::theme;
use gpui::{AppContext, HeadlessAppContext, px, size};

fn main() -> anyhow::Result<()> {
    let out = std::path::Path::new("docs/ui");
    std::fs::create_dir_all(out)?;

    let platform = gpui_platform::current_platform(true);
    let text_system = platform.text_system();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(berth_app::Assets), || {
            gpui_platform::current_headless_renderer()
        });

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
                    _ => {
                        app.show_tab(Tab::Images);
                        app.select_fixture_image(cx);
                    }
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
    Ok(())
}
