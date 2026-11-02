//! The window's breakpoints, in one place. The rail, the inspector and every
//! table read the same widths, so a resize cannot leave one of them believing
//! there is more room than there is.
const std = @import("std");

pub const Widths = struct {
    /// The navigation rail: labels at normal widths, icons alone when small.
    rail: f32 = 200,
    /// The inspector, or zero when nothing is selected.
    pane: f32 = 0,
    /// What a view has to draw its tables in: the window, minus the rail, the
    /// main column's padding and the inspector.
    content: f32 = 940,
    /// The rail has dropped its labels.
    narrow_rail: bool = false,
};

pub const MAIN_PADDING = 40;
pub const RAIL = 200;
pub const RAIL_NARROW = 88;

/// Below this the rail folds to icons. The inspector is fixed-width, so the
/// rail is where the room comes from when both cannot fit.
const RAIL_BREAK = 1120;
/// The inspector is comfortable at 430; below the default window it gives the
/// table back 70 points rather than squeezing both.
const PANE_WIDE = 430;
const PANE_NARROW = 360;
const PANE_BREAK = 1180;

pub fn of(window: f32, pane_open: bool) Widths {
    const narrow = window < RAIL_BREAK;
    const rail: f32 = if (narrow) RAIL_NARROW else RAIL;
    const pane: f32 = if (!pane_open) 0 else if (window >= PANE_BREAK) PANE_WIDE else PANE_NARROW;
    return .{
        .rail = rail,
        .pane = pane,
        .content = @max(280, window - rail - MAIN_PADDING - pane),
        .narrow_rail = narrow,
    };
}

test "the smallest window keeps room for a table beside the inspector" {
    const small = of(960, true);
    try std.testing.expectEqual(@as(f32, 88), small.rail);
    try std.testing.expectEqual(@as(f32, 360), small.pane);
    try std.testing.expectEqual(@as(f32, 472), small.content);

    const wide = of(1440, true);
    try std.testing.expectEqual(@as(f32, 200), wide.rail);
    try std.testing.expectEqual(@as(f32, 430), wide.pane);
    try std.testing.expectEqual(@as(f32, 770), wide.content);

    const alone = of(1180, false);
    try std.testing.expectEqual(@as(f32, 940), alone.content);
    try std.testing.expect(!alone.narrow_rail);

    // The default window with the inspector: the compact table's 510 points.
    const default_open = of(1280, true);
    try std.testing.expectEqual(@as(f32, 610), default_open.content);
    const old_default_open = of(1180, true);
    try std.testing.expectEqual(@as(f32, 510), old_default_open.content);
    // Just below the rail break, the icon rail gives the table more room.
    const nearly = of(1119, true);
    try std.testing.expectEqual(@as(f32, 88), nearly.rail);
    try std.testing.expectEqual(@as(f32, 631), nearly.content);
}
