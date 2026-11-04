//! Compose project summaries, built once per snapshot. The rail and Stacks
//! page both need the same grouping; computing it here keeps their counts and
//! order in agreement and keeps the per-frame path to a slice read.
const std = @import("std");
const api = @import("engine/api.zig");

pub const Summary = struct {
    name: []const u8,
    /// Engine-order index of the first container in the project.
    first: usize,
    running: usize,
    total: usize,
    /// Engine-order row indices, so a card does not rescan every container to
    /// find its own members.
    members: []const usize = &.{},
};

/// One pass over the containers, in first-seen project order. `running` uses
/// `isUp`, so paused and restarting containers count as up just as they do in
/// the running totals.
pub fn summarize(allocator: std.mem.Allocator, containers: []const api.Container) ![]Summary {
    var by_name: std.StringArrayHashMapUnmanaged(usize) = .empty;
    defer by_name.deinit(allocator);
    var out: std.ArrayListUnmanaged(Summary) = .empty;
    errdefer out.deinit(allocator);

    var grouped: usize = 0;
    for (containers, 0..) |ct, i| {
        const name = ct.project() orelse continue;
        grouped += 1;
        const gop = try by_name.getOrPut(allocator, name);
        if (!gop.found_existing) {
            gop.value_ptr.* = out.items.len;
            try out.append(allocator, .{ .name = name, .first = i, .running = 0, .total = 0 });
        }
        const summary = &out.items[gop.value_ptr.*];
        summary.total += 1;
        if (ct.isUp()) summary.running += 1;
    }
    if (out.items.len == 0) return out.toOwnedSlice(allocator);

    // A second pass fills one flat member array. The map makes each lookup
    // O(1); the cursor array keeps the slices in engine order without another
    // per-project scan.
    const members = try allocator.alloc(usize, grouped);
    errdefer allocator.free(members);
    const cursor = try allocator.alloc(usize, out.items.len);
    defer allocator.free(cursor);
    var offset: usize = 0;
    for (out.items, 0..) |summary, i| {
        cursor[i] = offset;
        offset += summary.total;
    }
    for (containers, 0..) |ct, i| {
        const name = ct.project() orelse continue;
        const index = by_name.get(name) orelse continue;
        members[cursor[index]] = i;
        cursor[index] += 1;
    }
    offset = 0;
    for (out.items) |*summary| {
        summary.members = members[offset .. offset + summary.total];
        offset += summary.total;
    }
    return out.toOwnedSlice(allocator);
}

test "projects keep first-seen order and count paused containers as up" {
    const allocator = std.testing.allocator;
    var labels = try std.json.parseFromSlice(std.json.Value, allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer labels.deinit();
    var other = try std.json.parseFromSlice(std.json.Value, allocator, "{\"com.docker.compose.project\":\"api\"}", .{});
    defer other.deinit();

    const containers = [_]api.Container{
        .{ .Id = "1", .State = "running", .Labels = labels.value },
        .{ .Id = "2", .State = "paused", .Labels = labels.value },
        .{ .Id = "3", .State = "exited", .Labels = other.value },
        .{ .Id = "4", .State = "running", .Labels = labels.value },
        .{ .Id = "5", .State = "exited" },
    };
    const summaries = try summarize(allocator, &containers);
    defer allocator.free(summaries);
    defer {
        var total: usize = 0;
        for (summaries) |summary| total += summary.total;
        if (summaries.len > 0) allocator.free(summaries[0].members.ptr[0..total]);
    }

    try std.testing.expectEqual(@as(usize, 2), summaries.len);
    try std.testing.expectEqualStrings("shop", summaries[0].name);
    try std.testing.expectEqual(@as(usize, 0), summaries[0].first);
    try std.testing.expectEqual(@as(usize, 3), summaries[0].running);
    try std.testing.expectEqual(@as(usize, 3), summaries[0].total);
    try std.testing.expectEqualSlices(usize, &.{ 0, 1, 3 }, summaries[0].members);

    try std.testing.expectEqualStrings("api", summaries[1].name);
    try std.testing.expectEqual(@as(usize, 2), summaries[1].first);
    try std.testing.expectEqual(@as(usize, 0), summaries[1].running);
    try std.testing.expectEqual(@as(usize, 1), summaries[1].total);
    try std.testing.expectEqualSlices(usize, &.{2}, summaries[1].members);
}

test "a snapshot without compose labels has no projects" {
    const containers = [_]api.Container{
        .{ .Id = "1", .State = "running" },
        .{ .Id = "2", .State = "running" },
    };
    const summaries = try summarize(std.testing.allocator, &containers);
    defer std.testing.allocator.free(summaries);
    try std.testing.expectEqual(@as(usize, 0), summaries.len);
}
