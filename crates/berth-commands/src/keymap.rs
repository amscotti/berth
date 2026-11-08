//! One platform keymap model for the shell. Tests can install either dialect
//! on any host, which keeps macOS and Linux honest without separate runners.

use gpui::{App, KeyBinding};

use crate::app_actions::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Other
        }
    }
}

const MAC_TABS: [&str; 9] = [
    "cmd-1", "cmd-2", "cmd-3", "cmd-4", "cmd-5", "cmd-6", "cmd-7", "cmd-8", "cmd-9",
];
const OTHER_TABS: [&str; 9] = [
    "ctrl-1", "ctrl-2", "ctrl-3", "ctrl-4", "ctrl-5", "ctrl-6", "ctrl-7", "ctrl-8", "ctrl-9",
];

/// Install the application keybindings. Tab digits are one-based, matching the
/// rail: 1 is Overview through 9 is Runtimes. Settings deliberately has no
/// digit; ten views do not fit a single row of keys.
pub fn install(cx: &mut App, platform: Platform) {
    let (tabs, search, refresh, quit) = match platform {
        Platform::MacOs => (MAC_TABS, "cmd-f", "cmd-r", "cmd-q"),
        Platform::Other => (OTHER_TABS, "ctrl-f", "ctrl-r", "ctrl-q"),
    };

    let bindings = vec![
        KeyBinding::new(tabs[0], ShowOverview, None),
        KeyBinding::new(tabs[1], ShowContainers, None),
        KeyBinding::new(tabs[2], ShowImages, None),
        KeyBinding::new(tabs[3], ShowVolumes, None),
        KeyBinding::new(tabs[4], ShowNetworks, None),
        KeyBinding::new(tabs[5], ShowEvents, None),
        KeyBinding::new(tabs[6], ShowStacks, None),
        KeyBinding::new(tabs[7], ShowDisk, None),
        KeyBinding::new(tabs[8], ShowRuntimes, None),
        KeyBinding::new(search, FocusSearch, None),
        KeyBinding::new(refresh, Refresh, None),
        KeyBinding::new(quit, Quit, None),
        KeyBinding::new("escape", Dismiss, None),
        KeyBinding::new("down", SelectNext, Some("Berth")),
        KeyBinding::new("up", SelectPrev, Some("Berth")),
    ];
    cx.bind_keys(bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_with_a_digit_has_a_binding() {
        assert_eq!(MAC_TABS.len(), 9);
        assert_eq!(OTHER_TABS.len(), 9);
        assert_eq!(MAC_TABS[0], "cmd-1");
        assert_eq!(OTHER_TABS[0], "ctrl-1");
    }
}
