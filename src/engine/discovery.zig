//! Finding the engines a machine actually has.
//!
//! Nothing here hardcodes a single socket as "the" engine. Docker Desktop,
//! dockerd, Colima, Lima, Podman machine and rootless Podman all leave a
//! socket somewhere different, and several can be alive at once — so every
//! candidate is probed and the reachable ones are what the app offers.
const std = @import("std");
const api = @import("api.zig");
const http = @import("http_uds.zig");

extern "c" fn getenv(name: [*:0]const u8) ?[*:0]u8;
extern "c" fn getuid() c_uint;

/// An environment value, or null. libc's `getenv`, because Zig 0.17 moved
/// environment access behind `std.process.Init`.
fn env(name: [*:0]const u8) ?[]const u8 {
    const p = getenv(name) orelse return null;
    return std.mem.span(p);
}

pub const Kind = enum {
    docker_desktop,
    dockerd,
    colima,
    lima,
    podman_machine,
    podman,

    pub fn label(self: Kind) []const u8 {
        return switch (self) {
            .docker_desktop => "Docker Desktop",
            .dockerd => "dockerd",
            .colima => "Colima",
            .lima => "Lima",
            .podman_machine => "Podman machine",
            .podman => "Podman",
        };
    }
};

pub const Runtime = struct {
    name: []const u8,
    kind: Kind,
    socket: []const u8,
    reachable: bool = false,
    /// Why it is not reachable, when it is not.
    error_text: []const u8 = "",
    version: api.Version = .{},
    info: api.Info = .{},
};

pub const Found = struct {
    runtimes: []Runtime,

    pub fn deinit(self: Found, gpa: std.mem.Allocator) void {
        for (self.runtimes) |r| {
            gpa.free(r.name);
            gpa.free(r.socket);
            gpa.free(r.error_text);
        }
        gpa.free(self.runtimes);
    }
};

/// Every candidate socket, deduplicated, in the order a user would expect:
/// Docker Desktop first, then Colima, then Podman, then plain dockerd.
pub fn candidates(io: std.Io, gpa: std.mem.Allocator) ![][]const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (list.items) |s| gpa.free(s);
        list.deinit(gpa);
    }
    const home = env("HOME") orelse "";
    const xdg = env("XDG_RUNTIME_DIR");
    const uid = getuid();

    if (env("DOCKER_HOST")) |host| {
        if (std.mem.startsWith(u8, host, "unix://")) try add(gpa, &list, host["unix://".len..]);
    }

    const fixed = [_][]const u8{
        "/var/run/docker.sock",
        "/run/podman/podman.sock",
        "/var/run/podman/podman.sock",
    };
    for (fixed) |p| try add(gpa, &list, p);

    if (home.len > 0) {
        try addJoin(gpa, &list, home, "/.docker/run/docker.sock");
        try addJoin(gpa, &list, home, "/.docker/desktop/docker.sock");
        try addJoin(gpa, &list, home, "/.colima/default/docker.sock");
        try addJoin(gpa, &list, home, "/.colima/docker.sock");
        try addJoin(gpa, &list, home, "/.lima/default/sock/docker.sock");
        // Lima names the socket after the instance, so a VM called `docker`
        // lives under `~/.lima/docker/` and would otherwise be invisible.
        try addLimaInstances(io, gpa, &list, home);
    }
    if (xdg) |d| {
        try addJoin(gpa, &list, d, "/podman/podman.sock");
    } else {
        var buf: [64]u8 = undefined;
        const p = std.fmt.bufPrint(&buf, "/run/user/{d}/podman/podman.sock", .{uid}) catch "/run/user/0/podman/podman.sock";
        try add(gpa, &list, p);
    }
    return list.toOwnedSlice(gpa);
}

fn add(gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), path: []const u8) !void {
    if (path.len == 0) return;
    for (list.items) |existing| {
        if (std.mem.eql(u8, existing, path)) return;
    }
    try list.append(gpa, try gpa.dupe(u8, path));
}

fn addJoin(gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), base: []const u8, rest: []const u8) !void {
    const joined = try std.fmt.allocPrint(gpa, "{s}{s}", .{ base, rest });
    defer gpa.free(joined);
    try add(gpa, list, joined);
}

/// Every Lima instance, not just `default`: `limactl start docker` produces
/// `~/.lima/docker/sock/docker.sock`, which is a perfectly ordinary engine.
fn addLimaInstances(io: std.Io, gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), home: []const u8) !void {
    const root = try std.fmt.allocPrint(gpa, "{s}/.lima", .{home});
    defer gpa.free(root);
    const dir = std.Io.Dir.openDirAbsolute(io, root, .{ .iterate = true }) catch return;
    defer dir.close(io);
    var it = dir.iterate();
    while (it.next(io) catch null) |entry| {
        if (entry.kind != .directory) continue;
        // `_config` and friends are Lima's own bookkeeping, not instances.
        if (!isLimaInstance(entry.name)) continue;
        const sock = try std.fmt.allocPrint(gpa, "{s}/{s}/sock/docker.sock", .{ root, entry.name });
        defer gpa.free(sock);
        try add(gpa, list, sock);
    }
}

/// Lima's own bookkeeping directories start with an underscore; they are not
/// engine instances and have no socket.
pub fn isLimaInstance(name: []const u8) bool {
    return name.len > 0 and name[0] != '_';
}

/// Guesses the runtime from its socket path. Only used for the label; the
/// protocol is the same either way.
pub fn kindOf(path: []const u8) Kind {
    if (std.mem.indexOf(u8, path, ".colima") != null) return .colima;
    if (std.mem.indexOf(u8, path, ".lima") != null) return .lima;
    if (std.mem.indexOf(u8, path, "podman/machine") != null) return .podman_machine;
    if (std.mem.indexOf(u8, path, "podman") != null) return .podman;
    if (std.mem.indexOf(u8, path, ".docker/run") != null or
        std.mem.indexOf(u8, path, ".docker/desktop") != null) return .docker_desktop;
    return .dockerd;
}

/// Probes every candidate and returns the ones that answer, with their version
/// and info already read. Unreachable candidates are returned too, so the
/// Runtimes view can explain what was tried.
///
/// The probes run together: a machine with several stale Lima instances has a
/// dozen sockets, and waiting two seconds on each before the first snapshot is
/// a twenty-second launch. Only the reachable ones then pay for version/info,
/// on this thread, because their strings outlive the probe.
pub fn scan(gpa: std.mem.Allocator, io: std.Io) !Found {
    const paths = try candidates(io, gpa);
    defer {
        for (paths) |p| gpa.free(p);
        gpa.free(paths);
    }

    const runtimes = try gpa.alloc(Runtime, paths.len);
    for (runtimes) |*rt| rt.* = .{ .name = "", .kind = .dockerd, .socket = "" };
    errdefer {
        for (runtimes) |rt| {
            gpa.free(rt.name);
            gpa.free(rt.socket);
            gpa.free(rt.error_text);
        }
        gpa.free(runtimes);
    }

    for (paths, 0..) |path, i| {
        runtimes[i] = try runtimeFor(gpa, path);
    }

    // Bounded so a pathological candidate list cannot spawn without limit;
    // anything past the bound is probed in place, in the same total order.
    var threads: [16]std.Thread = undefined;
    var spawned: usize = 0;
    for (runtimes, 0..) |*rt, i| {
        if (i >= threads.len) break;
        threads[spawned] = std.Thread.spawn(.{}, probeOne, .{ io, rt }) catch break;
        spawned += 1;
    }
    for (threads[0..spawned]) |thread| thread.join();
    for (runtimes[spawned..]) |*rt| probeOne(io, rt);

    for (runtimes) |*rt| {
        if (!rt.reachable) {
            rt.error_text = try gpa.dupe(u8, "no answer on this socket");
            continue;
        }
        var client = api.Client.init(io, gpa, rt.socket);
        rt.version = client.version() catch api.Version{};
        rt.info = client.info() catch api.Info{};
        if (rt.info.Name.len > 0) {
            // Allocate before freeing: an OOM here must not leave `rt.name`
            // pointing at memory the error path frees again.
            const name = try std.fmt.allocPrint(gpa, "{s} · {s}", .{ rt.kind.label(), rt.info.Name });
            gpa.free(rt.name);
            rt.name = name;
        }
    }
    return .{ .runtimes = runtimes };
}

fn runtimeFor(gpa: std.mem.Allocator, path: []const u8) !Runtime {
    const name = try gpa.dupe(u8, kindOf(path).label());
    errdefer gpa.free(name);
    const socket = try gpa.dupe(u8, path);
    return .{ .name = name, .kind = kindOf(path), .socket = socket };
}

/// A worker owns a client on the page allocator and writes only its own
/// runtime's reachability; the strings it would allocate are read later, on
/// the scan's thread.
fn probeOne(io: std.Io, rt: *Runtime) void {
    var client = api.Client.init(io, std.heap.page_allocator, rt.socket);
    rt.reachable = probe(&client);
}

/// `GET /_ping` — the one call every Docker-compatible engine answers.
pub fn probe(client: *api.Client) bool {
    // Two seconds: a healthy engine answers /_ping at once, and a stale
    // forwarder must not make discovery take eight seconds per candidate.
    var conn = http.Conn.openTimeout(client.io, client.socket, 2) catch return false;
    defer conn.close();
    const res = conn.request(client.gpa, "GET", "/_ping", "", 4096) catch return false;
    defer res.deinit(client.gpa);
    return res.status == 200;
}

test "a socket path names its runtime" {
    try std.testing.expectEqual(Kind.colima, kindOf("/Users/x/.colima/default/docker.sock"));
    try std.testing.expectEqual(Kind.lima, kindOf("/Users/x/.lima/default/sock/docker.sock"));
    try std.testing.expectEqual(Kind.podman_machine, kindOf("/Users/x/.local/share/containers/podman/machine/applehv/podman.sock"));
    try std.testing.expectEqual(Kind.podman, kindOf("/run/user/1000/podman/podman.sock"));
    try std.testing.expectEqual(Kind.docker_desktop, kindOf("/Users/x/.docker/run/docker.sock"));
    try std.testing.expectEqual(Kind.dockerd, kindOf("/var/run/docker.sock"));
}

test "lima instance directories exclude lima's own bookkeeping" {
    try std.testing.expect(isLimaInstance("docker"));
    try std.testing.expect(isLimaInstance("berth-docker"));
    try std.testing.expect(!isLimaInstance("_config"));
    try std.testing.expect(!isLimaInstance(""));

    try std.testing.expectEqual(Kind.lima, kindOf("/Users/x/.lima/docker/sock/docker.sock"));
    try std.testing.expectEqual(Kind.podman_machine, kindOf("/Users/x/.local/share/containers/podman/machine/applehv/podman.sock"));
    try std.testing.expectEqual(Kind.podman, kindOf("/run/user/1000/podman/podman.sock"));
}

test "every runtime kind has a human label" {
    inline for (@typeInfo(Kind).@"enum".field_values) |value| {
        const kind: Kind = @fromBackingInt(@intCast(value));
        try std.testing.expect(kind.label().len > 0);
    }
}
