//! Shared pieces every view is built from, so a row looks like a row whether
//! it lists containers, images or volumes.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const format = @import("format.zig");
const theme = @import("theme.zig");

pub const muted = 12.5;
pub const body = 13.0;
pub const title = 22.0;

/// Where a monitor with no answer can actually be repaired. The rail no
/// longer carries a refresh button, so telling someone to use it is a dead
/// end; these are the two controls that exist.
pub const engine_help = "Open Runtimes to check the engine, or Settings \u{2192} Maintenance to refresh";

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

/// The page header's controls this frame, so a view can tell a cleared search
/// from a pressed action button.
pub const HeaderAction = struct {
    /// The search field's clear button was pressed (the text now empty).
    cleared: bool = false,
    /// The optional action button (New volume, New network) was pressed.
    action: bool = false,
};

/// A list view's header with its search field (and an optional action button,
/// which shares the row rather than adding a second toolbar).
pub fn pageHeaderSearch(c: *ui.Context, heading: []const u8, subtitle: []const u8, box: ?SearchBox) HeaderAction {
    const h = ui.div(c).row().wFull().itemsCenter().gap(10).pb(10);
    defer h.end();
    ui.text(c, heading, .{ .size = title, .weight = .semibold });
    if (subtitle.len > 0) ui.text(c, subtitle, .{ .size = muted, .color = c.theme.muted });
    ui.spacer(c);
    const search = box orelse return .{};
    var result: HeaderAction = .{};
    if (search.action_label.len > 0) {
        const button = ui.div(c).row().h(26).px(10).itemsCenter().gap(5).rounded(13).bg(c.theme.surface_hover).control();
        if (search.action_icon.len > 0) ui.icon(c, search.action_icon, .{ .size = 10, .color = c.theme.text });
        ui.text(c, search.action_label, .{ .size = 11.5, .color = c.theme.text });
        result.action = button.clicked();
        button.end();
    }
    _ = ui.textField(c, search.buffer, .{
        .placeholder = search.placeholder,
        .width = .{ .px = search.width },
        .size = 12.5,
    });
    if (search.buffer.text().len == 0) return result;
    const clear = ui.div(c).row().w(22).h(22).center().rounded(11).bg(c.theme.surface_hover).control();
    ui.icon(c, "xmark", .{ .size = 10, .color = c.theme.muted });
    const clicked = clear.clicked();
    clear.end();
    if (clicked) search.buffer.clear();
    result.cleared = clicked;
    return result;
}

pub const SearchBox = struct {
    buffer: *ui.TextBuffer,
    placeholder: []const u8,
    width: f32 = 190,
    /// A page-level action beside the search box — "New volume".
    action_label: []const u8 = "",
    action_icon: []const u8 = "",
};

/// One shape for every dialog action: same width, same height, so Cancel and
/// Confirm sit on one baseline instead of sizing themselves to their word.
pub fn dialogButton(c: *ui.Context, label: []const u8, danger: bool, primary: bool) bool {
    const bg = if (!primary) c.theme.surface_hover else if (danger) c.theme.danger else c.theme.accent;
    const fg = if (primary) c.theme.accent_text else c.theme.text;
    const box = ui.div(c).row().w(92).h(28).center().rounded(14).bg(bg).control();
    ui.text(c, label, .{ .size = 12, .weight = if (primary) .medium else .regular, .color = fg });
    const clicked = box.clicked();
    box.end();
    return clicked;
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
        const active_color = theme.accentText(c.theme);
        ui.text(c, col.name, .{
            .size = 11,
            .weight = .medium,
            .color = if (active) active_color else c.theme.muted,
        });
        if (active) ui.text(c, if (sort.?.desc) "▼" else "▲", .{ .size = 8, .color = active_color });

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

pub const StatusTone = enum { success, warning, danger, accent, muted };

pub fn statusTone(state: []const u8) StatusTone {
    if (std.mem.eql(u8, state, "running") or std.mem.eql(u8, state, "healthy")) return .success;
    if (std.mem.eql(u8, state, "exited") or std.mem.eql(u8, state, "stopped")) return .muted;
    if (std.mem.eql(u8, state, "paused") or std.mem.eql(u8, state, "restarting") or
        std.mem.eql(u8, state, "starting")) return .warning;
    if (std.mem.eql(u8, state, "dead") or std.mem.eql(u8, state, "unhealthy")) return .danger;
    if (std.mem.eql(u8, state, "created")) return .accent;
    return .muted;
}

/// Pills are text first: every tone has a light-theme text variant that
/// survives its own 14% wash over an alternate, hovered or selected row.
pub fn statusColorFor(t: ui.Theme, state: []const u8) ui.Color {
    return switch (statusTone(state)) {
        .success => theme.successText(t),
        .warning => theme.warning(t),
        .danger => theme.dangerText(t),
        .accent => theme.accentText(t),
        .muted => theme.mutedText(t),
    };
}

pub fn statusColor(c: *ui.Context, state: []const u8) ui.Color {
    return statusColorFor(c.theme, state);
}

/// What a page can say before its first snapshot commits. "No events" and
/// "no disk figures" are claims about the engine's contents; before a snapshot
/// lands they describe nothing, and after a failed first pass the honest
/// answer is that the engine is not answering.
pub const FirstRead = enum { reading, failed, ready };

pub fn firstRead(last_update: i64, error_text: []const u8) FirstRead {
    if (last_update != 0) return .ready;
    return if (error_text.len > 0) .failed else .reading;
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
    const color = theme.dangerText(c.theme);
    const box = ui.div(c).row().wFull().itemsCenter().gap(8).px(12).py(8).rounded(8).bg(color.alpha(0.12));
    defer box.end();
    ui.icon(c, "xmark", .{ .size = 14, .color = color });
    ui.text(c, text, .{ .size = body, .color = color });
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
            .color = theme.accentText(c.theme),
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

test "first-read separates reading, a failed first pass and a snapshot" {
    try std.testing.expectEqual(FirstRead.reading, firstRead(0, ""));
    try std.testing.expectEqual(FirstRead.failed, firstRead(0, "engine stopped answering"));
    try std.testing.expectEqual(FirstRead.ready, firstRead(123, ""));
    // A later failure keeps the stale snapshot on screen; it is not a failed
    // first read and the views must not replace their rows with a skeleton.
    try std.testing.expectEqual(FirstRead.ready, firstRead(123, "could not read stats"));
}

test "every status pill clears 4.7:1 on every light-theme row background" {
    const t = theme.of(ui.Theme.light);
    const rows = [_]ui.Color{
        t.surface,
        t.surface.mix(t.surface_hover, 0.35), // alternate row on a card
        t.background.mix(t.surface_hover, 0.35), // alternate row on the page
        t.surface_hover, // hovered row
        t.background.mix(t.accent, 0.10), // selected row on the page
        t.surface.mix(t.accent, 0.10), // selected row on a card
        t.background,
    };
    const states = [_][]const u8{
        "running",  "healthy", "exited",    "stopped", "paused",   "restarting",
        "starting", "dead",    "unhealthy", "created", "removing",
    };
    for (states) |state| {
        const color = statusColorFor(t, state);
        for (rows) |row| {
            // The pill's own 14% wash sits on the row, not on the page.
            try std.testing.expect(theme.contrastRatio(color, row.mix(color, 0.14)) >= 4.7);
        }
    }
}

test "dark status pills keep the theme's own values" {
    const t = theme.of(ui.Theme.dark_theme);
    try std.testing.expectEqual(t.success, statusColorFor(t, "running"));
    try std.testing.expectEqual(theme.warning_fill, statusColorFor(t, "paused"));
    try std.testing.expectEqual(t.danger, statusColorFor(t, "dead"));
    try std.testing.expectEqual(t.accent, statusColorFor(t, "created"));
    try std.testing.expectEqual(t.muted, statusColorFor(t, "exited"));
}

test "accent text survives the tints used by pills and toggles" {
    const t = theme.of(ui.Theme.light);
    const color = theme.accentText(t);
    const rows = [_]ui.Color{
        t.surface,
        t.background,
        t.surface_hover,
        t.surface.mix(t.surface_hover, 0.35),
        t.background.mix(t.surface_hover, 0.35),
        t.background.mix(t.accent, 0.10),
        t.surface.mix(t.accent, 0.10),
    };
    // Port pills sit at 12%, Follow/Times and filter chips at 16%.
    for ([_]f32{ 0.12, 0.16 }) |tint| {
        for (rows) |row| {
            try std.testing.expect(theme.contrastRatio(color, row.mix(t.accent, tint)) >= 4.5);
        }
    }
    // The CPU card label and active sort header sit on bare surfaces.
    try std.testing.expect(theme.contrastRatio(color, t.surface) >= 4.5);
    try std.testing.expect(theme.contrastRatio(color, t.background) >= 4.5);
}
