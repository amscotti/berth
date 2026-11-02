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
/// The search field belongs in this row, on the right: it is part of the
/// page's chrome, not a second toolbar below it.
pub fn pageHeader(c: *ui.Context, heading: []const u8, subtitle: []const u8) void {
    _ = pageHeaderSearch(c, heading, subtitle, null);
}

/// A list view's header with its search field. Returns true when the field's
/// clear button was pressed, so the view can redraw with the filter gone.
pub fn pageHeaderSearch(c: *ui.Context, heading: []const u8, subtitle: []const u8, box: ?SearchBox) bool {
    const h = ui.div(c).row().wFull().itemsCenter().gap(10).pb(10);
    defer h.end();
    ui.text(c, heading, .{ .size = title, .weight = .semibold });
    if (subtitle.len > 0) ui.text(c, subtitle, .{ .size = muted, .color = c.theme.muted });
    ui.spacer(c);
    const search = box orelse return false;
    _ = ui.textField(c, search.buffer, .{
        .placeholder = search.placeholder,
        .width = .{ .px = search.width },
        .size = 12.5,
    });
    if (search.buffer.text().len == 0) return false;
    const clear = ui.div(c).row().w(22).h(22).center().rounded(11).bg(c.theme.surface_hover).control();
    ui.icon(c, "xmark", .{ .size = 10, .color = c.theme.muted });
    const clicked = clear.clicked();
    clear.end();
    if (clicked) search.buffer.clear();
    return clicked;
}

pub const SearchBox = struct {
    buffer: *ui.TextBuffer,
    placeholder: []const u8,
    width: f32 = 190,
};

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
    /// Push this column and everything after it to the right edge, so the
    /// actions stay where the mouse expects them on a wide window.
    flex_before: bool = false,
    /// Extra air before this column, to group related figures (the I/O pair,
    /// the process count) without drawing a divider.
    gap_before: f32 = 0,
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
    const row = ui.div(c).row().wFull().itemsCenter().px(12).pb(6).gap(10);
    for (cols) |col| {
        if (col.skip) continue;
        if (col.flex_before) ui.spacer(c);
        if (col.gap_before > 0) {
            const gap = ui.div(c).w(col.gap_before);
            gap.end();
        }
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
    if (std.mem.eql(u8, state, "running") or std.mem.eql(u8, state, "healthy")) return th.success;
    if (std.mem.eql(u8, state, "exited") or std.mem.eql(u8, state, "stopped")) return th.muted;
    if (std.mem.eql(u8, state, "paused") or std.mem.eql(u8, state, "restarting") or
        std.mem.eql(u8, state, "starting")) return .hex(0xff9f0a);
    if (std.mem.eql(u8, state, "dead") or std.mem.eql(u8, state, "unhealthy")) return th.danger;
    if (std.mem.eql(u8, state, "created")) return th.accent;
    return th.muted;
}

/// The four states every view implements, in one place.
pub fn emptyState(c: *ui.Context, icon: []const u8, heading: []const u8, detail: []const u8) void {
    _ = emptyStateAction(c, icon, heading, detail, null);
}

/// The same empty state with a way out of it: "Clear search", "Show all".
/// Returns true when that button was clicked.
pub fn emptyStateAction(c: *ui.Context, icon: []const u8, heading: []const u8, detail: []const u8, action_label: ?[]const u8) bool {
    const box = ui.div(c).column().wFull().itemsCenter().gap(6).py(40);
    defer box.end();
    ui.icon(c, icon, .{ .size = 28, .color = c.theme.muted });
    ui.text(c, heading, .{ .size = body, .weight = .semibold });
    if (detail.len > 0) ui.text(c, detail, .{ .size = muted, .color = c.theme.muted });
    const label = action_label orelse return false;
    const button = ui.div(c).row().h(26).px(11).itemsCenter().rounded(13).bg(c.theme.surface_hover);
    ui.text(c, label, .{ .size = 11.5, .weight = .medium, .color = c.theme.text });
    const clicked = button.clicked();
    button.end();
    return clicked;
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

/// Published ports as clickable pills: the host port, opening it in the
/// browser when clicked. Fixed total width so the column never shifts, and the
/// pills are bounded by that width however wide the digits are; at most `max`
/// pills, then a muted "+k". `show_target` adds the container-side port, as
/// `8080→80`, which the inspector has room for. Returns the binding clicked.
pub fn portPills(c: *ui.Context, ct: *const @import("../engine/api.zig").Container, max: usize, width: f32, show_target: bool) ?@import("../engine/api.zig").HostBinding {
    var ports: [16]@import("../engine/api.zig").HostBinding = undefined;
    const total = ct.publishedPorts(&ports);
    const filled = @min(total, ports.len);

    // Pills are intrinsic-width, so bound them by the slot: reserve room for
    // the "+k" suffix, then fit as many pills as their digits allow.
    var budget = width - 24;
    var shown: usize = 0;
    while (shown < filled and shown < max) : (shown += 1) {
        var digits: usize = 1;
        var port = ports[shown].port;
        while (port >= 10) : (port = @divTrunc(port, 10)) digits += 1;
        var label_len = digits;
        if (show_target) {
            var inner = ports[shown].private_port;
            label_len += 1;
            while (inner >= 10) : (inner = @divTrunc(inner, 10)) label_len += 1;
        }
        const pill_w = 14 + @as(f32, @floatFromInt(label_len)) * 6.6;
        if (shown > 0) budget -= 4;
        if (pill_w > budget) break;
        budget -= pill_w;
    }

    const slot = ui.div(c).row().w(width).h(20).itemsCenter().gap(4).clip();
    defer slot.end();
    var clicked: ?@import("../engine/api.zig").HostBinding = null;
    for (ports[0..shown]) |binding| {
        const host = ui.div(c).row().h(18).px(7).itemsCenter().rounded(9)
            .bg(c.theme.accent.alpha(0.12)).control();
        if (host.clicked()) clicked = binding;
        const label = if (show_target)
            c.print("{d}\u{2192}{d}", .{ binding.port, binding.private_port })
        else
            c.print("{d}", .{binding.port});
        ui.text(c, label, .{
            .size = 10.5,
            .weight = .medium,
            .color = c.theme.accent,
            .tabular = true,
        });
        host.end();
    }
    if (total > shown) {
        ui.text(c, c.print("+{d}", .{total - shown}), .{
            .size = 10.5,
            .color = c.theme.muted,
            .tabular = true,
        });
    }
    return clicked;
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
