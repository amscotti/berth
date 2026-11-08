//! Typed application actions. The app installs one handler per action; the
//! keymap and the native menu both refer to these types.

use gpui::actions;

actions!(
    berth_app,
    [
        Quit,
        Refresh,
        ShowOverview,
        ShowContainers,
        ShowImages,
        ShowVolumes,
        ShowNetworks,
        ShowEvents,
        ShowStacks,
        ShowDisk,
        ShowRuntimes,
        ShowSettings,
        FocusSearch,
        Dismiss,
        SelectNext,
        SelectPrev,
    ]
);
