//! Bundled SVG icons. `include_bytes!` keeps the binary independent of the
//! working directory; GPUI's `svg()` resolves them through this source.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct Assets;

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        fn bytes(path: &str) -> Option<&'static [u8]> {
            match path {
                $(concat!("icons/", $name, ".svg") => Some(include_bytes!(concat!("../../../assets/icons/", $name, ".svg"))),)*
                _ => None,
            }
        }
    };
}

icons!(
    "overview",
    "containers",
    "images",
    "volumes",
    "networks",
    "events",
    "stacks",
    "disk",
    "runtimes",
    "settings",
    "cpu",
    "memory",
    "running",
    "host",
    "sparkles",
    "plus",
    "xmark",
);

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(bytes(path).map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
