//! Raw inspect JSON: the whole object as the engine sees it, fetched on demand
//! for the detail pane. Kept out of `api.zig` so the typed model stays inside
//! the 600-line rule; the pane wants the unparsed body anyway.
const std = @import("std");
const api = @import("api.zig");

pub const Kind = enum {
    container,
    image,
    volume,
    network,
};

/// The endpoint each kind inspects. An image is addressed by its Id, never a
/// tag: tags can be re-pointed between the click and the fetch.
pub fn path(buf: []u8, kind: Kind, id: []const u8) ![]const u8 {
    return switch (kind) {
        .container => std.fmt.bufPrint(buf, "/containers/{s}/json", .{id}),
        .image => std.fmt.bufPrint(buf, "/images/{s}/json", .{id}),
        .volume => std.fmt.bufPrint(buf, "/volumes/{s}", .{id}),
        .network => std.fmt.bufPrint(buf, "/networks/{s}", .{id}),
    };
}

/// The raw body, owned by the caller. Inspect is one cheap object, so it gets
/// the same short budget `df` has: a stalled answer must not hold the pane.
pub fn fetch(client: *api.Client, kind: Kind, id: []const u8, gpa: std.mem.Allocator) ![]u8 {
    var buf: [320]u8 = undefined;
    const p = try path(&buf, kind, id);
    const body = try client.callOptsTimeout("GET", p, "", 1 << 20, false, 3, false);
    defer gpa.free(body);
    return gpa.dupe(u8, body);
}

/// Indented, and truncated with a visible marker when it would otherwise
/// overwhelm the pane — image inspect with thousands of layers indents large.
/// What is returned is never longer than `cap` plus the marker's own line.
pub fn pretty(gpa: std.mem.Allocator, raw: []const u8, cap: usize) ![]u8 {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const value = std.json.parseFromSliceLeaky(std.json.Value, arena.allocator(), raw, .{}) catch {
        const kept = @min(raw.len, cap);
        if (raw.len <= cap) return gpa.dupe(u8, raw);
        return std.fmt.allocPrint(gpa, "{s}\n… truncated, {d} of {d} bytes shown", .{ raw[0..kept], kept, raw.len });
    };
    const text = try std.json.Stringify.valueAlloc(gpa, value, .{ .whitespace = .indent_2 });
    if (text.len <= cap) return text;
    defer gpa.free(text);
    const cut = @min(cap, text.len) -| 64;
    return std.fmt.allocPrint(gpa, "{s}\n… truncated, {d} of {d} bytes shown", .{ text[0..cut], cut, text.len });
}

test "each kind addresses its own endpoint" {
    var buf: [320]u8 = undefined;
    try std.testing.expectEqualStrings("/containers/abc/json", try path(&buf, .container, "abc"));
    try std.testing.expectEqualStrings("/images/sha256:ff/json", try path(&buf, .image, "sha256:ff"));
    try std.testing.expectEqualStrings("/volumes/shop-data", try path(&buf, .volume, "shop-data"));
    try std.testing.expectEqualStrings("/networks/bridge", try path(&buf, .network, "bridge"));
}

test "pretty indents and truncates with a marker" {
    const gpa = std.testing.allocator;
    const small = try pretty(gpa, "{\"a\":1}", 4096);
    defer gpa.free(small);
    try std.testing.expectEqualStrings("{\n  \"a\": 1\n}", small);

    var wide_buf: [512]u8 = undefined;
    @memset(wide_buf[0..], 'x');
    wide_buf[0] = '{';
    wide_buf[1] = '"';
    wide_buf[402] = '"';
    wide_buf[403] = ':';
    wide_buf[404] = '1';
    wide_buf[405] = '}';
    const big = try pretty(gpa, wide_buf[0..406], 128);
    defer gpa.free(big);
    try std.testing.expect(std.mem.indexOf(u8, big, "truncated") != null);
    try std.testing.expect(big.len <= 128 + 128);

    // Not JSON at all: returned as-is rather than failing.
    const raw = try pretty(gpa, "not json", 4096);
    defer gpa.free(raw);
    try std.testing.expectEqualStrings("not json", raw);
}
