//! Engine lifecycle commands: start, stop and restart for the VM-backed
//! engines berth can find but, until now, could only watch. Commands run as
//! background jobs because `colima start` can take minutes.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const command = @import("engine/command.zig");
const discovery = @import("engine/discovery.zig");

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
};

pub const State = struct {
    job: ?*command.Job = null,
    label_buf: [96]u8 = undefined,
    label_len: usize = 0,
    /// The engine this job was started for, captured before a minutes-long
    /// stop lets discovery or a click select a different one.
    target_kind: discovery.Kind = .dockerd,
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
};

pub fn supported(kind: discovery.Kind) bool {
    return kind == .lima or kind == .colima or kind == .podman_machine;
}

pub fn start(s: *app.State, verb: Verb, kind: discovery.Kind, socket: []const u8) void {
    if (s.ops.running()) {
        s.notify("An engine command is already running", true);
        return;
    }

    var target_buf: [96]u8 = undefined;
    const target = instance(&target_buf, kind, socket);
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
    s.ops.target_kind = kind;
    const target_len = @min(target.len, s.ops.target_socket.len);
    @memcpy(s.ops.target_socket[0..target_len], target[0..target_len]);
    s.ops.target_socket_len = target_len;
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
        return;
    };
    s.ops.job = job;
    s.ops.restart_after_stop = planned.restart_after_stop;
    s.redraw();
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
        const socket = s.ops.target_socket[0..s.ops.target_socket_len];
        start(s, .start, s.ops.target_kind, socket);
        return;
    }
    s.discover();
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

/// The instance/profile a socket belongs to, as the tool names it.
fn instance(buf: []u8, kind: discovery.Kind, socket: []const u8) []const u8 {
    _ = buf;
    const marker: []const u8 = switch (kind) {
        .lima => "/.lima/",
        .colima => "/.colima/",
        .podman_machine => "/podman/machine/",
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
    try std.testing.expectEqualStrings("libkrun", instance(&buf, .podman_machine, "/Users/x/.local/share/containers/podman/machine/libkrun/podman.sock"));
}

test "restart uses one command where the tool has it" {
    const colima = plan(.restart, .colima);
    try std.testing.expectEqual(Verb.restart, colima.verb);
    try std.testing.expect(!colima.restart_after_stop);

    const lima = plan(.restart, .lima);
    try std.testing.expectEqual(Verb.stop, lima.verb);
    try std.testing.expect(lima.restart_after_stop);
}
