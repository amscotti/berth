//! The events feed's state transition: merge an interval into the ring, move
//! the `since` cursor, and turn exits, OOMs and restarts into toasts.
const std = @import("std");
const app = @import("app.zig");
const events = @import("engine/events.zig");

pub fn commit(s: *app.State, snap_since: i64, snap_until: i64, fresh: []const events.Event) void {
    if (snap_since == 0) {
        // This pass asked but got no interval. Its arena has just become the
        // state's, and the ring still points into the arena that was just
        // retired; merge with no fresh events to copy it forward while leaving
        // the cursors where the failed pass left them.
        s.events.items = events.merge(s.arena.allocator(), s.events.items, &.{}).items;
        return;
    }
    const previous_last = s.events.last_seen;
    const old = s.events.items;
    const merged = events.merge(s.arena.allocator(), old, fresh);
    s.events.items = merged.items;
    if (merged.latest > previous_last) {
        // The first load is history, not news: set the high-water mark quietly.
        if (previous_last != 0) notifyNew(s, old, fresh, previous_last);
        s.events.last_seen = merged.latest;
    }
    // Prefer the engine's own clock: a VM that trails the host would otherwise
    // filter out every later event. Re-read one overlapped second and dedupe.
    s.events.since = if (merged.latest > 0)
        @divTrunc(merged.latest, std.time.ns_per_s) -| 1
    else
        snap_until;
}

fn notifyNew(s: *app.State, old: []const events.Event, fresh: []const events.Event, previous_last: i64) void {
    for (fresh) |event| {
        if (event.time_nano <= previous_last) continue;
        if (events.has(old, event)) continue;
        const what = events.notice(event) orelse continue;
        var name = event.name;
        if (name.len == 0) name = event.id;
        s.notifyFmt("{s} {s}", .{ short(name), what }, true);
    }
}

fn short(name: []const u8) []const u8 {
    if (name.len <= 24) return name;
    return name[0..24];
}

test "the ring survives repeated interval-less passes and keeps merging" {
    // Only the events and the arena matter on this path; the app handle is
    // never read by `commit` for these rows.
    var s = app.State{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer s.arena.deinit();
    defer {
        if (s.retired) |*old| old.deinit();
    }

    const initial_ns: i64 = 1_790_000_000 * std.time.ns_per_s;
    const initial = [_]events.Event{
        .{ .time_nano = initial_ns, .kind = "container", .action = "die", .id = "abc", .name = "web" },
    };
    s.events = .{
        .items = events.merge(s.arena.allocator(), &.{}, initial[0..]).items,
        .since = 1_789_999_994,
        .last_seen = initial_ns,
    };
    const stale = s.events.items;
    const stale_kind = stale[0].kind;
    const stale_action = stale[0].action;
    const stale_id = stale[0].id;
    const stale_name = stale[0].name;

    // First interval-less pass. This mirrors refresh.commit: retire the
    // current arena, make the snapshot's arena the state's, then commit.
    s.retired = s.arena;
    s.arena = .init(std.testing.allocator);
    commit(&s, 0, 1_790_000_001, &.{});

    try std.testing.expectEqual(@as(usize, 1), s.events.items.len);
    try std.testing.expectEqualStrings("container", s.events.items[0].kind);
    try std.testing.expectEqualStrings("die", s.events.items[0].action);
    try std.testing.expectEqualStrings("abc", s.events.items[0].id);
    try std.testing.expectEqualStrings("web", s.events.items[0].name);
    try std.testing.expectEqual(@as(i64, 1_789_999_994), s.events.since);
    try std.testing.expectEqual(initial_ns, s.events.last_seen);

    // Poison the arena the ring came from before freeing it, so a shallow
    // move cannot hide behind freed memory that still holds the old bytes.
    @memset(@constCast(stale_kind), 'x');
    @memset(@constCast(stale_action), 'x');
    @memset(@constCast(stale_id), 'x');
    @memset(@constCast(stale_name), 'x');
    if (s.retired) |*old| old.deinit();
    s.retired = null;

    // Second interval-less pass: retire the arena that held pass one's ring,
    // commit into a fresh one, then free the old ring's arena.
    s.retired = s.arena;
    s.arena = .init(std.testing.allocator);
    commit(&s, 0, 1_790_000_002, &.{});
    try std.testing.expectEqual(@as(usize, 1), s.events.items.len);
    try std.testing.expectEqualStrings("die", s.events.items[0].action);
    if (s.retired) |*old| old.deinit();
    s.retired = null;
    try std.testing.expectEqualStrings("web", s.events.items[0].name);
    try std.testing.expectEqual(@as(i64, 1_789_999_994), s.events.since);
    try std.testing.expectEqual(initial_ns, s.events.last_seen);

    // A successful pass merges one fresh event over the surviving ring.
    s.retired = s.arena;
    s.arena = .init(std.testing.allocator);
    const fresh_ns: i64 = 1_790_000_005 * std.time.ns_per_s;
    const fresh = [_]events.Event{
        .{ .time_nano = fresh_ns, .kind = "container", .action = "start", .id = "def", .name = "api" },
    };
    commit(&s, 1_790_000_001, 1_790_000_006, fresh[0..]);
    if (s.retired) |*old| old.deinit();
    s.retired = null;

    try std.testing.expectEqual(@as(usize, 2), s.events.items.len);
    try std.testing.expectEqualStrings("start", s.events.items[0].action);
    try std.testing.expectEqualStrings("api", s.events.items[0].name);
    try std.testing.expectEqualStrings("def", s.events.items[0].id);
    try std.testing.expectEqualStrings("die", s.events.items[1].action);
    try std.testing.expectEqualStrings("web", s.events.items[1].name);
    try std.testing.expectEqual(@as(i64, 1_790_000_004), s.events.since);
    try std.testing.expectEqual(fresh_ns, s.events.last_seen);
}
