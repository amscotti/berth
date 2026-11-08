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
