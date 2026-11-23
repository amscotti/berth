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

/// The CPU column, at every density. It has to hold a figure such as `99.9%`
/// or `1235%`: at its old 46 points the 960-wide window showed `99…`.
pub const CPU_COLUMN: f32 = 58.0;
/// The short name the column tables use for [`CPU_COLUMN`].
const CPU: f32 = CPU_COLUMN;

/// The window's default and minimum content sizes.
pub const WINDOW_DEFAULT: (f32, f32) = (1280.0, 760.0);
pub const WINDOW_MIN: (f32, f32) = (960.0, 600.0);

/// Table row heights.
pub const ROW_CONTAINER: f32 = 44.0;
pub const ROW_EVENT: f32 = 34.0;
/// Table header height and the rule beneath it.
pub const TABLE_HEADER: f32 = 18.0;
/// The column of checkboxes a table shows while several rows can be picked.
pub const BULK_COLUMN: f32 = 28.0;
/// Room inside a row, and between its columns.
pub const ROW_PADDING: f32 = 12.0;
pub const ROW_GAP: f32 = 10.0;
/// Room after a figure set against the right edge of its column.
pub const CELL_PADDING_END: f32 = 8.0;
/// A status pill: its height, and the room inside it.
pub const PILL_HEIGHT: f32 = 20.0;
pub const PILL_PADDING: f32 = 8.0;
/// An item of the rail, with its label, and folded to an icon.
pub const RAIL_ITEM: f32 = 32.0;
pub const RAIL_ITEM_COMPACT: f32 = 36.0;

/// Cards and graph wells.
pub const CARD_RADIUS: f32 = 10.0;
pub const CARD_PADDING: f32 = 16.0;
/// Between two cards side by side, or stacked.
pub const CARD_GAP: f32 = 12.0;
/// A recessed well: logs, raw JSON, a graph.
pub const RADIUS_WELL: f32 = 8.0;
pub const GRAPH_HEIGHT: f32 = 70.0;
pub const SHEET_MAX_WIDTH: f32 = 620.0;

/// Control heights. A control is one of these two and nothing else: a pill,
/// a chip, a checkbox row or a button.
pub const CONTROL_SMALL: f32 = 24.0;
pub const CONTROL: f32 = 28.0;
/// A dialog's buttons share one width and one baseline.
pub const DIALOG_BUTTON_WIDTH: f32 = 92.0;
/// The inspector: its padding, and the column its facts are labelled in.
pub const INSPECTOR_PADDING: f32 = 14.0;
pub const FACT_LABEL: f32 = 64.0;

/// Corner radii. A pill's is half its height; everything else is one of these.
/// A table row.
pub const RADIUS_ROW: f32 = 4.0;
/// A well nested inside a card.
pub const RADIUS_NESTED: f32 = 6.0;
/// An item of the rail.
pub const RADIUS_ITEM: f32 = 7.0;
/// The square of tint behind an icon: its side, and its corner.
pub const CHIP: f32 = 20.0;
pub const RADIUS_CHIP: f32 = 5.0;

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
    /// How many columns are on screen. Each carries a sortable header.
    pub fn header_count(&self) -> usize {
        [
            self.show_mem,
            self.show_net,
            self.show_disk,
            self.show_pids,
            self.show_uptime,
            self.show_ports,
        ]
        .iter()
        .filter(|show| **show)
        .count()
            // Name, status and CPU are always there.
            + 3
    }

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
        let gaps = (widths.len().saturating_sub(1)) as f32 * ROW_GAP;
        widths.iter().sum::<f32>() + gaps + 2.0 * ROW_PADDING
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
            name: 162.0,
            status: 66.0,
            cpu: CPU,
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
            name: 148.0,
            status: 62.0,
            cpu: CPU,
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
            name: 136.0,
            status: 64.0,
            cpu: CPU,
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
            name: 110.0,
            status: 64.0,
            cpu: CPU,
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
            name: 124.0,
            status: 64.0,
            cpu: CPU,
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
        assert_eq!(columns.name, 190.0); // 162 + the 28 points of spare
        assert_eq!(columns.header_count(), 9);
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
        assert_eq!(columns.header_count(), 7);
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
    fn the_cpu_column_fits_a_five_character_figure_at_every_density() {
        // `99.9%` at the 13-point body size measures about 41 points; with
        // the 8 points of right padding it needs 49. Leave a margin.
        for content in [1040.0, 832.0, 770.0, 656.0, 510.0, 472.0] {
            let columns = container_columns(content, false);
            assert!(
                columns.cpu >= 58.0,
                "{} cpu was {}",
                columns.density.label(),
                columns.cpu
            );
            assert!(
                columns.row_width() <= content,
                "{} does not fit",
                columns.density.label()
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
