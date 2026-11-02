//! Shared pieces every view is built from, so a row looks like a row whether
//! it lists containers, images or volumes.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const format = @import("format.zig");

pub const muted = 12.5;
pub const body = 13.0;
pub const title = 22.0;

/// "1 image", "2 images": a count that reads like a sentence, not a label.
pub fn count(c: *ui.Context, n: usize, one: []const u8, many: []const u8) []const u8 {
    return c.print("{d} {s}", .{ n, if (n == 1) one else many });
}

/// Title, count and the one primary action, in the same order on every page.
pub fn pageHeader(c: *ui.Context, heading: []const u8, subtitle: []const u8) void {
    const h = ui.div(c).row().wFull().itemsCenter().gap(10).pb(12);
    defer h.end();
    ui.text(c, heading, .{ .size = title, .weight = .semibold });
    if (subtitle.len > 0) ui.text(c, subtitle, .{ .size = muted, .color = c.theme.muted });
    ui.spacer(c);
}

/// The column strip above a list. Widths are fixed so numbers line up; the
/// last column takes the rest.
pub const Col = struct {
    name: []const u8,
    width: f32,
    align_end: bool = false,
    /// The sort key this header sets; 0 means the column is not sortable.
    key: u8 = 0,
    /// A column that exists in the layout but is not drawn this frame.
    skip: bool = false,
};

/// Which column a table is ordered by, and which way.
pub const Sort = struct {
    key: u8 = 0,
    desc: bool = false,

    /// Pick a column: a new one starts ascending, the same one flips.
    pub fn pick(self: *Sort, key: u8) void {
        if (self.key == key) {
            self.desc = !self.desc;
        } else {
            self.key = key;
            self.desc = false;
        }
    }
};

pub fn tableHeader(c: *ui.Context, cols: []const Col, sort: ?*Sort) void {
    const row = ui.div(c).row().wFull().itemsCenter().px(12).pb(6).gap(8);
    for (cols) |col| {
        if (col.skip) continue;
        var head_cell = ui.div(c).row().w(col.width).h(18).itemsCenter().gap(3).control();
        if (col.align_end) head_cell = head_cell.justifyEnd();
        defer head_cell.end();

        const active = col.key != 0 and sort != null and sort.?.key == col.key;
        ui.text(c, col.name, .{
            .size = 11,
            .weight = .medium,
            .color = if (active) c.theme.accent else c.theme.muted,
        });
        if (active) ui.text(c, if (sort.?.desc) "▼" else "▲", .{ .size = 8, .color = c.theme.accent });

        if (col.key != 0 and sort != null and head_cell.clicked()) sort.?.pick(col.key);
    }
    row.end();
    const rule = ui.div(c).wFull().h(1).bg(c.theme.border);
    rule.end();
}

/// One cell of a row: text clipped to its column, numbers right-aligned and
/// tabular so digits stack vertically.
pub fn cell(c: *ui.Context, text: []const u8, width: f32, align_end: bool) void {
    const box = ui.div(c).column().w(width).h(20).justifyCenter().itemsStart();
    defer box.end();
    if (align_end) {
        const inner = ui.div(c).row().wFull().h(20).itemsCenter().justifyEnd();
        defer inner.end();
        ui.text(c, text, .{ .size = body, .tabular = true, .truncate = .ellipsis });
    } else {
        ui.text(c, text, .{ .size = body, .truncate = .ellipsis });
    }
}

pub fn cellMuted(c: *ui.Context, text: []const u8, width: f32, align_end: bool) void {
    const box = ui.div(c).column().w(width).h(20).justifyCenter().itemsStart();
    defer box.end();
    if (align_end) {
        const inner = ui.div(c).row().wFull().h(20).itemsCenter().justifyEnd();
        defer inner.end();
        ui.text(c, text, .{ .size = body, .tabular = true, .color = c.theme.muted, .truncate = .ellipsis });
    } else {
        ui.text(c, text, .{ .size = body, .color = c.theme.muted, .truncate = .ellipsis });
    }
}

/// A pill inside a fixed-width box. A bare pill is as wide as its word, which
/// shifts every column after it out of line with the header.
pub fn pillCell(c: *ui.Context, state: []const u8, width: f32) void {
    const slot = ui.div(c).row().w(width).h(20).itemsCenter();
    defer slot.end();
    pill(c, state);
}

/// Status is never colour alone: the word carries the meaning, the colour
/// reinforces it.
pub fn pill(c: *ui.Context, state: []const u8) void {
    const color = statusColor(c, state);
    const box = ui.div(c).row().itemsCenter().px(8).h(20).rounded(10).bg(color.alpha(0.14));
    defer box.end();
    ui.text(c, state, .{ .size = 11, .weight = .medium, .color = color });
}

/// A coloured tile with a glyph in it, the small mark a card and a rail item
/// carry so the eye finds the section before it reads the word.
pub fn chip(c: *ui.Context, icon: []const u8, color: ui.Color, size: f32) void {
    const box = ui.div(c).row().w(size).h(size).center().rounded(size * 0.28).bg(color.alpha(0.14));
    defer box.end();
    ui.icon(c, icon, .{ .size = size * 0.58, .color = color });
}

pub fn statusColor(c: *ui.Context, state: []const u8) ui.Color {
    const th = c.theme;
    if (std.mem.eql(u8, state, "running")) return th.success;
    if (std.mem.eql(u8, state, "exited") or std.mem.eql(u8, state, "stopped")) return th.muted;
    if (std.mem.eql(u8, state, "paused") or std.mem.eql(u8, state, "restarting")) return .hex(0xff9f0a);
    if (std.mem.eql(u8, state, "dead") or std.mem.eql(u8, state, "unhealthy")) return th.danger;
    if (std.mem.eql(u8, state, "created")) return th.accent;
    return th.muted;
}

/// The four states every view implements, in one place.
pub fn emptyState(c: *ui.Context, icon: []const u8, heading: []const u8, detail: []const u8) void {
    const box = ui.div(c).column().wFull().itemsCenter().gap(6).py(40);
    defer box.end();
    ui.icon(c, icon, .{ .size = 28, .color = c.theme.muted });
    ui.text(c, heading, .{ .size = body, .weight = .semibold });
    if (detail.len > 0) ui.text(c, detail, .{ .size = muted, .color = c.theme.muted });
}

pub fn loadingRows(c: *ui.Context, rows: usize) void {
    var i: usize = 0;
    while (i < rows) : (i += 1) {
        const r = ui.div(c).row().wFull().h(40).px(12).itemsCenter();
        defer r.end();
        const bar = ui.div(c).w(160).h(12).rounded(6).bg(c.theme.track);
        defer bar.end();
    }
}

/// The banner a view shows when something failed, with the reason kept.
pub fn errorBanner(c: *ui.Context, text: []const u8) void {
    const box = ui.div(c).row().wFull().itemsCenter().gap(8).px(12).py(8).rounded(8).bg(c.theme.danger.alpha(0.12));
    defer box.end();
    ui.icon(c, "xmark", .{ .size = 14, .color = c.theme.danger });
    ui.text(c, text, .{ .size = body, .color = c.theme.danger });
}

/// Published ports as `8080→80, 8443→443`, then `+n` when there are more.
/// Built in the frame's arena because the row is painted after the view returns.
pub fn portsText(c: *ui.Context, ct: *const @import("../engine/api.zig").Container, max: usize) []const u8 {
    // Docker lists the same binding once per address family, so "8080→80"
    // appears twice for a container published on both IPv4 and IPv6. Show it
    // once: the duplicate is an artifact of the API, not a second port.
    var total: usize = 0;
    {
        var last: ?u32 = null;
        for (ct.Ports) |p| {
            if (p.PublicPort == null) continue;
            const key = (@as(u32, p.PublicPort.?) << 16) | p.PrivatePort;
            if (last != null and last.? == key) continue;
            last = key;
            total += 1;
        }
    }

    var shown: usize = 0;
    var out: []const u8 = "";
    var last: ?u32 = null;
    for (ct.Ports) |p| {
        if (p.PublicPort == null) continue;
        const key = (@as(u32, p.PublicPort.?) << 16) | p.PrivatePort;
        if (last != null and last.? == key) continue;
        last = key;
        if (shown == max) {
            out = c.print("{s} +{d}", .{ out, total - shown });
            break;
        }
        const piece = c.print("{d}→{d}", .{ p.PublicPort.?, p.PrivatePort });
        out = if (shown == 0) piece else c.print("{s}, {s}", .{ out, piece });
        shown += 1;
    }
    return out;
}

/// A labelled figure, used by the Overview and the Runtimes cards.
pub fn metric(c: *ui.Context, label: []const u8, value: []const u8, width: f32) void {
    const box = ui.div(c).column().w(width).gap(2);
    defer box.end();
    ui.text(c, label, .{ .size = 11, .weight = .medium, .color = c.theme.muted });
    ui.text(c, value, .{ .size = 22, .weight = .semibold, .tabular = true });
}

/// A horizontal bar split into segments, for the memory and disk breakdowns.
pub fn stackedBar(c: *ui.Context, segments: []const Segment, width: f32) void {
    const bar = ui.div(c).row().w(width).h(8).rounded(4).bg(c.theme.track);
    defer bar.end();
    for (segments) |seg| {
        if (seg.fraction <= 0) continue;
        const w = @max(@as(f32, 1), width * seg.fraction);
        const piece = ui.div(c).w(w).h(8).rounded(4).bg(seg.color);
        defer piece.end();
    }
}

pub const Segment = struct {
    fraction: f32,
    color: ui.Color,
};

/// A legend line: dot, label, value.
pub fn legend(c: *ui.Context, color: ui.Color, label: []const u8, value: []const u8) void {
    const row = ui.div(c).row().itemsCenter().gap(6);
    defer row.end();
    const dot = ui.div(c).w(8).h(8).rounded(4).bg(color);
    defer dot.end();
    ui.text(c, label, .{ .size = 11, .color = c.theme.muted });
    ui.text(c, value, .{ .size = 11, .weight = .medium });
}

/// A card: surface, one hairline, 16 of padding. Cards never float — shadow
/// is reserved for layers above the page.
pub fn card(c: *ui.Context, width: f32) void {
    const box = ui.div(c).column().w(width).p(16).gap(10).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer box.end();
}

test "a new sort column starts ascending and the same one flips" {
    var sort: Sort = .{ .key = 1 };
    sort.pick(1);
    try std.testing.expect(sort.desc);
    sort.pick(1);
    try std.testing.expect(!sort.desc);
    sort.pick(3);
    try std.testing.expectEqual(@as(u8, 3), sort.key);
    try std.testing.expect(!sort.desc);
}
