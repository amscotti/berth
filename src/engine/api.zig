//! The Docker Engine API as berth uses it — the same surface Podman, Colima
//! and Lima expose, because they all implement Docker's compatibility layer.
//!
//! Types carry defaults for every field so a newer or older engine that adds
//! or omits one does not fail to parse; `ignore_unknown_fields` is always on
//! for the same reason.
const std = @import("std");
const http = @import("http_uds.zig");

const stats_mod = @import("stats.zig");

pub const Stats = stats_mod.Stats;
pub const Logs = stats_mod.Logs;
pub const demux = stats_mod.demux;

pub const Error = http.Error || std.mem.Allocator.Error || std.json.ParseError(std.json.Scanner) || error{
    EngineError,
};

/// A request that failed with an HTTP status, carrying the engine's message.
pub const EngineError = struct {
    status: u16,
    message: []u8,

    pub fn deinit(self: EngineError, gpa: std.mem.Allocator) void {
        gpa.free(self.message);
    }
};

pub const Client = struct {
    io: std.Io,
    gpa: std.mem.Allocator,
    /// `unix:///var/run/docker.sock` or a bare path.
    socket: []const u8,
    /// Filled from `/version`; requests are prefixed with `/v<api>`.
    api: []const u8 = "v1.43",
    /// Where parsed bodies live. The owner resets this arena to drop a whole
    /// snapshot at once, which is why nothing here frees individually.
    alloc: std.mem.Allocator,
    /// The engine's own words when a call failed, for the UI to show.
    err_body: []const u8 = "",

    pub fn init(io: std.Io, gpa: std.mem.Allocator, socket: []const u8) Client {
        return .{ .io = io, .gpa = gpa, .socket = socket, .alloc = gpa };
    }

    fn path(self: *Client, gpa: std.mem.Allocator, p: []const u8) ![]u8 {
        return std.fmt.allocPrint(gpa, "/{s}{s}", .{ self.api, p });
    }

    /// One request, one response, connection closed; body owned. `p` gets the API prefix unless `raw`.
    pub fn callOpts(self: *Client, method: []const u8, p: []const u8, body: []const u8, max: usize, raw: bool) ![]u8 {
        return self.callOptsTimeout(method, p, body, max, raw, http.Conn.io_timeout_sec, false);
    }

    pub fn callOptsTimeout(
        self: *Client,
        method: []const u8,
        p: []const u8,
        body: []const u8,
        max: usize,
        raw: bool,
        seconds: isize,
        allow_not_modified: bool, // lifecycle calls: 304 means "already there"
    ) ![]u8 {
        var conn = try http.Conn.openTimeout(self.io, self.socket, seconds);
        defer conn.close();
        var full: []u8 = undefined;
        if (raw) {
            full = try self.gpa.dupe(u8, p);
        } else {
            full = try self.path(self.gpa, p);
        }
        defer self.gpa.free(full);
        const res = try conn.request(self.gpa, method, full, body, max);
        if (!res.okFor(method, allow_not_modified)) {
            // Keep what the engine said: it is usually the whole diagnosis
            // ("client version 1.43 is too old"). Engines wrap it as
            // {"message":"…"}; unwrap when they do so the UI shows words, not
            // the envelope.
            const Message = struct { message: []const u8 = "" };
            if (std.json.parseFromSliceLeaky(Message, self.alloc, res.body, .{
                .ignore_unknown_fields = true,
                // Without this the parsed message can point into `res.body`,
                // which is freed on the next line and takes the diagnosis with it.
                .allocate = .alloc_always,
            })) |m| {
                self.err_body = if (m.message.len > 0) m.message else (self.alloc.dupe(u8, res.body) catch "");
            } else |_| {
                self.err_body = self.alloc.dupe(u8, res.body) catch "";
            }
            res.deinit(self.gpa);
            return error.EngineError;
        }
        self.err_body = "";
        return res.body;
    }

    pub fn call(self: *Client, method: []const u8, p: []const u8, body: []const u8, max: usize) ![]u8 {
        return self.callOpts(method, p, body, max, false);
    }

    /// Parses a JSON answer into `T`, then frees the bytes.
    pub fn getOpts(self: *Client, comptime T: type, p: []const u8, raw: bool) !T {
        return self.getOptsTimeout(T, p, raw, http.Conn.io_timeout_sec);
    }

    fn getOptsTimeout(self: *Client, comptime T: type, p: []const u8, raw: bool, seconds: isize) !T {
        const body = try self.callOptsTimeout("GET", p, "", 8 * 1024 * 1024, raw, seconds, false);
        defer self.gpa.free(body);
        const parsed = std.json.parseFromSliceLeaky(T, self.alloc, body, .{
            .ignore_unknown_fields = true,
            .allocate = .alloc_always,
        }) catch |e| {
            // Name the failure: a parse error and an HTTP error are very
            // different problems and both used to read as "could not".
            self.err_body = self.alloc.dupe(u8, @errorName(e)) catch "";
            return error.EngineError;
        };
        return parsed;
    }

    pub fn get(self: *Client, comptime T: type, p: []const u8) !T {
        return self.getOpts(T, p, false);
    }

    /// `/version` is deliberately un-prefixed: it is the call that tells us
    /// which prefix this engine will accept, so it cannot need one.
    pub fn version(self: *Client) !Version {
        return self.getOpts(Version, "/version", true);
    }

    /// Uses the engine's own API version where it is a sane one, so a Docker 29
    /// engine is not asked for 1.43 routes it has dropped. The engine reports
    /// `1.52`; the route is `/v1.52/...`, so the `v` is ours to add.
    pub fn adoptApi(self: *Client, reported: []const u8) void {
        if (reported.len == 0 or reported[0] < '0' or reported[0] > '9') return;
        self.api = std.fmt.allocPrint(self.alloc, "v{s}", .{reported}) catch self.api;
    }

    pub fn info(self: *Client) !Info {
        return self.get(Info, "/info");
    }

    pub fn containers(self: *Client) ![]Container {
        return self.get([]Container, "/containers/json?all=1");
    }

    pub fn images(self: *Client) ![]Image {
        return self.get([]Image, "/images/json?all=0");
    }

    pub fn volumes(self: *Client) !ListResponse(Volume) {
        return self.get(ListResponse(Volume), "/volumes");
    }

    pub fn networks(self: *Client) ![]Network {
        return self.get([]Network, "/networks");
    }

    /// The events of an interval, one JSON object per line. `until` is what
    /// keeps this from turning into the streaming endpoint.
    pub fn events(self: *Client, since: i64, until: i64) ![]const u8 {
        const p = try std.fmt.allocPrint(self.gpa, "/events?since={d}&until={d}", .{ since, until });
        defer self.gpa.free(p);
        // Short: a VM whose clock trails the host can hold this open until it
        // catches up, and that must cost one pass, not the whole chain.
        return self.callOptsTimeout("GET", p, "", 4 << 20, false, 3, false);
    }

    pub fn df(self: *Client) !Df {
        // Shorter than the default: df walks every store, and on a stalled
        // forwarder it is the call that used to freeze the window. Missing
        // disk figures for one refresh is better than a hung monitor.
        return self.getOptsTimeout(Df, "/system/df", false, 3);
    }

    /// One stats sample, with `precpu` populated: `stream=false` alone waits
    /// two collection cycles (~2 s) and that wait is what the number needs.
    /// `one-shot=true` would be faster and wrong.
    pub fn stats(self: *Client, id: []const u8) !Stats {
        const p = try std.fmt.allocPrint(self.gpa, "/containers/{s}/stats?stream=false", .{id});
        defer self.gpa.free(p);
        return self.get(Stats, p);
    }

    /// The last `tail` lines. Logs go through the arena like everything else,
    /// so the view can hold them for the frame without copying.
    pub fn logs(self: *Client, id: []const u8, tail: usize, timestamps: bool) ![]const u8 {
        const p = try std.fmt.allocPrint(self.gpa, "/containers/{s}/logs?stdout=1&stderr=1&timestamps={d}&tail={d}", .{ id, @as(u8, if (timestamps) 1 else 0), tail });
        defer self.gpa.free(p);
        const body = try self.call("GET", p, "", 4 * 1024 * 1024);
        defer self.gpa.free(body);
        return demux(self.alloc, body, 4 * 1024 * 1024) catch "";
    }

    pub fn inspect(self: *Client, id: []const u8) !ContainerDetail {
        const p = try std.fmt.allocPrint(self.gpa, "/containers/{s}/json", .{id});
        defer self.gpa.free(p);
        return self.get(ContainerDetail, p);
    }

    /// Lifecycle and maintenance; only this call treats 304 as "already there".
    pub fn action(self: *Client, method: []const u8, p: []const u8) !void {
        const body = self.callOptsTimeout(method, p, "", 1 << 20, false, http.Conn.io_timeout_sec, true) catch |e| return e;
        self.gpa.free(body);
    }
};

pub fn ListResponse(comptime T: type) type {
    return struct {
        Volumes: []T = &.{},
        Warnings: ?[]const []const u8 = null,
    };
}

pub const Version = struct {
    Version: []const u8 = "",
    ApiVersion: []const u8 = "",
    MinAPIVersion: []const u8 = "",
    Os: []const u8 = "",
    Arch: []const u8 = "",
    GoVersion: []const u8 = "",
};

pub const Info = struct {
    ID: []const u8 = "",
    Name: []const u8 = "",
    ServerVersion: []const u8 = "",
    OperatingSystem: []const u8 = "",
    OSType: []const u8 = "",
    Architecture: []const u8 = "",
    NCPU: u32 = 0,
    MemTotal: u64 = 0,
    DockerRootDir: []const u8 = "",
    Driver: []const u8 = "",
    LoggingDriver: []const u8 = "",
    CgroupVersion: []const u8 = "",
    Containers: u32 = 0,
    ContainersRunning: u32 = 0,
    ContainersPaused: u32 = 0,
    ContainersStopped: u32 = 0,
    Images: u32 = 0,
};

pub const Port = struct {
    IP: ?[]const u8 = null,
    PrivatePort: u16 = 0,
    PublicPort: ?u16 = null,
    Type: []const u8 = "",
};

/// A host-side binding a browser can be pointed at.
pub const HostBinding = struct {
    port: u16 = 0,
    /// The port inside the container, for the `8080→80` spelling in the
    /// inspector. Kept from the first binding that carried the host port.
    private_port: u16 = 0,
    /// The address the port is bound on, when Docker reports one that is not
    /// the wildcard; empty means "any", which is `localhost` to a browser.
    ip: []const u8 = "",
};

fn bindingLessThan(_: void, a: HostBinding, b: HostBinding) bool {
    return a.port < b.port;
}

pub const Container = struct {
    Id: []const u8 = "",
    Names: []const []const u8 = &.{},
    Image: []const u8 = "",
    ImageID: []const u8 = "",
    Command: []const u8 = "",
    Created: i64 = 0,
    State: []const u8 = "",
    Status: []const u8 = "",
    Ports: []const Port = &.{},
    Labels: ?std.json.Value = null,
    NetworkSettings: ?std.json.Value = null,
    Mounts: ?[]const Mount = null,

    /// The first name without Docker's leading slash.
    pub fn name(self: Container) []const u8 {
        if (self.Names.len == 0) return self.Id;
        const n = self.Names[0];
        return if (n.len > 0 and n[0] == '/') n[1..] else n;
    }

    /// A Compose label, or null when the container is not part of a project.
    pub fn label(self: Container, key: []const u8) ?[]const u8 {
        const labels = self.Labels orelse return null;
        if (labels != .object) return null;
        const v = labels.object.get(key) orelse return null;
        return switch (v) {
            .string => |s| s,
            else => null,
        };
    }

    pub fn project(self: Container) ?[]const u8 {
        return self.label("com.docker.compose.project");
    }

    pub fn service(self: Container) ?[]const u8 {
        return self.label("com.docker.compose.service");
    }

    pub fn isRunning(self: Container) bool {
        return std.mem.eql(u8, self.State, "running");
    }

    pub fn isPaused(self: Container) bool {
        return std.mem.eql(u8, self.State, "paused");
    }

    /// "Up" in the `docker ps` sense: running or paused. A paused container
    /// still holds memory, still has real (frozen) stats, and still counts in
    /// the running totals; classifying it as stopped hides all of that.
    pub fn isUp(self: Container) bool {
        return self.isRunning() or self.isPaused() or std.mem.eql(u8, self.State, "restarting");
    }

    /// The healthcheck verdict, when the container has one. It rides at the end
    /// of the human `Status` string; `State` never carries it. Only these three
    /// literal suffixes count — the parentheses in "Exited (0) 2 seconds ago"
    /// and "Restarting (1) …" are exit codes, not health.
    pub fn health(self: Container) ?[]const u8 {
        if (std.mem.endsWith(u8, self.Status, "(healthy)")) return "healthy";
        if (std.mem.endsWith(u8, self.Status, "(unhealthy)")) return "unhealthy";
        if (std.mem.endsWith(u8, self.Status, "(health: starting)")) return "starting";
        return null;
    }

    /// The host-side TCP ports a container publishes, deduplicated by public
    /// port (Docker lists one binding per address family). Returns the total,
    /// which can exceed `out.len`: the buffer fills, the count stays honest,
    /// and the caller can still say "+k more". UDP and SCTP bindings are not
    /// browseable and are left out.
    pub fn publishedPorts(self: Container, out: []HostBinding) usize {
        var n: usize = 0;
        var extra: usize = 0;
        var last_extra: ?u16 = null;
        for (self.Ports) |p| {
            const public = p.PublicPort orelse continue;
            if (std.mem.eql(u8, p.Type, "udp") or std.mem.eql(u8, p.Type, "sctp")) continue;
            var seen = false;
            for (out[0..n]) |q| {
                if (q.port == public) {
                    seen = true;
                    break;
                }
            }
            if (seen) continue;
            if (n == out.len) {
                // Past the buffer: count, deduplicating the neighbouring
                // address-family twin the way the in-buffer scan does.
                if (last_extra == public) continue;
                last_extra = public;
                extra += 1;
                continue;
            }
            const ip = p.IP orelse "";
            out[n] = .{
                .port = public,
                .private_port = p.PrivatePort,
                .ip = if (std.mem.eql(u8, ip, "0.0.0.0") or std.mem.eql(u8, ip, "::")) "" else ip,
            };
            n += 1;
        }
        // The engine's port list is not ordered; the table and the inspector
        // must show the same first pills.
        std.mem.sort(HostBinding, out[0..n], {}, bindingLessThan);
        return n + extra;
    }

    /// Published ports as `8080→80/tcp`; at most `max` then `+n`.
    pub fn portsText(self: Container, buf: []u8, max: usize) []const u8 {
        var fbs = std.Io.Writer.fixed(buf);
        const w = &fbs;
        var shown: usize = 0;
        for (self.Ports) |p| {
            if (p.PublicPort == null) continue;
            if (shown == max) {
                w.print(" +{d}", .{self.Ports.len - shown}) catch {};
                break;
            }
            if (shown > 0) w.writeAll(", ") catch {};
            w.print("{d}→{d}", .{ p.PublicPort.?, p.PrivatePort }) catch {};
            shown += 1;
        }
        return fbs.buffered();
    }
};

pub const Mount = struct {
    Type: []const u8 = "",
    Name: ?[]const u8 = null,
    Source: []const u8 = "",
    Destination: []const u8 = "",
    Mode: []const u8 = "",
    RW: bool = true,
};

pub const ContainerDetail = struct {
    Id: []const u8 = "",
    Name: []const u8 = "",
    Created: []const u8 = "",
    Path: []const u8 = "",
    Args: []const []const u8 = &.{},
    State: DetailState = .{},
    Config: DetailConfig = .{},
    HostConfig: ?std.json.Value = null,
    Mounts: ?[]const Mount = null,
    NetworkSettings: ?std.json.Value = null,
};

pub const DetailState = struct {
    Status: []const u8 = "",
    Running: bool = false,
    Paused: bool = false,
    Restarting: bool = false,
    OOMKilled: bool = false,
    Dead: bool = false,
    Pid: u32 = 0,
    ExitCode: i32 = 0,
    Error: []const u8 = "",
    StartedAt: []const u8 = "",
    FinishedAt: []const u8 = "",
    Health: ?Health = null,
};

pub const Health = struct {
    Status: []const u8 = "",
    FailingStreak: i32 = 0,
    Log: ?std.json.Value = null,
};

pub const DetailConfig = struct {
    Hostname: []const u8 = "",
    Image: []const u8 = "",
    Tty: bool = false,
    Env: ?[]const []const u8 = null,
    Cmd: ?[]const []const u8 = null,
    Entrypoint: ?[]const []const u8 = null,
    Labels: ?std.json.Value = null,
};

pub const Image = struct {
    Id: []const u8 = "",
    ParentId: []const u8 = "",
    RepoTags: ?[]const []const u8 = null,
    RepoDigests: ?[]const []const u8 = null,
    Created: i64 = 0,
    Size: u64 = 0,
    SharedSize: i64 = -1,
    Labels: ?std.json.Value = null,
    Containers: i64 = -1,

    pub fn tag(self: Image) []const u8 {
        const tags = self.RepoTags orelse return "<none>:<none>";
        if (tags.len == 0) return "<none>:<none>";
        return tags[0];
    }

    pub fn dangling(self: Image) bool {
        const tags = self.RepoTags orelse return true;
        return tags.len == 0 or std.mem.startsWith(u8, tags[0], "<none>");
    }

    /// What a table row shows: the first tag, or "untagged".
    pub fn label(self: Image) []const u8 {
        return if (self.dangling()) "untagged" else self.tag();
    }
};

pub const Volume = struct {
    Name: []const u8 = "",
    Driver: []const u8 = "",
    Mountpoint: []const u8 = "",
    CreatedAt: ?[]const u8 = null,
    Labels: ?std.json.Value = null,
    Scope: []const u8 = "",
    UsageData: ?UsageData = null,
};

pub const UsageData = struct {
    Size: i64 = -1,
    RefCount: i64 = 0,
};

pub const Network = struct {
    Name: []const u8 = "",
    Id: []const u8 = "",
    Created: []const u8 = "",
    Scope: []const u8 = "",
    Driver: []const u8 = "",
    EnableIPv6: bool = false,
    Internal: bool = false,
    Attachable: bool = false,
    Containers: ?std.json.Value = null,
    Labels: ?std.json.Value = null,

    pub fn builtin(self: Network) bool {
        return std.mem.eql(u8, self.Name, "bridge") or
            std.mem.eql(u8, self.Name, "host") or
            std.mem.eql(u8, self.Name, "none");
    }

    /// How many containers are attached.
    pub fn attached(self: Network) usize {
        const c = self.Containers orelse return 0;
        if (c != .object) return 0;
        return c.object.count();
    }
};

/// How many containers are attached to `network`. The `/networks` list leaves
/// `Containers` empty on current engines, so count from each container's
/// `NetworkSettings.Networks` instead of trusting the list's field.
pub fn attachedCount(containers: []const Container, network: []const u8) usize {
    var n: usize = 0;
    for (containers) |ct| {
        const settings = ct.NetworkSettings orelse continue;
        if (settings != .object) continue;
        const nets = settings.object.get("Networks") orelse continue;
        if (nets != .object) continue;
        if (nets.object.get(network) != null) n += 1;
    }
    return n;
}

/// `/system/df` in both shapes: `*Usage` from API 1.52, the flat fields before.
pub const Df = struct {
    LayersSize: u64 = 0,
    Images: ?[]Image = null,
    Containers: ?[]Container = null,
    Volumes: ?[]Volume = null,
    BuildCache: ?[]BuildCacheItem = null,
    ImageUsage: ?Usage = null,
    ContainerUsage: ?Usage = null,
    VolumeUsage: ?Usage = null,
    BuildCacheUsage: ?Usage = null,

    pub const Usage = struct {
        TotalCount: i64 = 0,
        ActiveCount: i64 = 0,
        TotalSize: i64 = 0,
        Reclaimable: i64 = 0,
    };

    pub const BuildCacheItem = struct {
        ID: []const u8 = "",
        Size: i64 = 0,
        InUse: bool = false,
        Shared: bool = false,
    };

    /// The four figures the Disk view shows, whichever shape arrived.
    pub fn imageBytes(self: Df) i64 {
        if (self.ImageUsage) |u| return u.TotalSize;
        var sum: i64 = 0;
        if (self.Images) |list| {
            for (list) |i| sum += @intCast(i.Size);
        }
        return sum;
    }

    /// Everything the engine's stores hold, not only images: the figure the
    /// overview's DISK card and its graph stand for.
    pub fn totalBytes(self: Df) i64 {
        var sum = self.imageBytes();
        if (self.ContainerUsage) |u| sum += u.TotalSize;
        if (self.VolumeUsage) |u| {
            sum += u.TotalSize;
        } else if (self.Volumes) |list| {
            for (list) |v| {
                if (v.UsageData) |d| {
                    // -1 means "not measured"; adding it would shrink the total.
                    if (d.Size > 0) sum += d.Size;
                }
            }
        }
        if (self.BuildCacheUsage) |u| {
            sum += u.TotalSize;
        } else if (self.BuildCache) |list| {
            for (list) |b| sum += b.Size;
        }
        return sum;
    }

    pub fn reclaimable(self: Df) i64 {
        var sum: i64 = 0;
        if (self.ImageUsage) |u| sum += u.Reclaimable;
        if (self.ContainerUsage) |u| sum += u.Reclaimable;
        if (self.VolumeUsage) |u| sum += u.Reclaimable;
        if (self.BuildCacheUsage) |u| sum += u.Reclaimable;
        return sum;
    }
};
