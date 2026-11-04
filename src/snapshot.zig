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
    /// False when any up container had no sample: sums over `stats` are then
    /// lower bounds, not the containers' memory use.
    stats_complete: bool = false,
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    events: []events.Event = &.{},
    events_since: i64 = 0,
    events_until: i64 = 0,
    /// The interval was attempted and failed. The ring can still hold older
    /// events; an empty ring must not claim the engine has nothing to say.
    events_error: bool = false,
    logs: []const u8 = &.{},
    logs_id: []const u8 = &.{},
    logs_timestamps: bool = false,
    df: ?api.Df = null,
    /// `/system/df` was attempted and failed. A null `df` with this false is
    /// "not read yet"; with it true it is "could not be read".
    df_error: bool = false,
    /// The engine's host-process RSS, or null when this pass could not read it.
    engine_rss: ?u64 = null,

    pub fn init(gpa: std.mem.Allocator) Snapshot {
        return .{ .arena = .init(gpa) };
    }

    pub fn deinit(self: *Snapshot) void {
        self.arena.deinit();
    }
};
