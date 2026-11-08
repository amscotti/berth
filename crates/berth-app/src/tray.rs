//! The menu-bar mini-monitor. macOS only; other platforms get a no-op.

#[cfg(target_os = "macos")]
mod imp {
    use tray_icon::TrayIcon;
    use tray_icon::TrayIconBuilder;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};

    pub struct Tray {
        icon: TrayIcon,
        open: MenuItem,
        quit: MenuItem,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Event {
        Open,
        Quit,
    }

    pub fn create() -> Option<Tray> {
        let open = MenuItem::new("Open berth", true, None);
        let quit = MenuItem::new("Quit berth", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit])
            .ok()?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("berth")
            .with_title("berth")
            .build()
            .ok()?;
        Some(Tray { icon, open, quit })
    }

    pub fn set_title(tray: &Tray, title: &str) {
        tray.icon.set_title(Some(title));
    }

    pub fn drain(tray: &Tray) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == tray.open.id() {
                events.push(Event::Open);
            } else if event.id == tray.quit.id() {
                events.push(Event::Quit);
            }
        }
        events
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub struct Tray;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[allow(dead_code)]
    pub enum Event {
        Open,
        Quit,
    }
    pub fn create() -> Option<Tray> {
        None
    }
    pub fn set_title(_tray: &Tray, _title: &str) {}
    pub fn drain(_tray: &Tray) -> Vec<Event> {
        Vec::new()
    }
}

pub use imp::{Event, Tray, create, drain, set_title};
