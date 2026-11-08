//! Window and table geometry, in one place.
//!
//! The rail, the inspector and every table read the same widths, so a resize
//! cannot leave one of them believing there is more room than there is. The
//! values come from the Zig app's design contract.

/// The rail at normal widths.
pub const RAIL: f32 = 200.0;
/// The rail when it folds to icons.
pub const RAIL_NARROW: f32 = 88.0;
/// Below this the rail folds. The inspector is fixed-width, so the rail is
/// where the room comes from when both cannot fit.
pub const RAIL_BREAK: f32 = 1120.0;

/// The inspector at a comfortable window width.
pub const PANE_WIDE: f32 = 430.0;
/// The inspector when the window gives some room back to the table.
pub const PANE_NARROW: f32 = 360.0;
/// Below this the inspector gives the table 70 points.
pub const PANE_BREAK: f32 = 1180.0;

/// Left and right page padding.
pub const MAIN_PADDING: f32 = 40.0;
/// The smallest content width a table is expected to draw in.
pub const MIN_CONTENT: f32 = 280.0;

/// The window's default and minimum content sizes.
pub const WINDOW_DEFAULT: (f32, f32) = (1280.0, 760.0);
pub const WINDOW_MIN: (f32, f32) = (960.0, 600.0);

/// Table row heights.
pub const ROW_CONTAINER: f32 = 44.0;
pub const ROW_EVENT: f32 = 34.0;
/// Table header height and the rule beneath it.
pub const TABLE_HEADER: f32 = 18.0;

/// Cards and graph wells.
pub const CARD_RADIUS: f32 = 10.0;
pub const CARD_PADDING: f32 = 16.0;
pub const GRAPH_HEIGHT: f32 = 70.0;
pub const SHEET_MAX_WIDTH: f32 = 620.0;

/// The placed page within a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Widths {
    pub rail: f32,
    pub pane: f32,
    /// The window, minus the rail, the main column's padding and the inspector.
    pub content: f32,
    pub narrow_rail: bool,
}

/// Resolve the rail, inspector and content widths for a window.
pub fn for_window(window: f32, pane_open: bool) -> Widths {
    let narrow = window < RAIL_BREAK;
    let rail = if narrow { RAIL_NARROW } else { RAIL };
    let pane = if !pane_open {
        0.0
    } else if window >= PANE_BREAK {
        PANE_WIDE
    } else {
        PANE_NARROW
    };
    Widths {
        rail,
        pane,
        content: (window - rail - MAIN_PADDING - pane).max(MIN_CONTENT),
        narrow_rail: narrow,
    }
}

/// Which columns a container table keeps at a given content width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Full,
    Medium,
    Dense,
    Compact,
    Tiny,
}

impl Density {
    pub fn label(self) -> &'static str {
        match self {
            Density::Full => "full",
            Density::Medium => "medium",
            Density::Dense => "dense",
            Density::Compact => "compact",
            Density::Tiny => "tiny",
        }
    }
}

/// The tiers are named for what they keep, not for a device: the default
/// window has room for every column; a small one gives up the least-used
/// figures first.
pub fn density(content: f32) -> Density {
    if content >= 1040.0 {
        Density::Full
    } else if content >= 832.0 {
        Density::Medium
    } else if content >= 656.0 {
        Density::Dense
    } else if content >= 510.0 {
        Density::Compact
    } else {
        Density::Tiny
    }
}

/// Column widths for one table density. The name column takes the window's
/// spare width up to a readable limit; a wide table then reads as a table, not
/// as columns with a hole in them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContainerColumns {
    pub density: Density,
    pub name: f32,
    pub status: f32,
    pub cpu: f32,
    pub mem: f32,
    pub net: f32,
    pub disk: f32,
    pub pids: f32,
    pub uptime: f32,
    pub ports: f32,
    pub actions: f32,
    pub show_mem: bool,
    pub show_net: bool,
    pub show_disk: bool,
    pub show_pids: bool,
    pub show_uptime: bool,
    pub show_ports: bool,
}

impl ContainerColumns {
    /// The width the row asks for, including padding and the gaps between
    /// columns. The name column already includes any spare it took.
    pub fn row_width(&self) -> f32 {
        let mut widths = vec![self.name, self.status, self.cpu, self.actions];
        if self.show_mem {
            widths.push(self.mem);
        }
        if self.show_net {
            widths.push(self.net);
        }
        if self.show_disk {
            widths.push(self.disk);
        }
        if self.show_pids {
            widths.push(self.pids);
        }
        if self.show_uptime {
            widths.push(self.uptime);
        }
        if self.show_ports {
            widths.push(self.ports);
        }
        let gaps = (widths.len().saturating_sub(1)) as f32 * 10.0;
        widths.iter().sum::<f32>() + gaps + 24.0
    }
}

/// Resolve the container table's columns.
///
/// `budget` is the content width. `any_project` is reserved for the Compose
/// column; the initial shell always passes `false`.
pub fn container_columns(content: f32, _any_project: bool) -> ContainerColumns {
    let density = density(content);
    let mut columns = match density {
        Density::Full => ContainerColumns {
            density,
            name: 170.0,
            status: 66.0,
            cpu: 50.0,
            mem: 62.0,
            net: 82.0,
            disk: 82.0,
            pids: 38.0,
            uptime: 54.0,
            ports: 140.0,
            actions: 154.0,
            show_mem: true,
            show_net: true,
            show_disk: true,
            show_pids: true,
            show_uptime: true,
            show_ports: true,
        },
        Density::Medium => ContainerColumns {
            density,
            name: 156.0,
            status: 62.0,
            cpu: 46.0,
            mem: 56.0,
            net: 72.0,
            disk: 72.0,
            pids: 0.0,
            uptime: 0.0,
            ports: 112.0,
            actions: 154.0,
            show_mem: true,
            show_net: true,
            show_disk: true,
            show_pids: false,
            show_uptime: false,
            show_ports: true,
        },
        Density::Dense => ContainerColumns {
            density,
            name: 146.0,
            status: 64.0,
            cpu: 48.0,
            mem: 60.0,
            net: 0.0,
            disk: 0.0,
            pids: 0.0,
            uptime: 0.0,
            ports: 110.0,
            actions: 154.0,
            show_mem: true,
            show_net: false,
            show_disk: false,
            show_pids: false,
            show_uptime: false,
            show_ports: true,
        },
        Density::Compact => ContainerColumns {
            density,
            name: 120.0,
            status: 64.0,
            cpu: 48.0,
            mem: 60.0,
            net: 0.0,
            disk: 0.0,
            pids: 0.0,
            uptime: 0.0,
            ports: 0.0,
            actions: 154.0,
            show_mem: true,
            show_net: false,
            show_disk: false,
            show_pids: false,
            show_uptime: false,
            show_ports: false,
        },
        Density::Tiny => ContainerColumns {
            density,
            name: 132.0,
            status: 64.0,
            cpu: 48.0,
            mem: 0.0,
            net: 0.0,
            disk: 0.0,
            pids: 0.0,
            uptime: 0.0,
            ports: 0.0,
            actions: 154.0,
            show_mem: false,
            show_net: false,
            show_disk: false,
            show_pids: false,
            show_uptime: false,
            show_ports: false,
        },
    };

    let spare = (content - columns.row_width()).clamp(0.0, 110.0);
    columns.name += spare;
    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_smallest_window_keeps_room_for_a_table_beside_the_inspector() {
        let small = for_window(960.0, true);
        assert_eq!(small.rail, RAIL_NARROW);
        assert_eq!(small.pane, PANE_NARROW);
        assert_eq!(small.content, 472.0);

        let wide = for_window(1440.0, true);
        assert_eq!(wide.rail, RAIL);
        assert_eq!(wide.pane, PANE_WIDE);
        assert_eq!(wide.content, 770.0);
    }

    #[test]
    fn the_default_window_has_every_container_column() {
        let widths = for_window(WINDOW_DEFAULT.0, false);
        assert_eq!(widths.content, 1040.0);
        let columns = container_columns(widths.content, false);
        assert_eq!(columns.density, Density::Full);
        assert!(columns.show_net && columns.show_ports);
        assert!(columns.row_width() <= 1040.0);
        assert_eq!(columns.name, 198.0); // 170 + the 28 points of spare
    }

    #[test]
    fn the_narrow_window_folds_the_rail_and_drops_pids_and_uptime() {
        let widths = for_window(960.0, false);
        assert!(widths.narrow_rail);
        assert_eq!(widths.content, 832.0);
        let columns = container_columns(widths.content, false);
        assert_eq!(columns.density, Density::Medium);
        assert!(!columns.show_pids && !columns.show_uptime);
        assert!(columns.show_ports);
        assert!(columns.row_width() <= 832.0);
    }

    #[test]
    fn every_density_fits_the_width_that_chose_it() {
        for content in [1040.0, 832.0, 770.0, 510.0, 472.0] {
            let columns = container_columns(content, false);
            assert!(
                columns.row_width() <= content,
                "{} columns asked for {} in {content}",
                columns.density.label(),
                columns.row_width()
            );
        }
    }

    #[test]
    fn the_tiny_table_gives_up_memory_and_ports() {
        let columns = container_columns(472.0, false);
        assert_eq!(columns.density, Density::Tiny);
        assert!(!columns.show_mem);
        assert!(!columns.show_ports);
        assert_eq!(columns.ports, 0.0);
    }
}
