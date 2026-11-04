//! Filling volume sizes from the df document, separate from the API types so
//! `api.zig` can stay inside the line rule.
const std = @import("std");
const api = @import("api.zig");

/// `/volumes` omits UsageData on Docker; `/system/df` carries it. Fill the
/// missing sizes so the volume table matches the Disk page. One name→usage map
/// makes this O(V) instead of a scan of the df list per volume.
pub fn mergeVolumeUsage(gpa: std.mem.Allocator, volumes: []api.Volume, df: api.Df) !void {
    const usage = df.Volumes orelse return;
    var by_name: std.StringHashMapUnmanaged(?api.UsageData) = .empty;
    defer by_name.deinit(gpa);
    try by_name.ensureTotalCapacity(gpa, @intCast(usage.len));
    for (usage) |u| {
        const gop = try by_name.getOrPut(gpa, u.Name);
        // The old first-match scan stopped at the first duplicate; keep that
        // answer rather than letting a later row win.
        if (!gop.found_existing) gop.value_ptr.* = u.UsageData;
    }
    for (volumes) |*v| {
        // -1 is "not measured", so the df may still fill it.
        if (v.UsageData) |u| if (u.Size >= 0) continue;
        if (by_name.get(v.Name)) |maybe| {
            if (maybe) |next| {
                // Keep a known RefCount when the df did not measure size.
                if (next.Size >= 0) v.UsageData = next;
            }
        }
    }
}

test "a large volume list merges in one map pass" {
    const gpa = std.testing.allocator;
    const n = 10_000;
    const volumes = try gpa.alloc(api.Volume, n);
    defer gpa.free(volumes);
    const df_volumes = try gpa.alloc(api.Volume, n);
    defer gpa.free(df_volumes);
    for (volumes, 0..) |*v, i| {
        v.* = .{ .Name = try std.fmt.allocPrint(gpa, "vol-{d}", .{i}) };
    }
    defer for (volumes) |v| gpa.free(v.Name);
    // Reverse the df order so a linear per-volume scan would do the most work.
    for (df_volumes, 0..) |*v, i| {
        const slot = n - 1 - i;
        v.* = .{
            .Name = try std.fmt.allocPrint(gpa, "vol-{d}", .{slot}),
            .UsageData = .{ .Size = @intCast(slot + 1), .RefCount = 0 },
        };
    }
    defer for (df_volumes) |v| gpa.free(v.Name);

    try mergeVolumeUsage(gpa, volumes, .{ .Volumes = df_volumes });
    try std.testing.expectEqual(@as(i64, 1), volumes[0].UsageData.?.Size);
    try std.testing.expectEqual(@as(i64, n), volumes[n - 1].UsageData.?.Size);
}

test "a missing df leaves volume usage alone" {
    var volumes = [_]api.Volume{.{ .Name = "data" }};
    try mergeVolumeUsage(std.testing.allocator, &volumes, .{});
    try std.testing.expect(volumes[0].UsageData == null);
}

test "duplicate df names keep the first answer" {
    var volumes = [_]api.Volume{
        .{ .Name = "data" },
        .{ .Name = "unknown" },
    };
    var df_volumes = [_]api.Volume{
        .{ .Name = "data", .UsageData = .{ .Size = 10, .RefCount = 1 } },
        .{ .Name = "data", .UsageData = .{ .Size = 99, .RefCount = 9 } },
        // The first match had no measurement, so the old scan stopped there.
        .{ .Name = "unknown" },
        .{ .Name = "unknown", .UsageData = .{ .Size = 7, .RefCount = 1 } },
    };
    try mergeVolumeUsage(std.testing.allocator, &volumes, .{ .Volumes = &df_volumes });
    try std.testing.expectEqual(@as(i64, 10), volumes[0].UsageData.?.Size);
    try std.testing.expectEqual(@as(i64, 1), volumes[0].UsageData.?.RefCount);
    try std.testing.expect(volumes[1].UsageData == null);
}
