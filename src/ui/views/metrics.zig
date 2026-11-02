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

/// Name over image, the way Docker Desktop reads a row: the two together
/// identify the container and only one column is spent doing it.
pub fn nameCell(c: *ui.Context, ct: *const api.Container, width: f32) void {
    const box = ui.div(c).column().w(width).justifyCenter().gap(0);
    defer box.end();
    ui.text(c, ct.name(), .{ .size = 12.5, .weight = .medium, .truncate = .ellipsis });
    ui.text(c, ct.Image, .{ .size = 10, .color = c.theme.muted, .truncate = .ellipsis });
}

/// Two lines in one 44pt row: received over sent, read over written. The
/// figures are lifetime totals, exactly what `docker stats` prints. All text
/// goes through the frame arena — a stack buffer dies before paint.
pub fn netCell(c: *ui.Context, ct: *const api.Container, st: app.Stat, width: f32) void {
    if (!ct.isUp() or !st.sampled) {
        widgets.cellMuted(c, "—", width, false);
        return;
    }
    const box = ui.div(c).column().w(width).justifyCenter().gap(0);
    defer box.end();
    pair(c, c.print("↓ {s}", .{format.bytes(c, st.net_rx)}), c.print("↑ {s}", .{format.bytes(c, st.net_tx)}));
}

pub fn diskCell(c: *ui.Context, ct: *const api.Container, st: app.Stat, width: f32) void {
    if (!ct.isUp() or !st.sampled) {
        widgets.cellMuted(c, "—", width, false);
        return;
    }
    const box = ui.div(c).column().w(width).justifyCenter().gap(0);
    defer box.end();
    pair(c, c.print("{s} read", .{format.bytes(c, st.blk_read)}), c.print("{s} wrote", .{format.bytes(c, st.blk_write)}));
}

fn pair(c: *ui.Context, top: []const u8, bottom: []const u8) void {
    ui.text(c, top, .{ .size = 10, .color = c.theme.text, .tabular = true, .truncate = .ellipsis });
    ui.text(c, bottom, .{ .size = 10, .color = c.theme.muted, .tabular = true, .truncate = .ellipsis });
}

pub fn pidsCell(c: *ui.Context, ct: *const api.Container, st: app.Stat, width: f32) void {
    if (!ct.isUp() or !st.sampled) {
        widgets.cellMuted(c, "—", width, true);
        return;
    }
    // A missing controller is unknown, not zero: a live zero is a real zero.
    if (st.pids) |n| {
        widgets.cell(c, c.print("{d}", .{n}), width, true);
    } else {
        widgets.cellMuted(c, "—", width, true);
    }
}

pub fn uptimeCell(c: *ui.Context, ct: *const api.Container, width: f32) void {
    if (!ct.isUp()) {
        widgets.cellMuted(c, "—", width, false);
        return;
    }
    // "Restarting" is up in the docker-ps sense but has no uptime to read;
    // a dash is honest, "up" would not be.
    const text = if (lifetime.upSeconds(ct.Status)) |secs|
        format.duration(c, secs)
    else
        "—";
    widgets.cellMuted(c, text, width, false);
}

/// The sort arms for the new keys; shared with the comparator in lists.zig.
pub fn statKey(s: *app.State, key: u8, ct: *const api.Container) f64 {
    const st = app.statFor(s, ct.Id);
    return switch (key) {
        7 => if (st.sampled) @floatFromInt(st.net_rx + st.net_tx) else -1,
        8 => if (st.sampled) @floatFromInt(st.blk_read + st.blk_write) else -1,
        9 => if (st.pids) |n| @floatFromInt(n) else -1,
        10 => if (lifetime.upSeconds(ct.Status)) |secs| @floatFromInt(secs) else -1,
        else => 0,
    };
}
