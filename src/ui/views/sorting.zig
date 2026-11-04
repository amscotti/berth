//! Table ordering: one comparator per list, with the tests that pin their
//! case-insensitivity and their handling of unknown figures. Kept apart from
//! the drawing code so the rules can be exercised without a window.
const std = @import("std");
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const network_counts = @import("../../engine/network_counts.zig");
const metrics = @import("metrics.zig");

fn numOrder(a: f64, b: f64) std.math.Order {
    if (a < b) return .lt;
    if (a > b) return .gt;
    return .eq;
}

fn ordered(order: std.math.Order, desc: bool) bool {
    return if (desc) order == .gt else order == .lt;
}

pub const ContainerSort = struct {
    items: []const api.Container,
    /// Index-aligned with `items`, exactly as the snapshot stores it.
    stats: []const app.Stat,
    key: u8,
    desc: bool,

    pub fn lessThan(ctx: ContainerSort, a: usize, b: usize) bool {
        const x = ctx.items[a];
        const y = ctx.items[b];
        const sx = statAt(ctx.stats, a);
        const sy = statAt(ctx.stats, b);
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(x.name(), y.name()),
            2 => std.ascii.orderIgnoreCase(x.Image, y.Image),
            3 => std.ascii.orderIgnoreCase(x.health() orelse x.State, y.health() orelse y.State),
            4 => numOrder(statOrUnknown(sx, .cpu), statOrUnknown(sy, .cpu)),
            5 => numOrder(statOrUnknown(sx, .mem), statOrUnknown(sy, .mem)),
            6 => std.ascii.orderIgnoreCase(x.project() orelse "", y.project() orelse ""),
            7, 8, 9, 10 => numOrder(metrics.statKey(ctx.key, &x, sx), metrics.statKey(ctx.key, &y, sy)),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};

pub const ImageSort = struct {
    items: []const api.Image,
    key: u8,
    desc: bool,

    pub fn lessThan(ctx: ImageSort, a: usize, b: usize) bool {
        const x = ctx.items[a];
        const y = ctx.items[b];
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(x.label(), y.label()),
            2 => std.ascii.orderIgnoreCase(x.Id, y.Id),
            3 => numOrder(@floatFromInt(x.Size), @floatFromInt(y.Size)),
            // AGE is now − Created, so ascending age is descending creation
            // time: the newest image has the smallest age.
            4 => numOrder(@floatFromInt(y.Created), @floatFromInt(x.Created)),
            5 => std.ascii.orderIgnoreCase(if (x.dangling()) "dangling" else "ready", if (y.dangling()) "dangling" else "ready"),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};

pub const VolumeSort = struct {
    items: []const api.Volume,
    key: u8,
    desc: bool,

    pub fn lessThan(ctx: VolumeSort, a: usize, b: usize) bool {
        const x = ctx.items[a];
        const y = ctx.items[b];
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(x.Name, y.Name),
            2 => std.ascii.orderIgnoreCase(x.Driver, y.Driver),
            3 => numOrder(@floatFromInt(volumeSize(x)), @floatFromInt(volumeSize(y))),
            4 => numOrder(@floatFromInt(volumeUse(x)), @floatFromInt(volumeUse(y))),
            5 => std.ascii.orderIgnoreCase(x.Mountpoint, y.Mountpoint),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};

pub const NetworkSort = struct {
    items: []const api.Network,
    counts: network_counts.Counts,
    key: u8,
    desc: bool,

    pub fn lessThan(ctx: NetworkSort, a: usize, b: usize) bool {
        const x = ctx.items[a];
        const y = ctx.items[b];
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(x.Name, y.Name),
            2 => std.ascii.orderIgnoreCase(x.Driver, y.Driver),
            3 => std.ascii.orderIgnoreCase(x.Scope, y.Scope),
            4 => numOrder(
                @floatFromInt(network_counts.get(ctx.counts, x.Name)),
                @floatFromInt(network_counts.get(ctx.counts, y.Name)),
            ),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};

fn statAt(stats: []const app.Stat, index: usize) app.Stat {
    return if (index < stats.len) stats[index] else .{};
}

fn statOrUnknown(st: app.Stat, field: enum { cpu, mem }) f64 {
    if (!st.sampled) return -1;
    return switch (field) {
        .cpu => if (st.cpu_known) st.cpu else -1,
        .mem => @floatFromInt(st.mem),
    };
}

fn volumeSize(v: api.Volume) i64 {
    const u = v.UsageData orelse return -1;
    return u.Size;
}

fn volumeUse(v: api.Volume) i64 {
    const u = v.UsageData orelse return -1;
    return u.RefCount;
}

test "container sorting is case-insensitive and reversible" {
    const items = [_]api.Container{
        .{ .Id = "b", .Names = &.{"/Web"}, .Image = "nginx", .State = "running" },
        .{ .Id = "a", .Names = &.{"/api"}, .Image = "alpine", .State = "exited" },
    };
    const name_asc = ContainerSort{ .items = &items, .stats = &.{}, .key = 1, .desc = false };
    const name_desc = ContainerSort{ .items = &items, .stats = &.{}, .key = 1, .desc = true };
    try std.testing.expect(name_asc.lessThan(1, 0));
    try std.testing.expect(name_desc.lessThan(0, 1));

    const image_asc = ContainerSort{ .items = &items, .stats = &.{}, .key = 2, .desc = false };
    try std.testing.expect(image_asc.lessThan(1, 0));

    const state_asc = ContainerSort{ .items = &items, .stats = &.{}, .key = 3, .desc = false };
    try std.testing.expect(state_asc.lessThan(1, 0));

    // STATUS shows the health verdict when there is one; the sort agrees.
    const healthy: api.Container = .{ .Id = "h", .State = "running", .Status = "Up 1 hour (healthy)" };
    const unhealthy: api.Container = .{ .Id = "u", .State = "running", .Status = "Up 1 hour (unhealthy)" };
    const states = [_]api.Container{ healthy, unhealthy };
    const state_check = ContainerSort{ .items = &states, .stats = &.{}, .key = 3, .desc = false };
    try std.testing.expect(state_check.lessThan(0, 1));
    try std.testing.expect(!state_check.lessThan(1, 0));
}

test "container sort reads stats by row index" {
    const items = [_]api.Container{
        .{ .Id = "a", .State = "running" },
        .{ .Id = "b", .State = "running" },
    };
    // Index 1 carries 10 bytes and index 0 carries 20; the comparator must use
    // the row's own sample, not a search that could pick the other.
    const stats = [_]app.Stat{
        .{ .mem = 20, .sampled = true },
        .{ .mem = 10, .sampled = true },
    };
    const sort = ContainerSort{ .items = &items, .stats = &stats, .key = 5, .desc = false };
    try std.testing.expect(sort.lessThan(1, 0));
    const desc = ContainerSort{ .items = &items, .stats = &stats, .key = 5, .desc = true };
    try std.testing.expect(desc.lessThan(0, 1));
}

test "image sorting follows tag, size and dangling state" {
    const items = [_]api.Image{
        .{ .Id = "2", .RepoTags = &.{"nginx:alpine"}, .Size = 90 },
        .{ .Id = "1", .RepoTags = &.{"alpine:3.22"}, .Size = 12 },
        .{ .Id = "3", .RepoTags = &.{}, .Size = 1 },
    };
    const tag = ImageSort{ .items = items[0..], .key = 1, .desc = false };
    try std.testing.expect(tag.lessThan(1, 0)); // alpine < nginx
    try std.testing.expect(tag.lessThan(0, 2)); // nginx < untagged
    try std.testing.expect(!tag.lessThan(2, 0));

    const size = ImageSort{ .items = items[0..], .key = 3, .desc = false };
    try std.testing.expect(size.lessThan(2, 1)); // 1 B < 12
    try std.testing.expect(size.lessThan(1, 0)); // 12 < 90

    const status = ImageSort{ .items = items[0..], .key = 5, .desc = false };
    try std.testing.expect(status.lessThan(2, 0)); // dangling < ready
}

test "image AGE sorts newest first when ascending" {
    const items = [_]api.Image{
        .{ .Id = "old", .Created = 1000 },
        .{ .Id = "new", .Created = 3000 },
        .{ .Id = "mid", .Created = 2000 },
    };
    const age = ImageSort{ .items = items[0..], .key = 4, .desc = false };
    try std.testing.expect(age.lessThan(1, 2)); // newest before mid
    try std.testing.expect(age.lessThan(2, 0)); // mid before oldest
    try std.testing.expect(!age.lessThan(0, 2));
    const flip = ImageSort{ .items = items[0..], .key = 4, .desc = true };
    try std.testing.expect(flip.lessThan(0, 2)); // oldest first when reversed
}

test "volume sorting handles size, use and missing usage data" {
    const items = [_]api.Volume{
        .{ .Name = "b", .Driver = "local", .Mountpoint = "/b", .UsageData = .{ .Size = 20, .RefCount = 1 } },
        .{ .Name = "a", .Driver = "nfs", .Mountpoint = "/a", .UsageData = .{ .Size = 10, .RefCount = 5 } },
        .{ .Name = "c", .Driver = "local", .Mountpoint = "/c" },
    };
    const name = VolumeSort{ .items = items[0..], .key = 1, .desc = false };
    try std.testing.expect(name.lessThan(1, 0));
    const size = VolumeSort{ .items = items[0..], .key = 3, .desc = false };
    try std.testing.expect(size.lessThan(1, 0)); // 10 < 20
    try std.testing.expect(size.lessThan(2, 1)); // -1 (unknown) < 10
    const used = VolumeSort{ .items = items[0..], .key = 4, .desc = false };
    try std.testing.expect(used.lessThan(0, 1)); // 1 < 5
}

test "network sorting follows name, driver and scope" {
    const items = [_]api.Network{
        .{ .Name = "shop", .Driver = "bridge", .Scope = "local" },
        .{ .Name = "bridge", .Driver = "bridge", .Scope = "local" },
        .{ .Name = "host", .Driver = "host", .Scope = "local" },
    };
    const name = NetworkSort{ .items = items[0..], .counts = .{}, .key = 1, .desc = false };
    try std.testing.expect(name.lessThan(1, 0));
    const driver = NetworkSort{ .items = items[0..], .counts = .{}, .key = 2, .desc = false };
    try std.testing.expect(driver.lessThan(0, 2)); // bridge < host
    const desc = NetworkSort{ .items = items[0..], .counts = .{}, .key = 1, .desc = true };
    try std.testing.expect(desc.lessThan(0, 1));
}

test "network sort orders by the snapshot's attached counts" {
    const items = [_]api.Network{
        .{ .Name = "shop" },
        .{ .Name = "bridge" },
    };
    var counts: network_counts.Counts = .empty;
    defer counts.deinit(std.testing.allocator);
    try counts.put(std.testing.allocator, "shop", 2);
    try counts.put(std.testing.allocator, "bridge", 1);
    const most = NetworkSort{ .items = items[0..], .counts = counts, .key = 4, .desc = true };
    try std.testing.expect(most.lessThan(0, 1));
    const least = NetworkSort{ .items = items[0..], .counts = counts, .key = 4, .desc = false };
    try std.testing.expect(least.lessThan(1, 0));
}
