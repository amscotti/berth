//! Saved preferences: appearance, the last chosen socket and where the window
//! was, in the platform data directory. A small JSON file, because the app
//! owns its own storage on both macOS and Linux.
//!
//! The file is replaced in one step or not at all. It is written beside
//! itself and renamed over itself, so an interrupted save leaves the old
//! preferences whole rather than half of the new ones.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use berth_ui::layout;
use gpui::{App, Bounds, Pixels, Size, point, px, size};
use serde::{Deserialize, Serialize};

/// Monotonic sequence so concurrent or direct saves never share one `.tmp` name.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
/// Monotonic ticket assigned on the caller's thread to preserve save order.
static SAVE_TICKET: AtomicU64 = AtomicU64::new(1);

fn save_lock() -> &'static Mutex<HashMap<PathBuf, u64>> {
    static LOCK: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reserve an ordered ticket for an upcoming asynchronous save.
pub fn next_save_ticket() -> u64 {
    SAVE_TICKET.fetch_add(1, Ordering::SeqCst)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

/// Where the window was, in points on the desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Window {
    /// What to remember of a window whose frame is `frame`, title bar and
    /// all, and whose content is `content`.
    ///
    /// The place is the frame's corner and the size is the content's,
    /// because that is what a window is opened from: its frame put at that
    /// place, with that much content in it. Either alone drifts. The frame's
    /// size grows by a title bar on every launch; the content's corner,
    /// which is a title bar below the frame's, walks down the screen.
    pub fn of(frame: Bounds<Pixels>, content: Size<Pixels>) -> Self {
        let round = |value: Pixels| f32::from(value).round();
        Self {
            x: round(frame.origin.x) as i32,
            y: round(frame.origin.y) as i32,
            width: round(content.width).max(0.0) as u32,
            height: round(content.height).max(0.0) as u32,
        }
    }

    /// The rect to open the window with, if that is a window berth can
    /// open: no smaller than its minimum, with its middle on a display, and
    /// clamped inside that display so its titlebar and edges remain reachable.
    /// A display unplugged since, or a size from an older build, puts it back
    /// in the usual place.
    pub fn bounds(
        self,
        minimum: Size<Pixels>,
        displays: &[Bounds<Pixels>],
    ) -> Option<Bounds<Pixels>> {
        let bounds = Bounds::new(
            point(px(self.x as f32), px(self.y as f32)),
            size(px(self.width as f32), px(self.height as f32)),
        );
        if bounds.size.width < minimum.width || bounds.size.height < minimum.height {
            return None;
        }
        let middle = bounds.center();
        let display = displays.iter().find(|display| display.contains(&middle))?;
        let width = bounds.size.width.min(display.size.width.max(minimum.width));
        let height = bounds
            .size
            .height
            .min(display.size.height.max(minimum.height));
        let min_x = display.origin.x;
        let max_x = (display.origin.x + display.size.width - width).max(min_x);
        let min_y = display.origin.y;
        let max_y = (display.origin.y + display.size.height - height).max(min_y);
        let x = bounds.origin.x.clamp(min_x, max_x);
        let y = bounds.origin.y.clamp(min_y, max_y);
        Some(Bounds::new(point(x, y), size(width, height)))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub appearance: Appearance,
    pub socket: String,
    pub window: Option<Window>,
}

pub fn path() -> Option<PathBuf> {
    dirs::data_dir().map(|root| root.join("berth").join("settings.json"))
}

pub fn load() -> Data {
    let Some(path) = path() else {
        return Data::default();
    };
    load_from(&path)
}

pub fn load_from(path: &Path) -> Data {
    let Ok(bytes) = std::fs::read(path) else {
        return Data::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Where a new window opens: where it was left, if that is still somewhere
/// on a display, and in the middle of the main display otherwise.
pub fn opening_bounds(saved: &Data, cx: &App) -> Bounds<Pixels> {
    let minimum = size(px(layout::WINDOW_MIN.0), px(layout::WINDOW_MIN.1));
    let displays: Vec<Bounds<Pixels>> = cx
        .displays()
        .iter()
        .map(|display| display.bounds())
        .collect();
    saved
        .window
        .and_then(|window| window.bounds(minimum, &displays))
        .unwrap_or_else(|| {
            Bounds::centered(
                None,
                size(px(layout::WINDOW_DEFAULT.0), px(layout::WINDOW_DEFAULT.1)),
                cx,
            )
        })
}

/// Serialize saves to `path` and skip any write superseded by a newer ticket.
pub fn save_ordered(path: &Path, data: &Data, ticket: u64) -> std::io::Result<()> {
    let mut guard = save_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard
        .get(path)
        .is_some_and(|&last_written| ticket < last_written)
    {
        return Ok(());
    }
    save_to(path, data)?;
    guard.insert(path.to_path_buf(), ticket);
    Ok(())
}

pub fn save_to(path: &Path, data: &Data) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(data)?;
    // Beside the file, on the same volume, with a unique sequence so concurrent
    // writes never collide on one temporary file, and the rename is one step.
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(
        "{}.{}.{seq}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("settings.json"),
        std::process::id()
    ));
    // The file is made whole, and pushed to the disk, before it replaces
    // the old one. Until then the old one is untouched.
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&body)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use berth_engine::test_support::TempDir;

    fn display() -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)))
    }

    #[test]
    fn a_missing_file_defaults_to_system() {
        let dir = TempDir::new("settings-missing");
        let data = load_from(&dir.path().join("settings.json"));
        assert_eq!(data, Data::default());
    }

    #[test]
    fn settings_round_trip_including_the_window() {
        let dir = TempDir::new("settings-round-trip");
        let path = dir.path().join("settings.json");
        let data = Data {
            appearance: Appearance::Dark,
            socket: "/tmp/custom.sock".into(),
            window: Some(Window {
                x: 40,
                y: -200,
                width: 1100,
                height: 700,
            }),
        };
        save_to(&path, &data).expect("save");
        assert_eq!(load_from(&path), data);
        // A file from before the window was remembered still loads.
        std::fs::write(&path, br#"{"appearance":"light","socket":"/a.sock"}"#).expect("write");
        assert_eq!(
            load_from(&path),
            Data {
                appearance: Appearance::Light,
                socket: "/a.sock".into(),
                window: None,
            }
        );
    }

    #[test]
    fn an_interrupted_write_leaves_the_old_file() {
        let dir = TempDir::new("settings-interrupted");
        let path = dir.path().join("settings.json");
        let old = Data {
            appearance: Appearance::Dark,
            ..Data::default()
        };
        save_to(&path, &old).expect("save");

        // A crash part-way through a save leaves a temporary file beside the
        // preferences, holding half of the new ones.
        let half_written = path.with_file_name(format!("settings.json.{}.tmp", std::process::id()));
        std::fs::write(&half_written, br#"{"appearance":"light"#).expect("write");
        assert_eq!(load_from(&path), old, "the old preferences are whole");

        // The next save isn't blocked by it and leaves no new temporary file.
        let new = Data {
            appearance: Appearance::Light,
            ..Data::default()
        };
        save_to(&path, &new).expect("save");
        assert_eq!(load_from(&path), new);
        let new_litter: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .filter(|name| {
                let name = name.to_string_lossy();
                name.ends_with(".tmp")
                    && name != half_written.file_name().unwrap().to_string_lossy()
            })
            .collect();
        assert!(
            new_litter.is_empty(),
            "no new temp files left: {new_litter:?}"
        );
    }

    #[test]
    fn ordered_saves_never_revert_to_an_older_ticket() {
        let dir = TempDir::new("settings-ordered");
        let path = dir.path().join("settings.json");
        let older = next_save_ticket();
        let newer = next_save_ticket();
        let newest_data = Data {
            appearance: Appearance::Dark,
            socket: "/tmp/new.sock".into(),
            window: None,
        };
        let older_data = Data {
            appearance: Appearance::Light,
            socket: "/tmp/old.sock".into(),
            window: None,
        };
        // Even if the newer ticket hits disk first, the older one is skipped.
        save_ordered(&path, &newest_data, newer).expect("newer");
        save_ordered(&path, &older_data, older).expect("older skipped");
        assert_eq!(load_from(&path), newest_data);
    }

    #[test]
    fn a_save_that_cannot_finish_leaves_the_old_file_and_no_litter() {
        let dir = TempDir::new("settings-refused");
        let path = dir.path().join("settings.json");
        let old = Data {
            appearance: Appearance::Dark,
            ..Data::default()
        };
        save_to(&path, &old).expect("save");
        // A directory nobody may write to. Root may write anywhere, so the
        // save may go through after all; what must hold either way is that
        // the file is never left broken or beside a temporary file.
        let original = std::fs::metadata(dir.path())
            .expect("metadata")
            .permissions();
        let mut refused = original.clone();
        refused.set_readonly(true);
        std::fs::set_permissions(dir.path(), refused).expect("refuse writes");
        let result = save_to(
            &path,
            &Data {
                appearance: Appearance::Light,
                ..Data::default()
            },
        );
        std::fs::set_permissions(dir.path(), original).expect("allow writes again");

        let left = load_from(&path);
        if result.is_ok() {
            assert_eq!(left.appearance, Appearance::Light);
        } else {
            assert_eq!(left, old, "the old preferences are whole");
        }
        let litter: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            litter.is_empty(),
            "no temporary file left behind: {litter:?}"
        );
    }

    #[test]
    fn a_window_with_a_title_bar_opens_where_it_was_without_drifting() {
        let minimum = size(px(960.0), px(600.0));
        // The frame, title bar and all, and the content inside it.
        let frame = Bounds::new(point(px(116.0), px(128.0)), size(px(1280.0), px(793.0)));
        let content = size(px(1280.0), px(760.0));

        // What is kept is the frame's corner and the content's size, which
        // is what a window is opened from.
        let saved = Window::of(frame, content);
        assert_eq!(
            saved.bounds(minimum, &[display()]),
            Some(Bounds::new(point(px(116.0), px(128.0)), content))
        );
        // The window that comes up from that has the same frame and the same
        // content, so saving it again is the same entry: neither a title bar
        // taller, nor a title bar lower.
        assert_eq!(Window::of(frame, content), saved);
        assert_eq!((saved.width, saved.height), (1280, 760));

        // Where the frame is the content, as on a window system that draws
        // no title bar of its own, there is nothing to tell apart.
        let bare = Bounds::new(point(px(10.0), px(20.0)), content);
        assert_eq!(
            Window::of(bare, content).bounds(minimum, &[display()]),
            Some(bare)
        );
    }

    #[test]
    fn the_window_opens_where_it_was_left_when_that_is_still_a_place() {
        let left = Window {
            x: 100,
            y: 80,
            width: 1100,
            height: 700,
        };
        let minimum = size(px(layout::WINDOW_MIN.0), px(layout::WINDOW_MIN.1));
        assert_eq!(
            left.bounds(minimum, &[display()]),
            Some(Bounds::new(
                point(px(100.0), px(80.0)),
                size(px(1100.0), px(700.0))
            ))
        );
        // A display that was unplugged: its middle is off every screen.
        let gone = Window {
            x: 2500,
            y: 80,
            ..left
        };
        assert_eq!(gone.bounds(minimum, &[display()]), None);
        // Smaller than berth's minimum, as from an older build.
        let tiny = Window {
            width: 400,
            height: 300,
            ..left
        };
        assert_eq!(tiny.bounds(minimum, &[display()]), None);
        // Nowhere shown at all.
        assert_eq!(left.bounds(minimum, &[]), None);
    }

    #[test]
    fn restored_bounds_clamp_titlebar_and_oversized_windows_to_the_display() {
        let minimum = size(px(layout::WINDOW_MIN.0), px(layout::WINDOW_MIN.1));
        let laptop = Bounds::new(point(px(0.0), px(0.0)), size(px(1440.0), px(900.0)));

        // Titlebar dragged above the top of the display: center is still on screen,
        // so it reopens clamped down to the display's top edge.
        let high = Window {
            x: 100,
            y: -60,
            width: 1100,
            height: 700,
        };
        assert_eq!(
            high.bounds(minimum, &[laptop]),
            Some(Bounds::new(
                point(px(100.0), px(0.0)),
                size(px(1100.0), px(700.0))
            ))
        );

        // Saved large on an external display, reopened on a smaller laptop display:
        // shrinks to fit and stays inside the display.
        let huge = Window {
            x: 50,
            y: 50,
            width: 2000,
            height: 1200,
        };
        assert_eq!(
            huge.bounds(minimum, &[laptop]),
            Some(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(1440.0), px(900.0))
            ))
        );
    }
}
