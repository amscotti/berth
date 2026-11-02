//! Docker API parsing tests, kept beside the types but out of `api.zig` so
//! the implementation stays inside the 600-line rule.
const std = @import("std");
const api = @import("api.zig");

test "a container's compose labels" {
    const json =
        \\{"Id":"abc","Names":["/web-1"],"Image":"nginx","State":"running",
        \\ "Labels":{"com.docker.compose.project":"shop","com.docker.compose.service":"web"}}
    ;
    var parsed = try std.json.parseFromSlice(api.Container, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    const c = parsed.value;
    try std.testing.expectEqualStrings("web-1", c.name());
    try std.testing.expectEqualStrings("shop", c.project().?);
    try std.testing.expectEqualStrings("web", c.service().?);
    try std.testing.expect(c.isRunning());
}

test "a container outside compose has no project" {
    const json =
        \\{"Id":"abc","Names":["/solo"],"State":"exited","Labels":{}}
    ;
    var parsed = try std.json.parseFromSlice(api.Container, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expect(parsed.value.project() == null);
    try std.testing.expect(!parsed.value.isRunning());
}

test "ports read as publish to expose" {
    const json =
        \\{"Ports":[{"PrivatePort":80,"PublicPort":8080,"Type":"tcp"},
        \\          {"PrivatePort":443,"PublicPort":8443,"Type":"tcp"},
        \\          {"PrivatePort":9000,"Type":"tcp"}]}
    ;
    var parsed = try std.json.parseFromSlice(api.Container, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    var buf: [64]u8 = undefined;
    try std.testing.expectEqualStrings("8080→80, 8443→443", parsed.value.portsText(&buf, 2));
}

test "df reads the new shape and the old one" {
    const new_shape =
        \\{"ImageUsage":{"TotalCount":3,"ActiveCount":1,"TotalSize":100,"Reclaimable":60},
        \\ "ContainerUsage":{"TotalSize":10,"Reclaimable":5}}
    ;
    var a = try std.json.parseFromSlice(api.Df, std.testing.allocator, new_shape, .{ .ignore_unknown_fields = true });
    defer a.deinit();
    try std.testing.expectEqual(@as(i64, 100), a.value.imageBytes());
    try std.testing.expectEqual(@as(i64, 65), a.value.reclaimable());

    const old_shape =
        \\{"LayersSize":1,"Images":[{"Size":40},{"Size":2}]}
    ;
    var b = try std.json.parseFromSlice(api.Df, std.testing.allocator, old_shape, .{ .ignore_unknown_fields = true });
    defer b.deinit();
    try std.testing.expectEqual(@as(i64, 42), b.value.imageBytes());
}

test "df total is every store, not just images" {
    const json =
        \\{"ImageUsage":{"TotalSize":100},"ContainerUsage":{"TotalSize":10},
        \\ "VolumeUsage":{"TotalSize":5},"BuildCacheUsage":{"TotalSize":2}}
    ;
    var parsed = try std.json.parseFromSlice(api.Df, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqual(@as(i64, 117), parsed.value.totalBytes());

    const old =
        \\{"Images":[{"Size":40},{"Size":2}],
        \\ "Volumes":[{"UsageData":{"Size":7}},{"UsageData":{"Size":-1}}],
        \\ "BuildCache":[{"Size":3}]}
    ;
    var b = try std.json.parseFromSlice(api.Df, std.testing.allocator, old, .{ .ignore_unknown_fields = true });
    defer b.deinit();
    // The -1 volume means "not measured" and must not shrink the total.
    try std.testing.expectEqual(@as(i64, 52), b.value.totalBytes());
}

test "attached counts come from container network settings" {
    const json =
        \\{"Id":"a","Names":["/web"],"NetworkSettings":{"Networks":{"bridge":{},"shop":{}}}}
    ;
    var parsed = try std.json.parseFromSlice(api.Container, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    const list = [_]api.Container{parsed.value};
    try std.testing.expectEqual(@as(usize, 1), api.attachedCount(&list, "bridge"));
    try std.testing.expectEqual(@as(usize, 1), api.attachedCount(&list, "shop"));
    try std.testing.expectEqual(@as(usize, 0), api.attachedCount(&list, "host"));

    const none = [_]api.Container{.{ .Id = "b" }};
    try std.testing.expectEqual(@as(usize, 0), api.attachedCount(&none, "bridge"));
}

test "an image is dangling without a usable tag" {
    const tagged = api.Image{ .RepoTags = &.{"alpine:3.22"} };
    try std.testing.expect(!tagged.dangling());
    try std.testing.expectEqualStrings("alpine:3.22", tagged.tag());

    const none = api.Image{ .RepoTags = &.{"<none>:<none>"} };
    try std.testing.expect(none.dangling());

    const absent = api.Image{};
    try std.testing.expect(absent.dangling());
    try std.testing.expectEqualStrings("<none>:<none>", absent.tag());
}

test "only the three default networks are built in" {
    try std.testing.expect((api.Network{ .Name = "bridge" }).builtin());
    try std.testing.expect((api.Network{ .Name = "host" }).builtin());
    try std.testing.expect((api.Network{ .Name = "none" }).builtin());
    try std.testing.expect(!(api.Network{ .Name = "shop" }).builtin());
}
