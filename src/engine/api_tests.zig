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

test "published ports dedup by host port across address families" {
    const json =
        \\{"Ports":[{"PrivatePort":80,"PublicPort":8080,"Type":"tcp"},
        \\          {"PrivatePort":80,"PublicPort":8080,"Type":"tcp","IP":"::"},
        \\          {"PrivatePort":443,"PublicPort":8443,"Type":"tcp","IP":"192.168.1.5"},
        \\          {"PrivatePort":1900,"PublicPort":1900,"Type":"udp"},
        \\          {"PrivatePort":9000,"Type":"tcp"}]}
    ;
    var parsed = try std.json.parseFromSlice(api.Container, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    var out: [16]api.HostBinding = undefined;
    try std.testing.expectEqual(@as(usize, 2), parsed.value.publishedPorts(&out));
    try std.testing.expectEqual(@as(u16, 8080), out[0].port);
    try std.testing.expectEqualStrings("", out[0].ip); // the wildcard is "any", not an address
    try std.testing.expectEqual(@as(u16, 8443), out[1].port);
    try std.testing.expectEqualStrings("192.168.1.5", out[1].ip); // a specific binding is kept
}

test "published ports cap at the buffer and keep counting past it" {
    var ports: [24]api.Port = undefined;
    for (&ports, 0..) |*p, i| {
        p.* = .{ .PrivatePort = @intCast(1000 + i), .PublicPort = @intCast(30000 + i), .Type = "tcp" };
    }
    const ct: api.Container = .{ .Ports = &ports };
    var out: [16]api.HostBinding = undefined;
    try std.testing.expectEqual(@as(usize, 24), ct.publishedPorts(&out));
}

test "a container with nothing published fills nothing" {
    const ct: api.Container = .{ .Ports = &.{.{ .PrivatePort = 9000, .Type = "tcp" }} };
    var out: [16]api.HostBinding = undefined;
    try std.testing.expectEqual(@as(usize, 0), ct.publishedPorts(&out));
}

test "a paused container is up but not running" {
    const json = \\{"Id":"p1","Names":["/frozen"],"State":"paused"}
    ;
    var parsed = try std.json.parseFromSlice(api.Container, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expect(!parsed.value.isRunning());
    try std.testing.expect(parsed.value.isPaused());
    try std.testing.expect(parsed.value.isUp());

    const restarting: api.Container = .{ .State = "restarting" };
    try std.testing.expect(restarting.isUp());
    const exited: api.Container = .{ .State = "exited" };
    try std.testing.expect(!exited.isUp());
}

test "a created container is not up" {
    const created: api.Container = .{ .State = "created" };
    try std.testing.expect(!created.isUp());
    try std.testing.expect(!created.isRunning());
    try std.testing.expect(!created.isPaused());
}

test "health is read from the status suffix and nowhere else" {
    const healthy: api.Container = .{ .State = "running", .Status = "Up 2 hours (healthy)" };
    try std.testing.expectEqualStrings("healthy", healthy.health().?);
    const starting: api.Container = .{ .State = "running", .Status = "Up 4 seconds (health: starting)" };
    try std.testing.expectEqualStrings("starting", starting.health().?);
    const unhealthy: api.Container = .{ .State = "running", .Status = "Up 5 minutes (unhealthy)" };
    try std.testing.expectEqualStrings("unhealthy", unhealthy.health().?);

    // Parentheses that are exit codes, not health, and no parentheses at all.
    try std.testing.expect((api.Container{ .State = "exited", .Status = "Exited (0) 5 seconds ago" }).health() == null);
    try std.testing.expect((api.Container{ .State = "restarting", .Status = "Restarting (1) 2 seconds ago" }).health() == null);
    try std.testing.expect((api.Container{ .State = "running", .Status = "Up 2 hours" }).health() == null);
}
