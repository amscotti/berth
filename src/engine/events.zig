//! Engine events: what the daemon did while berth was watching. `/events` with
//! `since` and `until` answers with one JSON object per line, so the refresh
//! worker can collect an interval without streaming.
const std = @import("std");

/// Enough for a busy engine over a few refreshes; older events fall off the
/// front (newest first) exactly as the graph samples do.
pub const Cap = 300;

pub const Event = struct {
    time_nano: i64 = 0,
    kind: []const u8 = "",
    action: []const u8 = "",
    id: []const u8 = "",
    name: []const u8 = "",
    image: []const u8 = "",
    exit_code: []const u8 = "",

    pub fn seconds(self: Event) i64 {
        return @divTrunc(self.time_nano, std.time.ns_per_s);
    }

    pub fn matches(self: Event, q: []const u8) bool {
        if (q.len == 0) return true;
        return contains(self.kind, q) or contains(self.action, q) or
            contains(self.name, q) or contains(self.image, q) or contains(self.id, q) or
            contains(self.exit_code, q) or containsExit(self, q);
    }
};

/// The table renders an exit as `exit <code>`; searching for that composed
/// phrase must find the row even though only the code is stored. This is a
/// substring search over the virtual concatenation, not just a prefix, so
/// "exit 13" matches a 137 exit the way the rendered text would.
fn containsExit(event: Event, q: []const u8) bool {
    if (event.exit_code.len == 0 or q.len == 0) return false;
    const prefix = "exit ";
    const total = prefix.len + event.exit_code.len;
    if (q.len > total) return false;
    var i: usize = 0;
    outer: while (i + q.len <= total) : (i += 1) {
        for (q, 0..) |n, j| {
            const at = i + j;
            const h = if (at < prefix.len) prefix[at] else event.exit_code[at - prefix.len];
            if (std.ascii.toLower(h) != std.ascii.toLower(n)) continue :outer;
        }
        return true;
    }
    return false;
}

pub const Feed = struct {
    /// Newest first, in the current snapshot arena.
    items: []Event = &.{},
    /// Where the next interval starts, in Unix seconds.
    since: i64 = 0,
    /// The newest event already shown. The first load sets it without
    /// notifying, so launching berth does not replay old exits as toasts.
    last_seen: i64 = 0,
    /// The engine's words for the last failed interval, when it gave any.
    error_text: []const u8 = "",
};

pub const Merge = struct {
    items: []Event,
    appended: usize,
    latest: i64,
};

pub fn parse(arena: std.mem.Allocator, body: []const u8) []Event {
    var out = std.ArrayList(Event).empty;
    var lines = std.mem.splitScalar(u8, body, '\n');
    while (lines.next()) |line| {
        const trimmed = std.mem.trim(u8, line, " \r\t");
        if (trimmed.len == 0) continue;
        const Doc = struct {
            Type: []const u8 = "",
            Action: []const u8 = "",
            Actor: struct {
                ID: []const u8 = "",
                Attributes: ?std.json.Value = null,
            } = .{},
            time: i64 = 0,
            timeNano: i64 = 0,
        };
        const parsed = std.json.parseFromSliceLeaky(Doc, arena, trimmed, .{
            .ignore_unknown_fields = true,
            // The caller frees the response body as soon as this returns;
            // without alloc_always every string above would alias it.
            .allocate = .alloc_always,
        }) catch continue;
        const attributes = parsed.Actor.Attributes;
        const fallback_ns = parsed.time * std.time.ns_per_s;
        out.append(arena, .{
            .time_nano = if (parsed.timeNano != 0) parsed.timeNano else fallback_ns,
            .kind = parsed.Type,
            .action = parsed.Action,
            .id = parsed.Actor.ID,
            .name = attr(attributes, "name"),
            .image = attr(attributes, "image"),
            .exit_code = attr(attributes, "exitCode"),
        }) catch break;
    }
    return out.items;
}

fn attr(value: ?std.json.Value, key: []const u8) []const u8 {
    const attrs = value orelse return "";
    if (attrs != .object) return "";
    const field = attrs.object.get(key) orelse return "";
    return switch (field) {
        .string => |s| s,
        else => "",
    };
}

fn contains(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > haystack.len) return false;
    var i: usize = 0;
    outer: while (i + needle.len <= haystack.len) : (i += 1) {
        for (needle, 0..) |n, j| {
            if (std.ascii.toLower(haystack[i + j]) != std.ascii.toLower(n)) continue :outer;
        }
        return true;
    }
    return false;
}

fn same(a: Event, b: Event) bool {
    return a.time_nano == b.time_nano and std.mem.eql(u8, a.action, b.action) and
        std.mem.eql(u8, a.id, b.id) and std.mem.eql(u8, a.kind, b.kind);
}

pub fn has(list: []const Event, event: Event) bool {
    for (list) |existing| {
        if (same(existing, event)) return true;
    }
    return false;
}

/// `fresh` arrives oldest-first from the engine; the ring stays newest-first.
/// `appended` counts every unique new event even when the cap drops it, so a
/// notification is never suppressed by a flood.
pub fn merge(arena: std.mem.Allocator, old: []const Event, fresh: []const Event) Merge {
    var latest: i64 = 0;
    var appended: usize = 0;
    for (fresh, 0..) |event, i| {
        latest = @max(latest, event.time_nano);
        if (has(old, event)) continue;
        // An engine can repeat a tuple inside one response; count it once.
        if (has(fresh[0..i], event)) continue;
        appended += 1;
    }

    var out = std.ArrayList(Event).empty;
    var i = fresh.len;
    while (i > 0) {
        i -= 1;
        if (has(old, fresh[i])) continue;
        if (has(out.items, fresh[i])) continue;
        if (out.items.len == Cap) break;
        out.append(arena, dupe(arena, fresh[i])) catch break;
    }
    for (old) |event| {
        if (out.items.len == Cap) break;
        if (has(out.items, event)) continue;
        out.append(arena, dupe(arena, event)) catch break;
    }
    return .{ .items = out.items, .appended = appended, .latest = latest };
}

fn dupe(arena: std.mem.Allocator, event: Event) Event {
    return .{
        .time_nano = event.time_nano,
        .kind = arena.dupe(u8, event.kind) catch "",
        .action = arena.dupe(u8, event.action) catch "",
        .id = arena.dupe(u8, event.id) catch "",
        .name = arena.dupe(u8, event.name) catch "",
        .image = arena.dupe(u8, event.image) catch "",
        .exit_code = arena.dupe(u8, event.exit_code) catch "",
    };
}

/// The toast line for an event worth interrupting the rail for.
pub fn notice(event: Event) ?[]const u8 {
    if (std.mem.eql(u8, event.action, "die")) return "exited";
    if (std.mem.eql(u8, event.action, "oom")) return "ran out of memory";
    if (std.mem.eql(u8, event.action, "restart")) return "restarted";
    return null;
}

test "events parse one JSON object per line" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const body =
        "{\"status\":\"start\",\"id\":\"abc\",\"from\":\"nginx:alpine\",\"Type\":\"container\",\"Action\":\"start\",\"Actor\":{\"ID\":\"abc\",\"Attributes\":{\"image\":\"nginx:alpine\",\"name\":\"web\"}},\"time\":1790000000,\"timeNano\":1790000000123456789}\n" ++
        "{\"Type\":\"container\",\"Action\":\"die\",\"Actor\":{\"ID\":\"abc\",\"Attributes\":{\"exitCode\":\"137\",\"name\":\"web\"}},\"time\":1790000001,\"timeNano\":1790000001000000000}\n\n";
    const parsed = parse(a, body);
    try std.testing.expectEqual(@as(usize, 2), parsed.len);
    try std.testing.expectEqualStrings("web", parsed[0].name);
    try std.testing.expectEqualStrings("137", parsed[1].exit_code);
    try std.testing.expectEqual(@as(i64, 1790000000), parsed[0].seconds());
    try std.testing.expectEqualStrings("die", parsed[1].action);
}

test "parsed events survive the response body being freed" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    // The refresh worker frees the /events body right after parsing; the
    // table then showed the bytes that overwrote it ("er" for "container").
    const body = try std.testing.allocator.dupe(u8, "{\"Type\":\"container\",\"Action\":\"kill\",\"Actor\":{\"ID\":\"abc\",\"Attributes\":{\"name\":\"web\"}},\"time\":1790000000}");
    defer std.testing.allocator.free(body);
    const parsed = parse(a, body);
    @memset(body, 'x');
    try std.testing.expectEqualStrings("container", parsed[0].kind);
    try std.testing.expectEqualStrings("kill", parsed[0].action);
    try std.testing.expectEqualStrings("web", parsed[0].name);
}

test "the ring keeps newest first and counts duplicates once" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const old = [_]Event{.{ .time_nano = 2, .kind = "container", .action = "start", .id = "a" }};
    const fresh = [_]Event{
        .{ .time_nano = 2, .kind = "container", .action = "start", .id = "a" },
        .{ .time_nano = 3, .kind = "container", .action = "die", .id = "a" },
        .{ .time_nano = 3, .kind = "container", .action = "die", .id = "a" },
    };
    const merged = merge(a, old[0..], fresh[0..]);
    try std.testing.expectEqual(@as(usize, 1), merged.appended);
    try std.testing.expectEqual(@as(usize, 2), merged.items.len);
    try std.testing.expectEqualStrings("die", merged.items[0].action);
    try std.testing.expectEqualStrings("start", merged.items[1].action);
    try std.testing.expectEqual(@as(i64, 3), merged.latest);
}

test "search matches the fields the table shows" {
    const event = Event{ .kind = "container", .action = "die", .name = "web", .image = "nginx", .exit_code = "137" };
    try std.testing.expect(event.matches("WEB"));
    try std.testing.expect(event.matches("nginx"));
    try std.testing.expect(event.matches("137"));
    try std.testing.expect(event.matches("exit 137"));
    try std.testing.expect(event.matches("EXIT 13"));
    try std.testing.expect(!event.matches("exit 138"));
    try std.testing.expect(!event.matches("redis"));
}
