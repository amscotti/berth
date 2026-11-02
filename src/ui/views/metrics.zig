//! The container table's live columns: network and disk I/O as two stacked
//! figures, PIDs, and uptime. Kept out of `lists.zig` so that file stays
//! inside the 600-line rule, and so the widths live beside the painters that
//! must agree with them.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const lifetime = @import("../../engine/lifetime.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");

const with_project = [_]widgets.Col{
    .{ .name = "NAME", .width = 124, .key = 1 },
    .{ .name = "IMAGE", .width = 104, .key = 2 },
    .{ .name = "STATE", .width = 70, .key = 3 },
    .{ .name = "CPU", .width = 54, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 70, .align_end = true, .key = 5 },
    .{ .name = "NET", .width = 84, .key = 7 },
    .{ .name = "DISK", .width = 84, .key = 8 },
    .{ .name = "PROJECT", .width = 90, .key = 6 },
    .{ .name = "ACTIONS", .width = 150 },
};

const without_project = [_]widgets.Col{
    .{ .name = "NAME", .width = 124, .key = 1 },
    .{ .name = "IMAGE", .width = 104, .key = 2 },
    .{ .name = "STATE", .width = 70, .key = 3 },
    .{ .name = "CPU", .width = 54, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 70, .align_end = true, .key = 5 },
    .{ .name = "NET", .width = 84, .key = 7 },
    .{ .name = "DISK", .width = 84, .key = 8 },
    .{ .name = "PIDS", .width = 40, .align_end = true, .key = 9 },
    .{ .name = "UPTIME", .width = 60, .key = 10 },
    .{ .name = "ACTIONS", .width = 150 },
};

/// The full set, with room for the live figures; a Compose project trades back
/// the two least-read columns for its own, because the window does not grow.
pub fn fullCols(any_project: bool) []const widgets.Col {
    return if (any_project) with_project[0..] else without_project[0..];
}

/// Two lines in one 44pt row: received over sent, read over written. The
/// figures are lifetime totals, exactly what `docker stats` prints. All text
/// goes through the frame arena — a stack buffer dies before paint.
pub fn netCell(c: *ui.Context, ct: *const api.Container, st: app.Stat) void {
    const box = ui.div(c).column().w(84).justifyCenter().gap(0);
    defer box.end();
    if (!ct.isUp()) {
        pair(c, "—", "—");
        return;
    }
    pair(c, c.print("↓ {s}", .{format.bytes(c, st.net_rx)}), c.print("↑ {s}", .{format.bytes(c, st.net_tx)}));
}

pub fn diskCell(c: *ui.Context, ct: *const api.Container, st: app.Stat) void {
    const box = ui.div(c).column().w(84).justifyCenter().gap(0);
    defer box.end();
    if (!ct.isUp()) {
        pair(c, "—", "—");
        return;
    }
    pair(c, c.print("{s} read", .{format.bytes(c, st.blk_read)}), c.print("{s} wrote", .{format.bytes(c, st.blk_write)}));
}

fn pair(c: *ui.Context, top: []const u8, bottom: []const u8) void {
    ui.text(c, top, .{ .size = 10, .color = c.theme.text, .tabular = true, .truncate = .ellipsis });
    ui.text(c, bottom, .{ .size = 10, .color = c.theme.muted, .tabular = true, .truncate = .ellipsis });
}

pub fn pidsCell(c: *ui.Context, ct: *const api.Container, st: app.Stat) void {
    if (!ct.isUp()) {
        widgets.cellMuted(c, "—", 40, true);
        return;
    }
    // A missing controller is unknown, not zero: a live zero is a real zero.
    if (st.pids) |n| {
        widgets.cell(c, c.print("{d}", .{n}), 40, true);
    } else {
        widgets.cellMuted(c, "—", 40, true);
    }
}

pub fn uptimeCell(c: *ui.Context, ct: *const api.Container) void {
    if (!ct.isUp()) {
        widgets.cellMuted(c, "—", 60, false);
        return;
    }
    const text = if (lifetime.upSeconds(ct.Status)) |secs|
        format.duration(c, secs)
    else
        "up";
    widgets.cellMuted(c, text, 60, false);
}

/// The sort arms for the new keys; shared with the comparator in lists.zig.
pub fn statKey(s: *app.State, key: u8, ct: *const api.Container) f64 {
    const st = app.statFor(s, ct.Id);
    return switch (key) {
        7 => @floatFromInt(st.net_rx + st.net_tx),
        8 => @floatFromInt(st.blk_read + st.blk_write),
        9 => if (st.pids) |n| @floatFromInt(n) else -1,
        10 => if (lifetime.upSeconds(ct.Status)) |secs| @floatFromInt(secs) else -1,
        else => 0,
    };
}
