//! Saved preferences: appearance and the last chosen socket, in the platform
//! data directory. A small JSON file, because the app owns its own storage on
//! both macOS and Linux.

use std::path::{Path, PathBuf};

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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub appearance: Appearance,
    pub socket: String,
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

pub fn save(data: &Data) -> std::io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    save_to(&path, data)
}

pub fn load_from(path: &Path) -> Data {
    let Ok(bytes) = std::fs::read(path) else {
        return Data::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_to(path: &Path, data: &Data) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(data).unwrap_or_else(|_| b"{}".to_vec());
    std::fs::write(path, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_defaults_to_system() {
        let path = std::env::temp_dir().join("berth-settings-missing.json");
        let _ = std::fs::remove_file(&path);
        let data = load_from(&path);
        assert_eq!(data.appearance, Appearance::System);
        assert!(data.socket.is_empty());
    }

    #[test]
    fn settings_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "berth-settings-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let data = Data {
            appearance: Appearance::Dark,
            socket: "/tmp/custom.sock".into(),
        };
        save_to(&path, &data).expect("save");
        let back = load_from(&path);
        assert_eq!(back, data);
        let _ = std::fs::remove_file(path);
    }
}
