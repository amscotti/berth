//! Attached-container counts per network, built once per snapshot. The
//! `/networks` list leaves its Containers field empty on current engines, so
//! the count comes from each container's NetworkSettings. Rows and the count
//! sort then read one map instead of scanning the whole container list.
const std = @import("std");
const api = @import("api.zig");

pub const Counts = std.StringHashMapUnmanaged(usize);

pub fn build(allocator: std.mem.Allocator, containers: []const api.Container) !Counts {
    var counts: Counts = .empty;
    errdefer counts.deinit(allocator);
    for (containers) |ct| {
        const settings = ct.NetworkSettings orelse continue;
        if (settings != .object) continue;
        const nets = settings.object.get("Networks") orelse continue;
        if (nets != .object) continue;
        var it = nets.object.iterator();
        while (it.next()) |entry| {
            const gop = try counts.getOrPut(allocator, entry.key_ptr.*);
            if (!gop.found_existing) gop.value_ptr.* = 0;
            gop.value_ptr.* += 1;
        }
    }
    return counts;
}

pub fn get(counts: Counts, name: []const u8) usize {
    return counts.get(name) orelse 0;
}

test "network counts follow every container's network settings" {
    const allocator = std.testing.allocator;
    var one = try std.json.parseFromSlice(std.json.Value, allocator,
        \\{"Networks":{"bridge":{},"shop":{}}}
    , .{});
    defer one.deinit();
    var two = try std.json.parseFromSlice(std.json.Value, allocator,
        \\{"Networks":{"bridge":{}}}
    , .{});
    defer two.deinit();
    const containers = [_]api.Container{
        .{ .Id = "a", .NetworkSettings = one.value },
        .{ .Id = "b", .NetworkSettings = two.value },
        .{ .Id = "c" },
    };
    var counts = try build(allocator, &containers);
    defer counts.deinit(allocator);
    try std.testing.expectEqual(@as(usize, 2), get(counts, "bridge"));
    try std.testing.expectEqual(@as(usize, 1), get(counts, "shop"));
    try std.testing.expectEqual(@as(usize, 0), get(counts, "host"));
}
