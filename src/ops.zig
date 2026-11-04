//! Engine lifecycle commands: start, stop and restart for the VM-backed
//! engines berth can find but, until now, could only watch. Commands run as
//! background jobs because `colima start` can take minutes.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const command = @import("engine/command.zig");
const discovery = @import("engine/discovery.zig");
const sheets = @import("sheets.zig");
const session = @import("session.zig");

/// GUI apps inherit a minimal PATH; the container tools live in Homebrew and
/// Podman's own prefix. `env` resolves the tool against this list.
const tool_path = "/opt/homebrew/bin:/usr/local/bin:/opt/podman/bin:/usr/bin:/bin:/usr/sbin:/sbin";

pub const Verb = enum {
    start,
    stop,
    restart,

    pub fn label(self: Verb) []const u8 {
        return switch (self) {
            .start => "Starting",
            .stop => "Stopping",
            .restart => "Restarting",
        };
    }

    pub fn word(self: Verb) []const u8 {
        return switch (self) {
            .start => "start",
            .stop => "stop",
            .restart => "restart",
        };
    }

    pub fn title(self: Verb) []const u8 {
        return switch (self) {
            .start => "Start",
            .stop => "Stop",
            .restart => "Restart",
        };
    }
};

/// A machine command waiting on its confirmation. The socket and name are
/// copied because discovery can replace the runtime slice between the click
/// and the answer, and the action must still target the engine that was named.
pub const Pending = struct {
    verb: Verb,
    kind: discovery.Kind,
    name: [128]u8 = undefined,
    name_len: usize = 0,
    socket: [512]u8 = undefined,
    socket_len: usize = 0,
    /// How many running containers the command takes down with the engine,
    /// read when the question was asked rather than when it is answered.
    /// Null when neither the live list nor `/info` has answered yet: the
    /// sentence must not claim zero.
    running: ?usize = null,

    pub fn nameText(self: *const Pending) []const u8 {
        return self.name[0..self.name_len];
    }

    pub fn socketText(self: *const Pending) []const u8 {
        return self.socket[0..self.socket_len];
    }
};

/// The sentence under the engine confirmation. Formatted into the caller's
/// buffer so the unknown-count case is testable without a frame.
pub fn consequence(p: *const Pending, buf: []u8) []const u8 {
    if (p.running) |n| {
        if (n == 1) return std.fmt.bufPrint(buf, "1 running container on {s} will stop with the engine.", .{p.nameText()}) catch "A running container will stop with the engine.";
        return std.fmt.bufPrint(buf, "{d} running containers on {s} will stop with the engine.", .{ n, p.nameText() }) catch "Running containers will stop with the engine.";
    }
    return std.fmt.bufPrint(buf, "Running containers on {s} will stop with the engine.", .{p.nameText()}) catch "Running containers will stop with the engine.";
}

pub const State = struct {
    job: ?*command.Job = null,
    pending: ?Pending = null,
    label_buf: [96]u8 = undefined,
    label_len: usize = 0,
    /// The engine this job was started for, captured before a minutes-long
    /// stop lets discovery or a click select a different one.
    target_kind: discovery.Kind = .dockerd,
    /// The original socket, not the derived instance name: the restart must
    /// re-derive the same name instead of treating it as a path.
    target_socket: [512]u8 = undefined,
    target_socket_len: usize = 0,
    /// Lima and Podman have no `restart` verb; their restart is a stop job
    /// followed by a start job.
    restart_after_stop: bool = false,

    pub fn label(self: *const State) []const u8 {
        return self.label_buf[0..self.label_len];
    }

    pub fn running(self: *const State) bool {
        return self.job != null;
    }

    pub fn dismissPending(self: *State) void {
        self.pending = null;
    }
};

pub fn supported(kind: discovery.Kind) bool {
    return kind == .lima or kind == .colima or kind == .podman_machine;
}

/// Asks before stopping or restarting a machine: both take every running
/// container down with them. Start is safe and still runs immediately.
pub fn confirm(s: *app.State, verb: Verb, rt: *const discovery.Runtime, running: ?usize) void {
    if (verb == .start or rt.socket.len > 512) return;
    sheets.dismiss(s);
    var p: Pending = .{ .verb = verb, .kind = rt.kind, .running = running };
    p.name_len = @min(rt.name.len, p.name.len);
    @memcpy(p.name[0..p.name_len], rt.name[0..p.name_len]);
    p.socket_len = rt.socket.len;
    @memcpy(p.socket[0..p.socket_len], rt.socket);
    s.ops.pending = p;
    s.redraw();
}

pub fn resolve(s: *app.State, ok: bool) void {
    const pending = s.ops.pending orelse return;
    if (!ok) {
        s.ops.pending = null;
        s.redraw();
        return;
    }
    // Keep the question until the command actually started. A refusal (busy,
    // spawn failure) can be retried; discarding it would hide the reason
    // behind a toast and leave the user to click Stop again from scratch.
    if (start(s, pending.verb, pending.kind, pending.socketText())) s.ops.pending = null;
    s.redraw();
}

pub fn start(s: *app.State, verb: Verb, kind: discovery.Kind, socket: []const u8) bool {
    if (s.mutating()) {
        s.notify("wait for the current engine operation to finish", true);
        return false;
    }

    var target_buf: [96]u8 = undefined;
    const target = instanceFor(s.io, &target_buf, kind, socket);
    const planned = plan(verb, kind);
    var argv: [6][]const u8 = undefined;
    var argc: usize = 0;
    argv[argc] = "/usr/bin/env";
    argc += 1;
    argv[argc] = "PATH=" ++ tool_path;
    argc += 1;
    argv[argc] = switch (kind) {
        .lima => "limactl",
        .colima => "colima",
        else => "podman",
    };
    argc += 1;
    argv[argc] = if (kind == .podman_machine) "machine" else planned.verb.word();
    argc += 1;
    if (kind == .podman_machine) {
        argv[argc] = planned.verb.word();
        argc += 1;
    }
    if (target.len > 0 and argc < argv.len) {
        argv[argc] = target;
        argc += 1;
    }

    const described = describe(&s.ops.label_buf, planned.verb, kind, target);
    s.ops.label_len = described.len;
    rememberTarget(s, kind, socket);
    const job = command.start(
        s.gpa,
        s.io,
        argv[0..argc],
        described,
        300,
        s,
        finish,
    ) catch {
        s.notify("could not start the engine command", true);
        return false;
    };
    s.ops.job = job;
    s.ops.restart_after_stop = planned.restart_after_stop;
    s.redraw();
    return true;
}

const Plan = struct {
    verb: Verb,
    restart_after_stop: bool = false,
};

fn plan(verb: Verb, kind: discovery.Kind) Plan {
    if (verb != .restart) return .{ .verb = verb };
    if (kind == .colima) return .{ .verb = .restart };
    // Lima and Podman machines have no restart subcommand.
    return .{ .verb = .stop, .restart_after_stop = true };
}

fn finish(ctx: ?*anyopaque) callconv(.c) void {
    const s: *app.State = @ptrCast(@alignCast(ctx.?));
    const job = s.ops.job orelse return;
    command.settle(job);
    const ok = job.ok;
    const output = s.gpa.dupe(u8, job.output) catch "";
    const label = s.gpa.dupe(u8, s.ops.label()) catch "";
    const restart_after_stop = s.ops.restart_after_stop;
    command.destroy(job);
    s.ops.job = null;
    s.ops.restart_after_stop = false;

    if (ok) {
        s.notifyFmt("{s}: done", .{label}, false);
    } else {
        s.notifyFmt("{s}: {s}", .{ label, if (output.len > 0) firstLine(output) else "failed" }, true);
    }
    if (output.len > 0) s.gpa.free(output);
    if (label.len > 0) s.gpa.free(label);

    if (restart_after_stop and ok) {
        _ = start(s, .start, s.ops.target_kind, restartSocket(s));
        // If the second half could not spawn, still rescan and release any
        // switch or scan queued behind the restart.
        if (s.ops.job == null) {
            s.discover_wanted = true;
            session.@"resume"(s);
        }
        return;
    }
    // An engine command always invalidates the scan; queue it through the
    // same resume path so a pending switch or refresh is not overtaken.
    s.discover_wanted = true;
    session.@"resume"(s);
}

fn firstLine(text: []const u8) []const u8 {
    const end = std.mem.indexOfScalar(u8, text, '\n') orelse @min(text.len, 90);
    return text[0..@min(end, 90)];
}

pub fn cancel(s: *app.State) void {
    if (s.ops.job) |job| {
        command.discard(job);
        s.ops.job = null;
    }
}

/// The original engine address is what survives a stop/start cycle. Keeping
/// the derived instance name here made the second start look for a socket
/// named after the machine and fall back to the default machine.
fn rememberTarget(s: *app.State, kind: discovery.Kind, socket: []const u8) void {
    s.ops.target_kind = kind;
    const n = @min(socket.len, s.ops.target_socket.len);
    // A restart passes this same buffer back in; copyForwards tolerates that
    // self-copy, where @memcpy would require disjoint regions.
    std.mem.copyForwards(u8, s.ops.target_socket[0..n], socket[0..n]);
    s.ops.target_socket_len = n;
}

fn restartSocket(s: *const app.State) []const u8 {
    return s.ops.target_socket[0..s.ops.target_socket_len];
}

/// The instance/profile a socket belongs to, as the tool names it.
fn instanceFor(io: std.Io, buf: []u8, kind: discovery.Kind, socket: []const u8) []const u8 {
    const name = instance(buf, kind, socket);
    if (name.len > 0 or kind != .podman_machine) return name;
    // Podman 4 has no name in the socket path. Its machine config is the only
    // place that knows a non-default name, and only when it is unambiguous.
    return discovery.podman4MachineName(io, socket, buf) orelse "";
}

/// The name encoded in the socket path alone; Podman 4 needs a config lookup
/// on top, which `instanceFor` supplies.
fn instance(buf: []u8, kind: discovery.Kind, socket: []const u8) []const u8 {
    _ = buf;
    // Podman 5 encodes the name in `<name>-api.sock`. Podman 4's socket sits
    // in a VM-type directory (`applehv`, `libkrun`), which is not a machine
    // name: the default machine is addressed without one.
    if (kind == .podman_machine) return discovery.podmanMachineName(socket) orelse "";
    const marker: []const u8 = switch (kind) {
        .lima => "/.lima/",
        .colima => "/.colima/",
        else => return "",
    };
    const at = std.mem.indexOf(u8, socket, marker) orelse return "";
    var rest = socket[at + marker.len ..];
    const end = std.mem.indexOfScalar(u8, rest, '/') orelse {
        // `~/.colima/docker.sock` is the default profile, not a profile named
        // "docker.sock"; starting that would create a second VM.
        if (kind == .colima and std.mem.eql(u8, rest, "docker.sock")) return "";
        return rest;
    };
    rest = rest[0..end];
    if (std.mem.eql(u8, rest, "default")) return "";
    return rest;
}

fn describe(buf: []u8, verb: Verb, kind: discovery.Kind, target: []const u8) []const u8 {
    const name = if (target.len == 0) switch (kind) {
        .lima => "Lima default",
        .colima => "Colima default",
        else => "Podman machine",
    } else target;
    const text = std.fmt.bufPrint(buf, "{s} {s}", .{ verb.label(), name }) catch "Engine command";
    return text;
}

test "the socket names the instance a tool would use" {
    var buf: [96]u8 = undefined;
    try std.testing.expectEqualStrings("docker", instance(&buf, .lima, "/Users/x/.lima/docker/sock/docker.sock"));
    try std.testing.expectEqualStrings("", instance(&buf, .lima, "/Users/x/.lima/default/sock/docker.sock"));
    try std.testing.expectEqualStrings("work", instance(&buf, .colima, "/Users/x/.colima/work/docker.sock"));
    try std.testing.expectEqualStrings("", instance(&buf, .podman_machine, "/Users/x/.local/share/containers/podman/machine/libkrun/podman.sock"));
}

test "podman 5 machines use their filename and 4.x uses the default" {
    var buf: [96]u8 = undefined;
    try std.testing.expectEqualStrings(
        "podman-machine-default",
        instance(&buf, .podman_machine, "/var/folders/x/T/podman/podman-machine-default-api.sock"),
    );
    try std.testing.expectEqualStrings(
        "work",
        instance(&buf, .podman_machine, "/var/folders/x/T/podman/work-api.sock"),
    );
    // The 4.x directory is the VM type, never the machine name, and the root
    // socket belongs to the default machine.
    try std.testing.expectEqualStrings("", instance(&buf, .podman_machine, "/Users/x/.local/share/containers/podman/machine/applehv/podman.sock"));
    try std.testing.expectEqualStrings("", instance(&buf, .podman_machine, "/Users/x/.local/share/containers/podman/machine/podman.sock"));
}

test "a named machine survives the stop then start round trip" {
    var s = app.State{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer s.arena.deinit();

    const podman_socket = "/var/folders/x/T/podman/work-api.sock";
    rememberTarget(&s, .podman_machine, podman_socket);
    var buf: [96]u8 = undefined;
    try std.testing.expectEqualStrings("work", instanceFor(std.testing.io, &buf, s.ops.target_kind, restartSocket(&s)));
    // The restart stores the same buffer it just read, which must not be an
    // overlapping @memcpy.
    rememberTarget(&s, .podman_machine, restartSocket(&s));
    try std.testing.expectEqualStrings("work", instanceFor(std.testing.io, &buf, s.ops.target_kind, restartSocket(&s)));

    // Lima's socket has always carried the instance; the restart must keep it.
    rememberTarget(&s, .lima, "/Users/x/.lima/work/sock/docker.sock");
    try std.testing.expectEqualStrings("work", instanceFor(std.testing.io, &buf, s.ops.target_kind, restartSocket(&s)));
}

test "restart uses one command where the tool has it" {
    const colima = plan(.restart, .colima);
    try std.testing.expectEqual(Verb.restart, colima.verb);
    try std.testing.expect(!colima.restart_after_stop);

    const lima = plan(.restart, .lima);
    try std.testing.expectEqual(Verb.stop, lima.verb);
    try std.testing.expect(lima.restart_after_stop);
}

test "an unknown engine count is worded without a number" {
    var p: Pending = .{ .verb = .stop, .kind = .lima };
    p.name_len = 4;
    @memcpy(p.name[0..4], "lima");
    var buf: [192]u8 = undefined;

    try std.testing.expectEqualStrings("Running containers on lima will stop with the engine.", consequence(&p, &buf));
    p.running = 1;
    try std.testing.expectEqualStrings("1 running container on lima will stop with the engine.", consequence(&p, &buf));
    p.running = 3;
    try std.testing.expectEqualStrings("3 running containers on lima will stop with the engine.", consequence(&p, &buf));
}

test "a refused engine command keeps its confirmation" {
    var state = app.State{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    var fake: command.Job = undefined;
    state.ops.job = &fake;
    state.ops.pending = .{ .verb = .stop, .kind = .lima };
    resolve(&state, true);
    // Still busy: the question stays so the retry is one click away.
    try std.testing.expect(state.ops.pending != null);
    state.ops.job = null;
}
