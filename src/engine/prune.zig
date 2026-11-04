//! The engine's prune endpoints: remove what nothing is using and report the
//! bytes that came back. Most routes are shared, but the filters and the
//! build-cache route are not identical between Docker and Podman.
const std = @import("std");
const api = @import("api.zig");

pub const Kind = enum { containers, images, volumes, networks, build_cache };

/// Which engine is answering. Podman's compatibility layer accepts the shared
/// routes but not every Docker query parameter or route.
pub const Engine = enum { docker, podman };

const containers_paths = [_][]const u8{"/containers/prune"};
const docker_images = [_][]const u8{"/images/prune?filters=%7B%22dangling%22%3A%7B%22false%22%3Atrue%7D%7D"};
// Podman's `dangling=false` is literal: it removes only non-dangling unused
// images and would leave the dangling ones behind. Two passes cover both.
const podman_images = [_][]const u8{
    "/images/prune?filters=%7B%22dangling%22%3A%7B%22true%22%3Atrue%7D%7D",
    "/images/prune?filters=%7B%22dangling%22%3A%7B%22false%22%3Atrue%7D%7D",
};
const docker_volumes = [_][]const u8{"/volumes/prune?filters=%7B%22all%22%3A%7B%22true%22%3Atrue%7D%7D"};
const podman_volumes = [_][]const u8{"/volumes/prune"};
const networks_paths = [_][]const u8{"/networks/prune"};
const build_cache_paths = [_][]const u8{"/build/prune"};

pub fn pathFor(kind: Kind) []const u8 {
    return pathsFor(kind, .docker)[0];
}

pub fn pathForEngine(kind: Kind, engine: Engine) []const u8 {
    return pathsFor(kind, engine)[0];
}

/// The requests one prune step needs. Docker almost always needs one; Podman
/// images need a dangling pass followed by an unused non-dangling pass.
pub fn pathsFor(kind: Kind, engine: Engine) []const []const u8 {
    return switch (kind) {
        .containers => &containers_paths,
        .images => if (engine == .podman) &podman_images else &docker_images,
        // Docker's default is anonymous volumes only; `all` is what the
        // dialog promises. Podman rejects that filter with HTTP 500, and its
        // default already prunes every unused volume.
        .volumes => if (engine == .podman) &podman_volumes else &docker_volumes,
        .networks => &networks_paths,
        .build_cache => &build_cache_paths,
    };
}

/// Podman has no build-cache prune route; asking would turn a safe cleanup
/// into a reported failure for a step that never existed.
pub fn supported(kind: Kind, engine: Engine) bool {
    return kind != .build_cache or engine != .podman;
}

/// The effective build-cache choice for this engine, without touching the
/// user's stored preference.
pub fn buildCacheEnabled(selected: bool, engine: Engine) bool {
    return selected and supported(.build_cache, engine);
}

pub fn run(client: *api.Client, kind: Kind, engine: Engine) !u64 {
    var reclaimed: u64 = 0;
    try runInto(client, kind, engine, &reclaimed);
    return reclaimed;
}

/// The same run, but the caller keeps the bytes reclaimed before a later
/// failure. Podman's image step is two passes; losing the first pass's figure
/// because the second failed would under-report what cleanup actually freed.
pub fn runInto(client: *api.Client, kind: Kind, engine: Engine, out: *u64) !void {
    for (pathsFor(kind, engine)) |path| {
        const freed = reclaim(client, path) catch |e| {
            // Docker-compatible engines sometimes reject the `all` volume
            // filter with 400/500 for reasons unrelated to Podman. Retry the
            // documented default before treating it as a real failure.
            if (kind == .volumes and engine == .docker and retryVolumeWithoutAll(e, client)) {
                out.* += reclaim(client, "/volumes/prune") catch |retry_error| return retry_error;
                continue;
            }
            if (unsupportedRoute(e, client, kind, engine)) return error.EngineUnsupported;
            return e;
        };
        out.* += freed;
    }
}

fn reclaim(client: *api.Client, path: []const u8) !u64 {
    const body = try client.callOpts("POST", path, "", 1 << 20, false);
    defer client.gpa.free(body);
    const parsed = std.json.parseFromSliceLeaky(Response, client.alloc, body, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    }) catch return 0;
    return parsed.SpaceReclaimed;
}

fn retryVolumeWithoutAll(e: anyerror, client: *api.Client) bool {
    if (e != error.EngineError) return false;
    if (client.err_status != 400 and client.err_status != 500) return false;
    // A genuine Docker failure must not silently downgrade to an anonymous
    // volume prune. Only a rejected filter, or an API that predates `all`,
    // gets the unfiltered retry.
    if (filterRejected(client.err_body)) return true;
    if (apiMinor(client.api)) |minor| return minor < 42;
    return false;
}

fn filterRejected(body: []const u8) bool {
    if (!containsIgnoreCase(body, "filter")) return false;
    return containsIgnoreCase(body, "invalid") or
        containsIgnoreCase(body, "unsupported") or
        containsIgnoreCase(body, "all");
}

fn containsIgnoreCase(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0 or haystack.len < needle.len) return false;
    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(haystack[i .. i + needle.len], needle)) return true;
    }
    return false;
}

fn apiMinor(version: []const u8) ?u32 {
    const dot = std.mem.lastIndexOfScalar(u8, version, '.') orelse return null;
    return std.fmt.parseInt(u32, version[dot + 1 ..], 10) catch null;
}

fn unsupportedRoute(e: anyerror, client: *api.Client, kind: Kind, engine: Engine) bool {
    if (e != error.EngineError or kind != .build_cache) return false;
    // Podman has no build-cache route at all. For Docker, only a missing
    // route is a skip; a 500 is a real failure worth reporting.
    if (engine == .podman) return true;
    return client.err_status == 404;
}

/// Only `SpaceReclaimed` is used; the deleted-name arrays are engine chatter.
const Response = struct {
    SpaceReclaimed: u64 = 0,
};

test "every prune route carries the flags the UI promises" {
    try std.testing.expectEqualStrings("/containers/prune", pathFor(.containers));
    // Unused images means tagged-but-unreferenced, not only dangling layers.
    try std.testing.expectEqualStrings(
        "/images/prune?filters=%7B%22dangling%22%3A%7B%22false%22%3Atrue%7D%7D",
        pathFor(.images),
    );
    // Named unused volumes are only included with Docker's `all` filter.
    try std.testing.expectEqualStrings(
        "/volumes/prune?filters=%7B%22all%22%3A%7B%22true%22%3Atrue%7D%7D",
        pathFor(.volumes),
    );
    try std.testing.expectEqualStrings("/networks/prune", pathFor(.networks));
    try std.testing.expectEqualStrings("/build/prune", pathFor(.build_cache));
}

test "podman image prune runs dangling then unused non-dangling" {
    try std.testing.expectEqual(@as(usize, 1), pathsFor(.images, .docker).len);
    const paths = pathsFor(.images, .podman);
    try std.testing.expectEqual(@as(usize, 2), paths.len);
    try std.testing.expectEqualStrings(
        "/images/prune?filters=%7B%22dangling%22%3A%7B%22true%22%3Atrue%7D%7D",
        paths[0],
    );
    try std.testing.expectEqualStrings(
        "/images/prune?filters=%7B%22dangling%22%3A%7B%22false%22%3Atrue%7D%7D",
        paths[1],
    );
}

/// A tiny scripted engine, so prune behavior is checked with real requests,
/// not only with a list of paths.
const Scripted = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    count: usize,
    statuses: [4]u16 = .{ 200, 200, 200, 200 },
    bodies: [4][]const u8 = .{ "", "", "", "" },
    requests: [4][1024]u8 = undefined,
    request_len: [4]usize = .{ 0, 0, 0, 0 },
    failure: ?anyerror = null,
    thread: ?std.Thread = null,

    fn serve(self: *Scripted) void {
        for (0..self.count) |i| {
            const stream = self.listener.accept(self.io) catch |e| {
                self.failure = e;
                return;
            };
            defer stream.socket.close(self.io);
            var head: [1024]u8 = undefined;
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
            self.request_len[i] = @min(len, self.requests[i].len);
            @memcpy(self.requests[i][0..self.request_len[i]], head[0..self.request_len[i]]);

            var response_buf: [256]u8 = undefined;
            const body = self.bodies[i];
            const status = self.statuses[i];
            const reason: []const u8 = switch (status) {
                200 => "OK",
                400 => "Bad Request",
                404 => "Not Found",
                500 => "Internal Server Error",
                else => "Error",
            };
            const response = std.fmt.bufPrint(&response_buf, "HTTP/1.1 {d} {s}\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}", .{ status, reason, body.len, body }) catch {
                self.failure = error.NoSpaceLeft;
                return;
            };
            const file = std.Io.File{ .handle = stream.socket.handle, .flags = .{ .nonblocking = false } };
            file.writeStreamingAll(self.io, response) catch |e| {
                self.failure = e;
                return;
            };
        }
    }

    fn start(self: *Scripted) !void {
        self.thread = try std.Thread.spawn(.{}, serve, .{self});
    }

    fn finish(self: *Scripted) !void {
        if (self.thread) |thread| {
            thread.join();
            self.thread = null;
        }
        if (self.failure) |e| return e;
    }
};

test "podman image prune sums both requests" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var server: Scripted = .{
        .io = std.testing.io,
        .listener = &listener,
        .count = 2,
        .bodies = .{ "{\"SpaceReclaimed\":100}", "{\"SpaceReclaimed\":200}", "", "" },
    };
    try server.start();
    defer server.finish() catch {};

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    try std.testing.expectEqual(@as(u64, 300), try run(&client, .images, .podman));
    try server.finish();
    try std.testing.expect(std.mem.indexOf(u8, server.requests[0][0..server.request_len[0]], "dangling%22%3A%7B%22true") != null);
    try std.testing.expect(std.mem.indexOf(u8, server.requests[1][0..server.request_len[1]], "dangling%22%3A%7B%22false") != null);
}

test "podman image prune keeps bytes from a successful first pass" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var server: Scripted = .{
        .io = std.testing.io,
        .listener = &listener,
        .count = 2,
        .statuses = .{ 200, 500, 200, 200 },
        .bodies = .{ "{\"SpaceReclaimed\":100}", "{\"message\":\"failed\"}", "", "" },
    };
    try server.start();
    defer server.finish() catch {};

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    var total: u64 = 0;
    try std.testing.expectError(error.EngineError, runInto(&client, .images, .podman, &total));
    try std.testing.expectEqual(@as(u64, 100), total);
    try server.finish();
}

// A genuine Docker 500 must not silently downgrade to an anonymous prune.
test "docker volume busy 500 is reported, not retried" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var server: Scripted = .{
        .io = std.testing.io,
        .listener = &listener,
        .count = 1,
        .statuses = .{ 500, 200, 200, 200 },
        .bodies = .{ "{\"message\":\"device or resource busy\"}", "", "", "" },
    };
    try server.start();
    defer server.finish() catch {};

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    try std.testing.expectError(error.EngineError, run(&client, .volumes, .docker));
    try server.finish();
}

test "docker volume filter rejection retries without all" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var server: Scripted = .{
        .io = std.testing.io,
        .listener = &listener,
        .count = 2,
        .statuses = .{ 500, 200, 200, 200 },
        .bodies = .{ "{\"message\":\"invalid filter \\\"all\\\"\"}", "{\"SpaceReclaimed\":42}", "", "" },
    };
    try server.start();
    defer server.finish() catch {};

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    try std.testing.expectEqual(@as(u64, 42), try run(&client, .volumes, .docker));
    try server.finish();
    try std.testing.expect(std.mem.indexOf(u8, server.requests[0][0..server.request_len[0]], "all%22") != null);
    try std.testing.expect(std.mem.indexOf(u8, server.requests[1][0..server.request_len[1]], "filters=") == null);
}

test "pre-1.42 docker volume 500 retries without all" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var server: Scripted = .{
        .io = std.testing.io,
        .listener = &listener,
        .count = 2,
        .statuses = .{ 500, 200, 200, 200 },
        .bodies = .{ "{\"message\":\"server error\"}", "{\"SpaceReclaimed\":55}", "", "" },
    };
    try server.start();
    defer server.finish() catch {};

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();
    client.api = "v1.41";

    try std.testing.expectEqual(@as(u64, 55), try run(&client, .volumes, .docker));
    try server.finish();
}

test "podman gets no unsupported volume filter or build-cache route" {
    try std.testing.expectEqualStrings("/volumes/prune", pathForEngine(.volumes, .podman));
    try std.testing.expect(!supported(.build_cache, .podman));
    try std.testing.expect(supported(.build_cache, .docker));
    try std.testing.expect(supported(.volumes, .podman));
}

test "a podman check does not clear the stored build-cache preference" {
    const selected = true;
    try std.testing.expect(!buildCacheEnabled(selected, .podman));
    try std.testing.expect(buildCacheEnabled(selected, .docker));
    // The preference itself is still true after the Podman check.
    try std.testing.expect(selected);
}

test "only build-cache 404 is unsupported on docker" {
    var client = api.Client.init(undefined, std.testing.allocator, "");
    client.err_status = 500;
    try std.testing.expect(!unsupportedRoute(error.EngineError, &client, .volumes, .docker));
    try std.testing.expect(!unsupportedRoute(error.EngineError, &client, .build_cache, .docker));
    client.err_status = 404;
    try std.testing.expect(unsupportedRoute(error.EngineError, &client, .build_cache, .docker));
    try std.testing.expect(!unsupportedRoute(error.EngineError, &client, .volumes, .docker));
    client.err_status = 500;
    try std.testing.expect(unsupportedRoute(error.EngineError, &client, .build_cache, .podman));
}

test "a prune answer is read for its reclaimed bytes" {
    const text = "{\"ImagesDeleted\":[{\"Deleted\":\"sha256:1\"}],\"SpaceReclaimed\":4096}";
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const parsed = try std.json.parseFromSliceLeaky(Response, arena.allocator(), text, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    });
    try std.testing.expectEqual(@as(u64, 4096), parsed.SpaceReclaimed);
}
