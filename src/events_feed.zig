//! The events feed's state transition: merge an interval into the ring, move
//! the `since` cursor, and turn exits, OOMs and restarts into toasts.
const std = @import("std");
const app = @import("app.zig");
const events = @import("engine/events.zig");

pub fn commit(s: *app.State, snap_since: i64, snap_until: i64, fresh: []const events.Event) void {
    if (snap_since == 0) return; // this pass asked but got no interval
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
