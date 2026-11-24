//! The native menu model. The app installs the handlers; this module only
//! describes the menus.

use gpui::{Menu, MenuItem};

use crate::app_actions::*;

pub fn menus() -> [Menu; 3] {
    [
        Menu::new("berth").items([MenuItem::action("Quit berth", Quit)]),
        Menu::new("View").items([
            MenuItem::action("Overview", ShowOverview),
            MenuItem::action("Containers", ShowContainers),
            MenuItem::action("Images", ShowImages),
            MenuItem::action("Volumes", ShowVolumes),
            MenuItem::action("Networks", ShowNetworks),
            MenuItem::action("Events", ShowEvents),
            MenuItem::action("Stacks", ShowStacks),
            MenuItem::separator(),
            MenuItem::action("Disk", ShowDisk),
            MenuItem::action("Runtimes", ShowRuntimes),
            MenuItem::action("Settings", ShowSettings),
            MenuItem::separator(),
            MenuItem::action("Refresh", Refresh),
        ]),
        Menu::new("Window").items([MenuItem::action("Close", Dismiss)]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_expose_application_view_and_window_entries() {
        let bar = menus();
        assert_eq!(bar.len(), 3);
        assert_eq!(bar[0].name.as_ref(), "berth");
        assert_eq!(bar[0].items.len(), 1);
        assert_eq!(bar[1].name.as_ref(), "View");
        // Ten views, Refresh, and two separators.
        assert_eq!(bar[1].items.len(), 13);
        assert_eq!(bar[2].name.as_ref(), "Window");
        assert_eq!(bar[2].items.len(), 1);
    }
}
