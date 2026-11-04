//! Raw inspect JSON: the whole object as the engine sees it, fetched on demand
//! for the detail pane. Kept out of `api.zig` so the typed model stays inside
//! the 600-line rule; the pane wants the unparsed body anyway.
const std = @import("std");
const api = @import("api.zig");
const guards = @import("guards.zig");

pub const Kind = enum {
    container,
    image,
    volume,
    network,
};

/// Deep enough for any real inspect document, shallow enough that the
/// recursive parser and the indenting stringifier cannot be turned into a
/// stack or allocation amplifier by a hostile/forwarded engine.
pub const max_depth: usize = guards.max_depth;

/// Shared with the list-body guard; kept here because inspect is the consumer
/// that calls it before both parsing and pretty-printing.
pub const nestingExceeds = guards.nestingExceeds;

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

pub const Pretty = struct {
    text: []u8,
    truncated: bool,
};

/// Indented, and truncated with a visible marker when it would otherwise
/// overwhelm the pane — image inspect with thousands of layers indents large.
/// The stringifier writes into a fixed window, so even a shallow document with
/// millions of members cannot allocate its fully-indented form just to cut it
/// back down.
pub fn pretty(gpa: std.mem.Allocator, raw: []const u8, cap: usize) !Pretty {
    if (nestingExceeds(raw, max_depth)) {
        return .{
            .text = try std.fmt.allocPrint(
                gpa,
                "… truncated: too complex — JSON nesting deeper than {d} levels ({d} bytes not shown)",
                .{ max_depth, raw.len },
            ),
            .truncated = true,
        };
    }

    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const value = std.json.parseFromSliceLeaky(std.json.Value, arena.allocator(), raw, .{}) catch {
        return rawFallback(gpa, raw, cap);
    };

    // Extra room after the text window is only for the marker; nothing the
    // engine controls is allocated past this point.
    const marker_room = 96;
    const buf = try gpa.alloc(u8, cap + marker_room);
    errdefer gpa.free(buf);
    var writer = std.Io.Writer.fixed(buf[0..cap]);
    var cut = false;
    std.json.Stringify.value(value, .{ .whitespace = .indent_2 }, &writer) catch |e| switch (e) {
        error.WriteFailed => cut = true,
    };
    var used = writer.buffered().len;
    if (cut) {
        // The writer stops at a byte, not a character. Cutting a multi-byte
        // codepoint leaves the pane with a replacement glyph; back up to the
        // last complete sequence before adding the marker.
        used = utf8PrefixLen(buf[0..used]);
        const marker = std.fmt.bufPrint(
            buf[used..],
            "\n… truncated, first {d} bytes shown",
            .{used},
        ) catch "";
        used += marker.len;
    }
    const text = try gpa.dupe(u8, buf[0..used]);
    gpa.free(buf);
    return .{ .text = text, .truncated = cut };
}

/// The longest prefix of `text` that ends on a UTF-8 codepoint boundary.
fn utf8PrefixLen(text: []const u8) usize {
    var cont: usize = 0;
    while (cont < text.len and (text[text.len - 1 - cont] & 0xC0) == 0x80) cont += 1;
    if (cont == text.len) return 0;
    const start = text.len - cont - 1;
    const need = std.unicode.utf8ByteSequenceLength(text[start]) catch return start;
    return if (need == cont + 1) text.len else start;
}

fn rawFallback(gpa: std.mem.Allocator, raw: []const u8, cap: usize) !Pretty {
    if (raw.len <= cap) return .{ .text = try gpa.dupe(u8, raw), .truncated = false };
    const kept = utf8PrefixLen(raw[0..cap]);
    return .{
        .text = try std.fmt.allocPrint(gpa, "{s}\n… truncated, {d} of {d} bytes shown", .{ raw[0..kept], kept, raw.len }),
        .truncated = true,
    };
}

/// Tracks live bytes and their peak while forwarding to a real allocator.
/// Tests use it to distinguish "parse tree plus a bounded output" from a
/// stringifier that materialises the whole indented document first.
const CountingAllocator = struct {
    child: std.mem.Allocator,
    current: usize = 0,
    peak: usize = 0,

    fn allocator(self: *CountingAllocator) std.mem.Allocator {
        return .{ .ptr = self, .vtable = &vtable };
    }

    const vtable = std.mem.Allocator.VTable{
        .alloc = alloc,
        .resize = std.mem.Allocator.noResize,
        .remap = std.mem.Allocator.noRemap,
        .free = free,
    };

    fn alloc(ctx: *anyopaque, len: usize, alignment: std.mem.Alignment, ret_addr: usize) ?[*]u8 {
        const self: *CountingAllocator = @ptrCast(@alignCast(ctx));
        const ptr = self.child.vtable.alloc(self.child.ptr, len, alignment, ret_addr) orelse return null;
        self.current += len;
        if (self.current > self.peak) self.peak = self.current;
        return ptr;
    }

    fn free(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, ret_addr: usize) void {
        const self: *CountingAllocator = @ptrCast(@alignCast(ctx));
        self.current -= memory.len;
        self.child.vtable.free(self.child.ptr, memory, alignment, ret_addr);
    }
};

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
    defer gpa.free(small.text);
    try std.testing.expectEqualStrings("{\n  \"a\": 1\n}", small.text);
    try std.testing.expect(!small.truncated);

    var wide_buf: [512]u8 = undefined;
    @memset(wide_buf[0..], 'x');
    wide_buf[0] = '{';
    wide_buf[1] = '"';
    wide_buf[402] = '"';
    wide_buf[403] = ':';
    wide_buf[404] = '1';
    wide_buf[405] = '}';
    const big = try pretty(gpa, wide_buf[0..406], 128);
    defer gpa.free(big.text);
    try std.testing.expect(big.truncated);
    try std.testing.expect(std.mem.indexOf(u8, big.text, "truncated") != null);
    try std.testing.expect(big.text.len <= 128 + 128);

    // Not JSON at all: returned as-is rather than failing.
    const raw = try pretty(gpa, "not json", 4096);
    defer gpa.free(raw.text);
    try std.testing.expectEqualStrings("not json", raw.text);
    try std.testing.expect(!raw.truncated);
}

test "a marker phrase in engine data does not set the flag" {
    const gpa = std.testing.allocator;
    const result = try pretty(gpa, "{\"note\":\"… truncated\"}", 4096);
    defer gpa.free(result.text);
    try std.testing.expect(!result.truncated);
    try std.testing.expect(std.mem.indexOf(u8, result.text, "… truncated") != null);
}

test "deep nesting is refused before the recursive parser runs" {
    const gpa = std.testing.allocator;
    var body: std.ArrayList(u8) = .empty;
    defer body.deinit(gpa);
    for (0..200) |_| try body.append(gpa, '[');
    for (0..200) |_| try body.append(gpa, ']');

    try std.testing.expect(nestingExceeds(body.items, max_depth));
    const result = try pretty(gpa, body.items, 4096);
    defer gpa.free(result.text);
    try std.testing.expect(std.mem.indexOf(u8, result.text, "too complex") != null);
    try std.testing.expect(result.truncated);
    try std.testing.expect(result.text.len < 128);
}

test "nesting brackets inside strings do not count" {
    try std.testing.expect(!nestingExceeds("{\"note\":\"[[[}}}]\",\"a\":1}", 2));
    try std.testing.expect(nestingExceeds("[[[]]]", 2));
}

test "a huge shallow document is cut at the cap, not stringified in full" {
    const gpa = std.testing.allocator;
    var body: std.ArrayList(u8) = .empty;
    defer body.deinit(gpa);
    try body.append(gpa, '[');
    for (0..20_000) |i| {
        if (i > 0) try body.append(gpa, ',');
        try body.appendSlice(gpa, "{\"k\":\"0123456789\"}");
    }
    try body.append(gpa, ']');

    const parse_peak = blk: {
        var counter = CountingAllocator{ .child = gpa };
        var arena = std.heap.ArenaAllocator.init(counter.allocator());
        defer arena.deinit();
        _ = try std.json.parseFromSliceLeaky(std.json.Value, arena.allocator(), body.items, .{});
        break :blk counter.peak;
    };

    const cap = 256;
    var counter = CountingAllocator{ .child = gpa };
    const result = try pretty(counter.allocator(), body.items, cap);
    defer counter.allocator().free(result.text);
    try std.testing.expect(result.truncated);
    try std.testing.expect(result.text.len <= cap + 96);
    try std.testing.expect(counter.peak <= parse_peak + 4 * (cap + 96));
    // The source is hundreds of times the cap; a stringifier that built the
    // whole indented document first would have allocated all of it.
    try std.testing.expect(body.items.len > cap * 100);
}

test "pretty peak allocation stays near the parse tree plus the cap" {
    const gpa = std.testing.allocator;
    var body: std.ArrayList(u8) = .empty;
    defer body.deinit(gpa);
    const levels = 32;
    const per_level = 4_000;
    for (0..levels) |_| {
        try body.append(gpa, '[');
        for (0..per_level) |_| try body.appendSlice(gpa, "0,");
    }
    try body.append(gpa, '0');
    for (0..levels) |_| try body.append(gpa, ']');

    // Baseline: the parse tree alone. The old implementation then allocated
    // the full indented form (~4.6 MiB here) before truncating it.
    const parse_peak = blk: {
        var counter = CountingAllocator{ .child = gpa };
        var arena = std.heap.ArenaAllocator.init(counter.allocator());
        defer arena.deinit();
        _ = try std.json.parseFromSliceLeaky(std.json.Value, arena.allocator(), body.items, .{});
        break :blk counter.peak;
    };

    const cap = 256;
    var counter = CountingAllocator{ .child = gpa };
    const result = try pretty(counter.allocator(), body.items, cap);
    defer counter.allocator().free(result.text);
    try std.testing.expect(result.truncated);
    // Parsed value + temporary window + returned text + marker room.
    try std.testing.expect(counter.peak <= parse_peak + 4 * (cap + 96));
}

test "truncation never splits a four-byte codepoint" {
    const gpa = std.testing.allocator;
    var body: std.ArrayList(u8) = .empty;
    defer body.deinit(gpa);
    try body.appendSlice(gpa, "{\"k\":\"");
    for (0..64) |_| try body.appendSlice(gpa, "😀");
    try body.appendSlice(gpa, "\"}");

    var with_prefix: usize = 0;
    for (1..80) |cap| {
        const result = try pretty(gpa, body.items, cap);
        defer gpa.free(result.text);
        try std.testing.expect(result.truncated);
        try std.testing.expect(std.unicode.utf8ValidateSlice(result.text));
        const mark = std.mem.indexOf(u8, result.text, "\n… truncated") orelse continue;
        try std.testing.expect(std.unicode.utf8ValidateSlice(result.text[0..mark]));
        try std.testing.expect(mark <= cap);
        if (mark > 0) with_prefix += 1;
    }
    // Several caps landed inside a codepoint; the prefix still has content.
    try std.testing.expect(with_prefix > 0);
}

test "raw fallback also clamps to a codepoint boundary" {
    const gpa = std.testing.allocator;
    var raw: std.ArrayList(u8) = .empty;
    defer raw.deinit(gpa);
    try raw.appendSlice(gpa, "not json 😀");
    for (0..32) |_| try raw.appendSlice(gpa, "😀");
    for (1..80) |cap| {
        const result = try pretty(gpa, raw.items, cap);
        defer gpa.free(result.text);
        try std.testing.expect(result.truncated);
        try std.testing.expect(std.unicode.utf8ValidateSlice(result.text));
        const mark = std.mem.indexOf(u8, result.text, "\n… truncated") orelse continue;
        try std.testing.expect(std.unicode.utf8ValidateSlice(result.text[0..mark]));
    }
}
