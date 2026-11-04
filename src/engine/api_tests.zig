//! Docker API parsing tests, kept beside the types but out of `api.zig` so
//! the implementation stays inside the 600-line rule.
const std = @import("std");
const api = @import("api.zig");
const image_ops = @import("image_ops.zig");
const volumes_mod = @import("volumes.zig");
const Io = std.Io;

/// One-shot engine socket: accepts one connection, reads its request head,
/// writes `response`, and closes. Engines answer 304 with no body, so status
/// handling can be exercised without a running daemon.
const OneShot = struct {
    io: Io,
    listener: *Io.net.Server,
    response: []const u8,
    failure: ?anyerror = null,
    thread: ?std.Thread = null,

    fn serve(self: *OneShot) void {
        const stream = self.listener.accept(self.io) catch |e| {
            self.failure = e;
            return;
        };
        defer stream.socket.close(self.io);
        var head: [2048]u8 = undefined;
        var len: usize = 0;
        while (std.mem.indexOf(u8, head[0..len], "\r\n\r\n") == null) {
            if (len == head.len) break;
            const n = std.posix.read(stream.socket.handle, head[len..]) catch |e| {
                self.failure = e;
                return;
            };
            if (n == 0) break;
            len += n;
        }
        const file = Io.File{ .handle = stream.socket.handle, .flags = .{ .nonblocking = false } };
        file.writeStreamingAll(self.io, self.response) catch |e| {
            self.failure = e;
        };
    }

    fn start(self: *OneShot) !void {
        self.thread = try std.Thread.spawn(.{}, serve, .{self});
    }

    fn finish(self: *OneShot) !void {
        if (self.thread) |thread| {
            thread.join();
            self.thread = null;
        }
        if (self.failure) |e| return e;
    }
};

test "a 304 is an action success but not a body-parsing success" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    const not_modified = "HTTP/1.1 304 Not Modified\r\n\r\n";

    var strict: OneShot = .{ .io = std.testing.io, .listener = &listener, .response = not_modified };
    try strict.start();
    defer strict.finish() catch {};
    try std.testing.expectError(
        error.EngineError,
        client.callOptsTimeout("POST", "/images/create?fromImage=x", "", 1 << 20, false, 3, false),
    );
    try strict.finish();

    var action: OneShot = .{ .io = std.testing.io, .listener = &listener, .response = not_modified };
    try action.start();
    defer action.finish() catch {};
    try client.action("POST", "/containers/abc/start");
    try action.finish();

    // The Run dialog starts the container it just created through image_ops.
    var run: OneShot = .{ .io = std.testing.io, .listener = &listener, .response = not_modified };
    try run.start();
    defer run.finish() catch {};
    try image_ops.start(&client, "def");
    try run.finish();
}

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

test "version components identify podman, not the socket path" {
    const podman =
        \\{"Version":"5.7.0","ApiVersion":"1.51",
        \\ "Components":[{"Name":"Podman Engine","Version":"5.7.0"},{"Name":"API","Version":"5.7.0"}]}
    ;
    var a = try std.json.parseFromSlice(api.Version, std.testing.allocator, podman, .{ .ignore_unknown_fields = true });
    defer a.deinit();
    try std.testing.expect(a.value.isPodman());

    const docker =
        \\{"Version":"28.0.1","ApiVersion":"1.51",
        \\ "Components":[{"Name":"Engine","Version":"28.0.1"},{"Name":"containerd","Version":"2.1.0"}]}
    ;
    var b = try std.json.parseFromSlice(api.Version, std.testing.allocator, docker, .{ .ignore_unknown_fields = true });
    defer b.deinit();
    try std.testing.expect(!b.value.isPodman());
}

test "df reads the new shape and the old one" {
    const new_shape =
        \\{"LayersSize":999,"Images":[{"Size":500,"Containers":0}],
        \\ "ImageUsage":{"TotalCount":3,"ActiveCount":1,"TotalSize":100,"Reclaimable":60},
        \\ "ContainerUsage":{"TotalSize":10,"Reclaimable":5}}
    ;
    var a = try std.json.parseFromSlice(api.Df, std.testing.allocator, new_shape, .{ .ignore_unknown_fields = true });
    defer a.deinit();
    try std.testing.expectEqual(@as(i64, 100), a.value.imageBytes());
    try std.testing.expectEqual(@as(i64, 65), a.value.reclaimable());

    const old_shape =
        \\{"LayersSize":42,"Images":[{"Size":40},{"Size":2}]}
    ;
    var b = try std.json.parseFromSlice(api.Df, std.testing.allocator, old_shape, .{ .ignore_unknown_fields = true });
    defer b.deinit();
    try std.testing.expectEqual(@as(i64, 42), b.value.imageBytes());

    // No LayersSize: fall back to the image summaries rather than zero.
    const no_layers =
        \\{"Images":[{"Size":40},{"Size":2}]}
    ;
    var c = try std.json.parseFromSlice(api.Df, std.testing.allocator, no_layers, .{ .ignore_unknown_fields = true });
    defer c.deinit();
    try std.testing.expectEqual(@as(i64, 42), c.value.imageBytes());
}

test "legacy df totals and reclaimable come from the raw lists" {
    // API 1.51 shape: no *Usage summaries, only LayersSize and the stores.
    // LayersSize is deliberately different from the sum of image Sizes, and
    // the same image is repeated under two tags, as a multi-tag image is.
    const json =
        \\{"LayersSize":107514958,
        \\ "Images":[{"Id":"sha256:used","Size":100000000,"SharedSize":5900273,"Containers":1},
        \\           {"Id":"sha256:used","Size":100000000,"SharedSize":5900273,"Containers":1},
        \\           {"Id":"sha256:unused-a","Size":20000000,"SharedSize":10000000,"Containers":0},
        \\           {"Id":"sha256:unused-a","Size":20000000,"SharedSize":10000000,"Containers":0},
        \\           {"Id":"sha256:unused-b","Size":2000000,"SharedSize":1584769,"Containers":0}],
        \\ "Containers":[{"Id":"run","State":"running","SizeRw":40960},
        \\               {"Id":"stop","State":"exited","SizeRw":40960}],
        \\ "Volumes":[{"Name":"used","UsageData":{"Size":1000000,"RefCount":1}},
        \\            {"Name":"unused","UsageData":{"Size":2374271,"RefCount":0}}],
        \\ "BuildCache":[{"ID":"used","Size":500000,"InUse":true},
        \\               {"ID":"unused","Size":200000,"InUse":false},
        \\               {"ID":"shared","Size":9000000,"InUse":false,"Shared":true}]}
    ;
    var parsed = try std.json.parseFromSlice(api.Df, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    const store = parsed.value.stores();
    try std.testing.expectEqual(@as(i64, 107514958), store.images);
    try std.testing.expectEqual(@as(i64, 81920), store.containers);
    try std.testing.expectEqual(@as(i64, 3374271), store.volumes);
    // Shared build-cache records are not this store's own bytes.
    try std.testing.expectEqual(@as(i64, 700000), store.build_cache);

    const free = parsed.value.reclaimableByStore();
    // 107514958 - (100000000 - 5900273) = 13415231; duplicate Ids and
    // SharedSize are both load-bearing in that figure.
    try std.testing.expectEqual(@as(i64, 13415231), free.images);
    try std.testing.expectEqual(@as(i64, 40960), free.containers);
    try std.testing.expectEqual(@as(i64, 2374271), free.volumes);
    try std.testing.expectEqual(@as(i64, 200000), free.build_cache);
    try std.testing.expectEqual(@as(i64, 16030462), parsed.value.reclaimable());
}

test "legacy image reclaimable without LayersSize uses unique unused sizes" {
    const json =
        \\{"Images":[{"Id":"sha256:used","Size":100000000,"SharedSize":5900273,"Containers":1},
        \\           {"Id":"sha256:unused-a","Size":20000000,"SharedSize":10000000,"Containers":0},
        \\           {"Id":"sha256:unused-a","Size":20000000,"SharedSize":10000000,"Containers":0},
        \\           {"Id":"sha256:unused-b","Size":2000000,"SharedSize":1584769,"Containers":0}]}
    ;
    var parsed = try std.json.parseFromSlice(api.Df, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    // (20000000 - 10000000) + (2000000 - 1584769), with the repeated row
    // counted once.
    try std.testing.expectEqual(@as(i64, 10415231), parsed.value.reclaimable());
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
    const json =
        \\{"Id":"p1","Names":["/frozen"],"State":"paused"}
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

test "published ports come out in host-port order" {
    const json =
        \\{"Ports":[{"PrivatePort":443,"PublicPort":8443,"Type":"tcp"},
        \\          {"PrivatePort":80,"PublicPort":8080,"Type":"tcp"}]}
    ;
    var parsed = try std.json.parseFromSlice(api.Container, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    var out: [16]api.HostBinding = undefined;
    try std.testing.expectEqual(@as(usize, 2), parsed.value.publishedPorts(&out));
    try std.testing.expectEqual(@as(u16, 8080), out[0].port);
    try std.testing.expectEqual(@as(u16, 8443), out[1].port);
}

test "volume sizes fall back to the df document" {
    var volumes = [_]api.Volume{
        .{ .Name = "shop-data" },
        .{ .Name = "cache", .UsageData = .{ .Size = 7, .RefCount = 1 } },
    };
    var df_volumes = [_]api.Volume{
        .{ .Name = "shop-data", .UsageData = .{ .Size = 2048, .RefCount = 0 } },
        .{ .Name = "cache", .UsageData = .{ .Size = 999, .RefCount = 9 } },
    };
    const df = api.Df{ .Volumes = df_volumes[0..] };
    volumes_mod.mergeVolumeUsage(volumes[0..], df);
    try std.testing.expectEqual(@as(i64, 2048), volumes[0].UsageData.?.Size);
    // An existing measurement is not overwritten by a later guess.
    try std.testing.expectEqual(@as(i64, 7), volumes[1].UsageData.?.Size);
}
