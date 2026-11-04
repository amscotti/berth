//! Finding the engines a machine actually has.
//!
//! Nothing here hardcodes a single socket as "the" engine. Docker Desktop,
//! dockerd, Colima, Lima, Podman machine and rootless Podman all leave a
//! socket somewhere different, and several can be alive at once — so every
//! candidate is probed and the reachable ones are what the app offers.
const std = @import("std");
const api = @import("api.zig");
const http = @import("http_uds.zig");
const podman_config = @import("podman_config.zig");
const socket_path = @import("socket_path.zig");

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
    orbstack,
    rancher,
    dockerd,
    colima,
    lima,
    podman_machine,
    podman,

    pub fn label(self: Kind) []const u8 {
        return switch (self) {
            .docker_desktop => "Docker Desktop",
            .orbstack => "OrbStack",
            .rancher => "Rancher Desktop",
            .dockerd => "Dockerd",
            .colima => "Colima",
            .lima => "Lima",
            .podman_machine => "Podman machine",
            .podman => "Podman",
        };
    }
};

pub const Dialect = enum { docker, podman };

pub const Runtime = struct {
    name: []const u8 = "",
    kind: Kind = .dockerd,
    /// What the engine says it is, from `/version` components. The socket
    /// path cannot tell Podman's Docker-compatible API from Docker's.
    dialect: Dialect = .docker,
    socket: []const u8 = "",
    /// The versioned prefix (`v1.56`) for this engine, owned here so the UI
    /// client can use it after discovery's client is gone.
    api: []const u8 = "",
    reachable: bool = false,
    /// Why it is not reachable, when it is not.
    error_text: []const u8 = "",
    version: api.Version = .{},
    info: api.Info = .{},
    /// `/info` answered, so `info.ContainersRunning` is a real count rather
    /// than the zero value of a failed read.
    info_read: bool = false,
};

/// Free every string a runtime owns. A runtime's version/info come from the
/// discovery client's gpa, not from a snapshot arena, so they are freed here.
pub fn freeRuntime(gpa: std.mem.Allocator, rt: *Runtime) void {
    gpa.free(rt.name);
    gpa.free(rt.socket);
    gpa.free(rt.error_text);
    gpa.free(rt.api);
    gpa.free(rt.version.Version);
    gpa.free(rt.version.ApiVersion);
    gpa.free(rt.version.MinAPIVersion);
    gpa.free(rt.version.Os);
    gpa.free(rt.version.Arch);
    gpa.free(rt.version.GoVersion);
    if (rt.version.Components) |components| {
        for (components) |component| gpa.free(component.Name);
        gpa.free(components);
    }
    gpa.free(rt.info.ID);
    gpa.free(rt.info.Name);
    gpa.free(rt.info.ServerVersion);
    gpa.free(rt.info.OperatingSystem);
    gpa.free(rt.info.OSType);
    gpa.free(rt.info.Architecture);
    gpa.free(rt.info.DockerRootDir);
    gpa.free(rt.info.Driver);
    gpa.free(rt.info.LoggingDriver);
    gpa.free(rt.info.CgroupVersion);
    rt.* = .{};
}

pub const Found = struct {
    runtimes: []Runtime,

    pub fn deinit(self: Found, gpa: std.mem.Allocator) void {
        for (self.runtimes) |*r| freeRuntime(gpa, r);
        gpa.free(self.runtimes);
    }
};

/// Every candidate socket, deduplicated, in the order a user would expect:
/// Docker Desktop first, then Colima, then Podman, then plain dockerd.
pub fn candidates(io: std.Io, gpa: std.mem.Allocator, preferred: []const u8) ![][]const u8 {
    var list: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (list.items) |s| gpa.free(s);
        list.deinit(gpa);
    }
    const home = env("HOME") orelse "";
    const xdg = env("XDG_RUNTIME_DIR");
    const uid = getuid();

    // The socket the user chose last time comes first: it wins ties, and it
    // is probed even when it is not one of the paths berth knows by heart.
    if (preferred.len > 0) try addResolved(io, gpa, &list, preferred);

    if (env("DOCKER_HOST")) |host| {
        if (std.mem.startsWith(u8, host, "unix://")) try addResolved(io, gpa, &list, host["unix://".len..]);
    }

    if (home.len > 0) {
        try addJoin(io, gpa, &list, home, "/.docker/run/docker.sock");
        try addJoin(io, gpa, &list, home, "/.docker/desktop/docker.sock");
        try addJoin(io, gpa, &list, home, "/.orbstack/run/docker.sock");
        try addJoin(io, gpa, &list, home, "/.rd/docker.sock");
        try addJoin(io, gpa, &list, home, "/Library/Containers/com.docker.docker/Data/docker.sock");
        try addJoin(io, gpa, &list, home, "/.colima/default/docker.sock");
        try addJoin(io, gpa, &list, home, "/.colima/docker.sock");
        try addJoin(io, gpa, &list, home, "/.lima/default/sock/docker.sock");
    }
    // Podman 5's named runtime sockets come before the 4.x alias, which may
    // be a symlink to whichever machine is active.
    try addPodmanRuntimeSockets(io, gpa, &list);
    try addPodmanMachines(io, gpa, &list, home);
    if (home.len > 0) {
        // Lima names the socket after the instance, so a VM called `docker`
        // lives under `~/.lima/docker/` and would otherwise be invisible.
        try addLimaInstances(io, gpa, &list, home);
    }
    if (xdg) |d| {
        try addJoin(io, gpa, &list, d, "/podman/podman.sock");
    } else {
        var buf: [64]u8 = undefined;
        const p = std.fmt.bufPrint(&buf, "/run/user/{d}/podman/podman.sock", .{uid}) catch "/run/user/0/podman/podman.sock";
        try addIfExists(io, gpa, &list, p);
    }
    // Fixed system paths come last: a Docker Desktop symlink at /var/run
    // should not win the label over the app that actually owns the socket.
    const fixed = [_][]const u8{
        "/var/run/docker.sock",
        "/run/podman/podman.sock",
        "/var/run/podman/podman.sock",
    };
    for (fixed) |p| try addIfExists(io, gpa, &list, p);
    return list.toOwnedSlice(gpa);
}

fn add(gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), path: []const u8) !void {
    if (path.len == 0) return;
    for (list.items) |existing| {
        if (std.mem.eql(u8, existing, path)) return;
    }
    try list.append(gpa, try gpa.dupe(u8, path));
}

fn addJoin(io: std.Io, gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), base: []const u8, rest: []const u8) !void {
    const joined = try std.fmt.allocPrint(gpa, "{s}{s}", .{ base, rest });
    defer gpa.free(joined);
    try addIfExists(io, gpa, list, joined);
}

pub const canonicalSocketPath = socket_path.canonicalSocketPath;
pub const sameSocket = socket_path.sameSocket;

/// A path that is not there is not a runtime. Filtering keeps stopped
/// engines from filling the chooser with sockets that can never answer.
fn addIfExists(io: std.Io, gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), path: []const u8) !void {
    const resolved = try socket_path.canonicalSocketPath(io, gpa, path);
    defer gpa.free(resolved);
    std.Io.Dir.accessAbsolute(io, resolved, .{}) catch return;
    try add(gpa, list, resolved);
}

/// Podman machine moved from qemu to applehv to libkrun and lets people name
/// machines; enumerate the provider directories rather than guessing one.
/// Podman 5 also moved the socket out of the VM-type directory into the
/// per-user runtime directory, named after the machine.
fn addPodmanMachines(io: std.Io, gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), home: []const u8) !void {
    const data_home = env("XDG_DATA_HOME") orelse "";
    const root = if (data_home.len > 0)
        try std.fmt.allocPrint(gpa, "{s}/containers/podman/machine", .{data_home})
    else if (home.len > 0)
        try std.fmt.allocPrint(gpa, "{s}/.local/share/containers/podman/machine", .{home})
    else
        return;
    defer gpa.free(root);
    // Podman 5's root socket is often a symlink to the active machine's
    // runtime socket; resolving it lets the named candidate win instead of
    // showing the same engine twice under a nameless alias.
    const alias = try std.fmt.allocPrint(gpa, "{s}/podman.sock", .{root});
    defer gpa.free(alias);
    try addIfExists(io, gpa, list, alias);
    if (std.Io.Dir.openDirAbsolute(io, root, .{ .iterate = true })) |dir| {
        defer dir.close(io);
        var it = dir.iterate();
        while (it.next(io) catch null) |entry| {
            if (entry.kind != .directory) continue;
            const rest = try std.fmt.allocPrint(gpa, "/{s}/podman.sock", .{entry.name});
            defer gpa.free(rest);
            try addJoin(io, gpa, list, root, rest);
        }
    } else |_| {}
}

/// Adds a path even when it is not answering, after resolving a symlink so a
/// remembered socket and the engine's real socket do not appear twice.
fn addResolved(io: std.Io, gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), path: []const u8) !void {
    const resolved = try socket_path.canonicalSocketPath(io, gpa, path);
    defer gpa.free(resolved);
    try add(gpa, list, resolved);
}

/// Podman 5 names the API socket `<machine>-api.sock` under the runtime
/// directory. macOS puts it under TMPDIR, Linux under XDG_RUNTIME_DIR or the
/// system runtime directory.
fn addPodmanRuntimeSockets(io: std.Io, gpa: std.mem.Allocator, list: *std.ArrayList([]const u8)) !void {
    const tmp = env("TMPDIR") orelse "/tmp";
    try addPodmanRuntimeRoot(io, gpa, list, if (tmp.len > 0) tmp else "/tmp");
    if (env("XDG_RUNTIME_DIR")) |xdg| {
        if (xdg.len > 0) try addPodmanRuntimeRoot(io, gpa, list, xdg);
    }
    // XDG_RUNTIME_DIR can point somewhere else; the per-uid fallback is still
    // where Podman puts machine sockets on many Linux installs.
    var uid_buf: [64]u8 = undefined;
    const uid_root = std.fmt.bufPrint(&uid_buf, "/run/user/{d}", .{getuid()}) catch "/run/user/0";
    try addPodmanRuntimeRoot(io, gpa, list, uid_root);
    try addPodmanRuntimeRoot(io, gpa, list, "/run");
}

fn addPodmanRuntimeRoot(io: std.Io, gpa: std.mem.Allocator, list: *std.ArrayList([]const u8), base: []const u8) !void {
    const clean = std.mem.trimEnd(u8, base, "/");
    const root = try std.fmt.allocPrint(gpa, "{s}/podman", .{if (clean.len > 0) clean else "/"});
    defer gpa.free(root);
    // Named `*-api.sock` sockets are the machines a lifecycle command can
    // address; the unqualified `podman.sock` may be an alias for whichever
    // one is active, so it comes after them.
    if (std.Io.Dir.openDirAbsolute(io, root, .{ .iterate = true })) |dir| {
        defer dir.close(io);
        var it = dir.iterate();
        while (it.next(io) catch null) |entry| {
            if (!isPodmanMachineSocket(entry.name)) continue;
            const path = try std.fmt.allocPrint(gpa, "{s}/{s}", .{ root, entry.name });
            defer gpa.free(path);
            try addIfExists(io, gpa, list, path);
        }
    } else |_| {}
    try addJoin(io, gpa, list, root, "/podman.sock");
}

/// A machine socket's filename carries the machine name; the VM type used to.
pub fn isPodmanMachineSocket(name: []const u8) bool {
    return std.mem.endsWith(u8, name, "-api.sock");
}

/// The machine name encoded in a Podman 5 socket filename, or null for the
/// unnamed 4.x layout.
pub fn podmanMachineName(path: []const u8) ?[]const u8 {
    const slash = std.mem.lastIndexOfScalar(u8, path, '/') orelse return null;
    const base = path[slash + 1 ..];
    if (!isPodmanMachineSocket(base)) return null;
    return base[0 .. base.len - "-api.sock".len];
}

/// Podman 4's socket lives under a VM-type directory, not the machine name.
/// The only name available there is the one config file Podman keeps for that
/// provider; when there are zero or several, the caller must use the default.
pub fn podman4MachineName(io: std.Io, socket: []const u8, out: []u8) ?[]const u8 {
    const home = env("HOME") orelse "";
    const xdg_config = env("XDG_CONFIG_HOME") orelse "";
    return podman4MachineNameIn(io, home, xdg_config, socket, out);
}

/// Podman 4's socket lives under a machine name (4.0–4.4) or a VM type
/// (4.5+); the config directory disambiguates. Kept here so callers have one
/// discovery entry point.
pub fn podman4MachineNameIn(io: std.Io, home: []const u8, xdg_config: []const u8, socket: []const u8, out: []u8) ?[]const u8 {
    return podman_config.machineName(io, home, xdg_config, socket, out);
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
        try addIfExists(io, gpa, list, sock);
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
    if (std.mem.indexOf(u8, path, ".orbstack") != null) return .orbstack;
    if (std.mem.indexOf(u8, path, "/.rd/") != null) return .rancher;
    if (std.mem.indexOf(u8, path, ".colima") != null) return .colima;
    if (std.mem.indexOf(u8, path, ".lima") != null) return .lima;
    if (std.mem.indexOf(u8, path, "podman/machine") != null) return .podman_machine;
    // Podman 5 puts `<machine>-api.sock` under the runtime dir, so the path
    // no longer contains the 4.x `machine` directory.
    if (std.mem.indexOf(u8, path, "/podman/") != null) {
        if (podmanMachineName(path) != null) return .podman_machine;
    }
    if (std.mem.indexOf(u8, path, "podman") != null) return .podman;
    if (std.mem.indexOf(u8, path, ".docker/run") != null or
        std.mem.indexOf(u8, path, ".docker/desktop") != null or
        std.mem.indexOf(u8, path, "com.docker.docker") != null) return .docker_desktop;
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
pub fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !Found {
    const paths = try candidates(io, gpa, preferred);
    defer {
        for (paths) |p| gpa.free(p);
        gpa.free(paths);
    }

    const runtimes = try gpa.alloc(Runtime, paths.len);
    for (runtimes) |*rt| rt.* = .{ .name = "", .kind = .dockerd, .socket = "" };
    errdefer {
        for (runtimes) |*rt| freeRuntime(gpa, rt);
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
        rt.dialect = if (rt.version.isPodman()) .podman else .docker;
        if (rt.version.ApiVersion.len > 0) {
            rt.api = std.fmt.allocPrint(gpa, "v{s}", .{rt.version.ApiVersion}) catch "";
            if (rt.api.len > 0) client.api = rt.api;
        }
        if (client.info()) |info| {
            rt.info = info;
            rt.info_read = true;
        } else |_| {
            gpa.free(rt.error_text);
            rt.error_text = try gpa.dupe(u8, if (client.err_body.len > 0)
                client.err_body
            else
                "could not read engine info");
        }
        if (client.err_body.len > 0) {
            gpa.free(client.err_body);
            client.err_body = "";
        }
        if (rt.info.Name.len > 0) {
            // Allocate before freeing: an OOM here must not leave `rt.name`
            // pointing at memory the error path frees again.
            const name = try std.fmt.allocPrint(gpa, "{s} · {s}", .{ rt.kind.label(), rt.info.Name });
            gpa.free(rt.name);
            rt.name = name;
        }
    }

    // Docker Desktop commonly exposes the same engine through /var/run and
    // ~/.docker/run. They are one engine: keep the first, drop the alias, so
    // the count and the chooser do not claim two.
    var unique: usize = 0;
    for (runtimes, 0..) |rt, i| {
        if (isDuplicate(runtimes[0..i], rt)) continue;
        unique += 1;
    }
    if (unique == runtimes.len) return .{ .runtimes = runtimes };

    const kept = try gpa.alloc(Runtime, unique);
    var n: usize = 0;
    for (runtimes, 0..) |*rt, i| {
        if (isDuplicate(runtimes[0..i], rt.*)) {
            freeRuntime(gpa, rt);
            continue;
        }
        kept[n] = rt.*;
        n += 1;
    }
    gpa.free(runtimes);
    return .{ .runtimes = kept };
}

fn isDuplicate(earlier: []const Runtime, rt: Runtime) bool {
    // Podman's info.ID is per request, so comparing IDs would leave every
    // alias visible. Path canonicalization already collapses those.
    if (!rt.reachable or rt.dialect == .podman or rt.info.ID.len == 0) return false;
    for (earlier) |prev| {
        if (prev.reachable and prev.dialect == .docker and prev.info.ID.len > 0 and std.mem.eql(u8, prev.info.ID, rt.info.ID)) return true;
    }
    return false;
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
    // Three seconds: a healthy engine answers /_ping at once, but a Docker
    // Desktop socket that is still waking up can miss a two-second window.
    // The probes run together, so a slow candidate delays the scan only once.
    var conn = http.Conn.openTimeout(client.io, client.socket, 3) catch return false;
    defer conn.close();
    const res = conn.request(client.gpa, "GET", "/_ping", "", 4096) catch return false;
    defer res.deinit(client.gpa);
    return res.status == 200;
}

test "a socket path names its runtime" {
    try std.testing.expectEqual(Kind.colima, kindOf("/Users/x/.colima/default/docker.sock"));
    try std.testing.expectEqual(Kind.lima, kindOf("/Users/x/.lima/default/sock/docker.sock"));
    try std.testing.expectEqual(Kind.podman_machine, kindOf("/Users/x/.local/share/containers/podman/machine/applehv/podman.sock"));
    try std.testing.expectEqual(Kind.podman_machine, kindOf("/var/folders/x/T/podman/podman-machine-default-api.sock"));
    try std.testing.expectEqual(Kind.podman, kindOf("/run/user/1000/podman/podman.sock"));
    try std.testing.expectEqual(Kind.docker_desktop, kindOf("/Users/x/.docker/run/docker.sock"));
    try std.testing.expectEqual(Kind.orbstack, kindOf("/Users/x/.orbstack/run/docker.sock"));
    try std.testing.expectEqual(Kind.rancher, kindOf("/Users/x/.rd/docker.sock"));
    try std.testing.expectEqual(Kind.dockerd, kindOf("/var/run/docker.sock"));
}

test "podman 5 sockets carry the machine name in the filename" {
    try std.testing.expect(isPodmanMachineSocket("podman-machine-default-api.sock"));
    try std.testing.expect(!isPodmanMachineSocket("podman.sock"));
    try std.testing.expectEqualStrings(
        "podman-machine-default",
        podmanMachineName("/var/folders/x/T/podman/podman-machine-default-api.sock").?,
    );
    // Podman 4's socket lives in a VM-type directory and has no machine name.
    try std.testing.expect(podmanMachineName("/Users/x/.local/share/containers/podman/machine/applehv/podman.sock") == null);
}

test "podman runtime sockets are scanned under xdg and system roots" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    try tmp.dir.createDirPath(io, "xdg/podman");
    try tmp.dir.createDirPath(io, "run/podman");
    try tmp.dir.createDirPath(io, "run/user/1000/podman");
    try tmp.dir.writeFile(io, .{ .sub_path = "xdg/podman/work-api.sock", .data = "" });
    try tmp.dir.writeFile(io, .{ .sub_path = "run/podman/system-api.sock", .data = "" });
    try tmp.dir.writeFile(io, .{ .sub_path = "run/user/1000/podman/rootless-api.sock", .data = "" });

    const xdg = try std.fmt.allocPrint(gpa, "{s}/xdg", .{base});
    defer gpa.free(xdg);
    const run = try std.fmt.allocPrint(gpa, "{s}/run", .{base});
    defer gpa.free(run);
    const rootless = try std.fmt.allocPrint(gpa, "{s}/run/user/1000", .{base});
    defer gpa.free(rootless);

    var list: std.ArrayList([]const u8) = .empty;
    defer {
        for (list.items) |s| gpa.free(s);
        list.deinit(gpa);
    }
    try addPodmanRuntimeRoot(io, gpa, &list, xdg);
    try addPodmanRuntimeRoot(io, gpa, &list, run);
    try addPodmanRuntimeRoot(io, gpa, &list, rootless);

    try std.testing.expectEqual(@as(usize, 3), list.items.len);
    const first = try std.fmt.allocPrint(gpa, "{s}/podman/work-api.sock", .{xdg});
    defer gpa.free(first);
    const second = try std.fmt.allocPrint(gpa, "{s}/podman/system-api.sock", .{run});
    defer gpa.free(second);
    const third = try std.fmt.allocPrint(gpa, "{s}/podman/rootless-api.sock", .{rootless});
    defer gpa.free(third);
    try std.testing.expectEqualStrings(first, list.items[0]);
    try std.testing.expectEqualStrings(second, list.items[1]);
    try std.testing.expectEqualStrings(third, list.items[2]);
}

test "a podman 5 alias resolves to the named runtime socket" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    try tmp.dir.createDirPath(io, "runtime/podman");
    try tmp.dir.createDirPath(io, "home/.local/share/containers/podman/machine");
    try tmp.dir.writeFile(io, .{ .sub_path = "runtime/podman/work-api.sock", .data = "" });

    const runtime_base = try std.fmt.allocPrint(gpa, "{s}/runtime", .{base});
    defer gpa.free(runtime_base);
    const socket = try std.fmt.allocPrint(gpa, "{s}/podman/work-api.sock", .{runtime_base});
    defer gpa.free(socket);
    const alias = try std.fmt.allocPrint(gpa, "{s}/home/.local/share/containers/podman/machine/podman.sock", .{base});
    defer gpa.free(alias);
    try tmp.dir.symLink(io, socket, "home/.local/share/containers/podman/machine/podman.sock", .{});

    var list: std.ArrayList([]const u8) = .empty;
    defer {
        for (list.items) |s| gpa.free(s);
        list.deinit(gpa);
    }
    try addPodmanRuntimeRoot(io, gpa, &list, runtime_base);
    try addResolved(io, gpa, &list, alias);

    try std.testing.expectEqual(@as(usize, 1), list.items.len);
    try std.testing.expectEqualStrings(socket, list.items[0]);
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
