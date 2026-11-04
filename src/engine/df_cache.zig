//! The last good `/system/df` document, owned outside the rotating snapshot
//! arena. df walks every store, so it is fetched on a slower cadence; between
//! fetches the previous figures stay readable, with their age. A compact copy
//! keeps only the fields the disk summaries, prune sheet and volume merge read,
//! so no string points back into a retired refresh arena.
const std = @import("std");
const api = @import("api.zig");

pub const Cache = struct {
    value: ?api.Df = null,
    /// Unix seconds when `value` was read; zero means never.
    at: i64 = 0,
    failed: bool = false,
    error_buf: [200]u8 = undefined,
    error_len: usize = 0,
    arena: ?std.heap.ArenaAllocator = null,
    /// Completed passes since the last attempt. Starting at `every - 1` makes
    /// the first pass fetch; a failed attempt also waits a full cadence.
    passes: u8 = every - 1,
    /// Bumped whenever a mutation or Refresh now makes an in-flight pass's df
    /// stale. A job captures it at start; a commit with an older value must not
    /// re-adopt the pre-mutation document.
    generation: u64 = 0,

    /// Five seconds per pass, so six passes is the 30-second disk cadence.
    pub const every: u8 = 6;

    pub fn wanted(self: *const Cache) bool {
        return self.passes >= every - 1;
    }

    pub fn errorText(self: *const Cache) []const u8 {
        return self.error_buf[0..self.error_len];
    }

    pub fn noteSkipped(self: *Cache) void {
        if (self.passes < every) self.passes += 1;
    }

    pub fn noteAttempt(self: *Cache) void {
        self.passes = 0;
    }

    /// A mutation or an explicit Refresh now invalidates the cached stores;
    /// the next pass must read `/system/df` even if the cadence is not due.
    pub fn invalidate(self: *Cache) void {
        self.passes = every - 1;
        self.generation +%= 1;
    }

    pub fn noteFailure(self: *Cache, message: []const u8) void {
        const n = @min(message.len, self.error_buf.len);
        @memcpy(self.error_buf[0..n], message[0..n]);
        self.error_len = n;
        self.failed = true;
        self.passes = 0;
    }

    /// Replaces the cached figures only after the copy succeeded: a failed
    /// copy keeps the previous good document rather than blanking the page.
    pub fn adopt(self: *Cache, gpa: std.mem.Allocator, df: api.Df, at: i64) bool {
        var fresh = std.heap.ArenaAllocator.init(gpa);
        const copy = clone(fresh.allocator(), df) catch {
            fresh.deinit();
            return false;
        };
        if (self.arena) |*old| old.deinit();
        self.arena = fresh;
        self.value = copy;
        self.at = at;
        self.failed = false;
        self.error_len = 0;
        self.passes = 0;
        return true;
    }

    pub fn reset(self: *Cache) void {
        if (self.arena) |*old| old.deinit();
        self.* = .{};
    }
};

fn clone(allocator: std.mem.Allocator, df: api.Df) !api.Df {
    var out: api.Df = .{
        .LayersSize = df.LayersSize,
        .ImageUsage = df.ImageUsage,
        .ContainerUsage = df.ContainerUsage,
        .VolumeUsage = df.VolumeUsage,
        .BuildCacheUsage = df.BuildCacheUsage,
    };
    if (df.Images) |items| {
        const copy = try allocator.alloc(api.Image, items.len);
        for (items, 0..) |item, i| {
            copy[i] = .{
                .Id = try allocator.dupe(u8, item.Id),
                .Size = item.Size,
                .SharedSize = item.SharedSize,
                .Containers = item.Containers,
            };
        }
        out.Images = copy;
    }
    if (df.Containers) |items| {
        const copy = try allocator.alloc(api.Container, items.len);
        for (items, 0..) |item, i| {
            copy[i] = .{ .State = try allocator.dupe(u8, item.State), .SizeRw = item.SizeRw };
        }
        out.Containers = copy;
    }
    if (df.Volumes) |items| {
        const copy = try allocator.alloc(api.Volume, items.len);
        for (items, 0..) |item, i| {
            copy[i] = .{ .Name = try allocator.dupe(u8, item.Name), .UsageData = item.UsageData };
        }
        out.Volumes = copy;
    }
    if (df.BuildCache) |items| {
        const copy = try allocator.alloc(api.Df.BuildCacheItem, items.len);
        for (items, 0..) |item, i| {
            copy[i] = .{ .ID = try allocator.dupe(u8, item.ID), .Size = item.Size, .InUse = item.InUse, .Shared = item.Shared };
        }
        out.BuildCache = copy;
    }
    return out;
}

test "df is wanted on the first pass and then once per cadence" {
    var cache: Cache = .{};
    try std.testing.expect(cache.wanted());
    cache.noteAttempt();
    for (0..Cache.every - 1) |_| {
        try std.testing.expect(!cache.wanted());
        cache.noteSkipped();
    }
    try std.testing.expect(cache.wanted());
    cache.noteAttempt();
    try std.testing.expect(!cache.wanted());
    const before = cache.generation;
    cache.invalidate();
    try std.testing.expect(cache.wanted());
    try std.testing.expectEqual(before + 1, cache.generation);
}

test "a failed df keeps the last good figures and reports its own error" {
    const df = api.Df{ .ImageUsage = .{ .TotalSize = 42 } };
    var cache: Cache = .{};
    defer cache.reset();
    try std.testing.expect(cache.adopt(std.testing.allocator, df, 100));
    cache.noteFailure("engine said no");
    try std.testing.expect(!cache.wanted());
    try std.testing.expectEqualStrings("engine said no", cache.errorText());
    try std.testing.expectEqual(@as(i64, 42), cache.value.?.imageBytes());
    try std.testing.expectEqual(@as(i64, 100), cache.at);
}

test "the cached copy owns its strings" {
    var source = std.heap.ArenaAllocator.init(std.testing.allocator);
    const df = api.Df{
        .Images = try source.allocator().alloc(api.Image, 1),
        .Containers = try source.allocator().alloc(api.Container, 1),
        .Volumes = try source.allocator().alloc(api.Volume, 1),
    };
    df.Images.?[0] = .{ .Id = "sha256:abc", .Size = 7, .Containers = 0 };
    df.Containers.?[0] = .{ .State = try source.allocator().dupe(u8, "exited"), .SizeRw = 42 };
    df.Volumes.?[0] = .{ .Name = "data", .UsageData = .{ .Size = 9, .RefCount = 0 } };
    const source_id = df.Images.?[0].Id;
    const source_state = df.Containers.?[0].State;
    const source_name = df.Volumes.?[0].Name;

    var cache: Cache = .{};
    defer cache.reset();
    try std.testing.expect(cache.adopt(std.testing.allocator, df, 1));
    source.deinit();
    const cached = cache.value.?;
    // Pointer identity, not equal contents: the cache must own separate
    // allocations, so the source arena going away cannot invalidate these.
    try std.testing.expect(@intFromPtr(cached.Images.?[0].Id.ptr) != @intFromPtr(source_id.ptr));
    try std.testing.expect(@intFromPtr(cached.Containers.?[0].State.ptr) != @intFromPtr(source_state.ptr));
    try std.testing.expect(@intFromPtr(cached.Volumes.?[0].Name.ptr) != @intFromPtr(source_name.ptr));
    try std.testing.expectEqualStrings("sha256:abc", cached.Images.?[0].Id);
    try std.testing.expectEqualStrings("data", cached.Volumes.?[0].Name);
    // A legacy df has no ContainerUsage; the reclaimable sum walks Container
    // State after the source arena is gone, so the copy must own that string.
    try std.testing.expectEqual(@as(i64, 42), cached.reclaimableByStore().containers);
}
