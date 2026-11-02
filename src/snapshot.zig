//! One complete view of an engine: its containers, stats, stores and the
//! engine's own version/info. A refresh builds one of these on a worker
//! thread; the main thread swaps it in only when it is whole.
const std = @import("std");
const api = @import("engine/api.zig");
const stats_pool = @import("stats_pool.zig");

pub const Snapshot = struct {
    arena: std.heap.ArenaAllocator,
    containers: []api.Container = &.{},
    stats: []stats_pool.Sample = &.{},
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    df: ?api.Df = null,
    version: api.Version = .{},
    info: api.Info = .{},

    pub fn init(gpa: std.mem.Allocator) Snapshot {
        return .{ .arena = .init(gpa) };
    }

    pub fn deinit(self: *Snapshot) void {
        self.arena.deinit();
    }
};
