//! Saved preferences: appearance, the last chosen socket and where the window
//! was, in the platform data directory. A small JSON file, because the app
//! owns its own storage on both macOS and Linux.
//!
//! The file is replaced in one step or not at all. It is written beside
//! itself and renamed over itself, so an interrupted save leaves the old
//! preferences whole rather than half of the new ones.

use std::path::{Path, PathBuf};

use berth_ui::layout;
use gpui::{App, Bounds, Pixels, Size, point, px, size};
use serde::{Deserialize, Serialize};

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
    pub fn of(bounds: Bounds<Pixels>) -> Self {
        let round = |value: Pixels| f32::from(value).round();
        Self {
            x: round(bounds.origin.x) as i32,
            y: round(bounds.origin.y) as i32,
            width: round(bounds.size.width).max(0.0) as u32,
            height: round(bounds.size.height).max(0.0) as u32,
        }
    }

    /// The window as it was saved, if that is a window berth can open: no
    /// smaller than its minimum, and with its middle on a display, so it is
    /// somewhere a person can find it. A display unplugged since, or a size
    /// from an older build, puts it back in the usual place.
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
        displays
            .iter()
            .any(|display| display.contains(&middle))
            .then_some(bounds)
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

pub fn save_to(path: &Path, data: &Data) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(data)?;
    // Beside the file, on the same volume, so the rename that follows is one
    // step and not a copy.
    let temporary = path.with_file_name(format!(
        "{}.{}.tmp",
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

        // A crash part-way through a save leaves berth's temporary file
        // beside the preferences, holding half of the new ones.
        let half_written = path.with_file_name(format!("settings.json.{}.tmp", std::process::id()));
        std::fs::write(&half_written, br#"{"appearance":"light"#).expect("write");
        assert_eq!(load_from(&path), old, "the old preferences are whole");

        // The next save isn't blocked by it, and leaves nothing behind.
        let new = Data {
            appearance: Appearance::Light,
            ..Data::default()
        };
        save_to(&path, &new).expect("save");
        assert_eq!(load_from(&path), new);
        assert!(!half_written.exists(), "the temporary file went with it");
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
}
