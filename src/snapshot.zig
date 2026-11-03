//! One complete view of an engine: its containers, stats and stores. A refresh
//! builds one of these on a worker thread; the main thread swaps it in only
//! when it is whole.
const std = @import("std");
const api = @import("engine/api.zig");
const stats_pool = @import("stats_pool.zig");
const events = @import("engine/events.zig");

pub const Snapshot = struct {
    arena: std.heap.ArenaAllocator,
    containers: []api.Container = &.{},
    stats: []stats_pool.Sample = &.{},
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    events: []events.Event = &.{},
    events_since: i64 = 0,
    events_until: i64 = 0,
    logs: []const u8 = &.{},
    logs_id: []const u8 = &.{},
    logs_timestamps: bool = false,
    df: ?api.Df = null,
    /// The engine's host-process RSS, or null when this pass could not read it.
    engine_rss: ?u64 = null,

    pub fn init(gpa: std.mem.Allocator) Snapshot {
        return .{ .arena = .init(gpa) };
    }

    pub fn deinit(self: *Snapshot) void {
        self.arena.deinit();
    }
};
