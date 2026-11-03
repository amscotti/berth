//! Opening a container's shell in the user's terminal. berth talks to the
//! engine's socket directly, so the command names that same socket: what opens
//! is attached to the engine the window shows, not to whatever context the
//! user last selected in a shell.
const std = @import("std");
const builtin = @import("builtin");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const discovery = @import("engine/discovery.zig");

const Cli = struct { exe: []const u8, flag: []const u8 };

/// Which CLI speaks to this engine, and how it names a socket. Podman's client
/// uses --url; the Docker family uses -H.
pub fn cliFor(kind: discovery.Kind) Cli {
    return switch (kind) {
        .podman, .podman_machine => .{ .exe = "podman", .flag = "--url" },
        else => .{ .exe = "docker", .flag = "-H" },
    };
}

/// `docker -H 'unix://…' exec -it 'web' sh`. `sh` is the safe common shell:
/// alpine-based images have it, and images without any shell fail visibly in
/// the terminal rather than silently here.
pub fn execCommand(alloc: std.mem.Allocator, kind: discovery.Kind, socket: []const u8, name: []const u8) ![]u8 {
    const cli = cliFor(kind);
    const sock = try std.fmt.allocPrint(alloc, "unix://{s}", .{socket});
    defer alloc.free(sock);
    const quoted_sock = try quote(alloc, sock);
    defer alloc.free(quoted_sock);
    const quoted_name = try quote(alloc, name);
    defer alloc.free(quoted_name);
    return std.fmt.allocPrint(alloc, "{s} {s} {s} exec -it {s} sh", .{ cli.exe, cli.flag, quoted_sock, quoted_name });
}

/// Single-quote one word for sh. Container names come from the engine and
/// socket paths can contain spaces; this string is spliced into a shell.
pub fn quote(alloc: std.mem.Allocator, text: []const u8) ![]u8 {
    var len: usize = 2;
    for (text) |ch| len += if (ch == '\'') 4 else 1;
    const out = try alloc.alloc(u8, len);
    var i: usize = 0;
    out[i] = '\'';
    i += 1;
    for (text) |ch| {
        if (ch == '\'') {
            @memcpy(out[i..][0..4], "'\\''");
            i += 4;
        } else {
            out[i] = ch;
            i += 1;
        }
    }
    out[i] = '\'';
    return out;
}

pub fn open(s: *app.State, ct: *const api.Container) void {
    const runtime = s.activeRuntime() orelse {
        s.notify("no engine is selected", true);
        return;
    };
    const socket = if (s.client) |client| client.socket else runtime.socket;
    if (socket.len == 0) {
        s.notify("this engine has no socket to attach to", true);
        return;
    }
    const name = if (ct.name().len > 0) ct.name() else ct.Id;
    const command = execCommand(s.gpa, runtime.kind, socket, name) catch {
        s.notify("could not build the terminal command", true);
        return;
    };
    defer s.gpa.free(command);
    launch(s, command) catch |err| {
        s.notify(if (err == error.TerminalTimeout)
            "Terminal did not answer — check Automation permissions"
        else
            "could not open a terminal", true);
        return;
    };
}

/// The launch errors `open` can word. Explicit, because the platform branches
/// otherwise infer different sets and this switch would fail to compile on
/// Linux, where TerminalTimeout is never produced.
const LaunchError = error{ TerminalTimeout, TerminalFailed };

fn launchError(err: anyerror) LaunchError {
    return if (err == error.Timeout) error.TerminalTimeout else error.TerminalFailed;
}

fn launch(s: *app.State, command: []const u8) LaunchError!void {
    if (builtin.os.tag == .macos) return launchMac(s, command);
    return launchLinux(s, command);
}

/// Terminal.app through AppleScript: `do script` opens a window running the
/// user's login shell, so their own PATH finds the docker/podman CLI. The wait
/// is bounded: the first AppleScript call can raise the Automation permission
/// sheet, and the frame loop must not sit behind an unanswered dialog.
fn launchMac(s: *app.State, command: []const u8) LaunchError!void {
    const script = appleScript(s.gpa, command) catch return error.TerminalFailed;
    defer s.gpa.free(script);
    const result = std.process.run(s.gpa, s.io, .{
        .argv = &.{
            "/usr/bin/osascript",
            "-e",
            script,
            "-e",
            "tell application \"Terminal\" to activate",
        },
        .timeout = .{ .duration = .{ .raw = .fromSeconds(2), .clock = .awake } },
    }) catch |err| return launchError(err);
    defer s.gpa.free(result.stdout);
    defer s.gpa.free(result.stderr);
    if (!result.term.success()) return error.TerminalFailed;
}

/// AppleScript's double-quoted string: backslash and double quote escape
/// themselves, nothing else does.
pub fn appleScript(alloc: std.mem.Allocator, command: []const u8) ![]u8 {
    const prefix = "tell application \"Terminal\" to do script \"";
    const suffix = "\"";
    var escaped: usize = 0;
    for (command) |ch| escaped += if (ch == '\\' or ch == '"') 2 else 1;
    const out = try alloc.alloc(u8, prefix.len + escaped + suffix.len);
    @memcpy(out[0..prefix.len], prefix);
    var i = prefix.len;
    for (command) |ch| {
        if (ch == '\\' or ch == '"') {
            out[i] = '\\';
            i += 1;
        }
        out[i] = ch;
        i += 1;
    }
    @memcpy(out[i..][0..suffix.len], suffix);
    return out;
}

/// Linux has no single terminal; try the common names in order and background
/// the one that exists. Both stdio streams are redirected into the background:
/// `run` reads its pipes to EOF, and an emulator that inherits them would keep
/// the frame loop waiting until the user closed its window.
fn launchLinux(s: *app.State, command: []const u8) LaunchError!void {
    const quoted = quote(s.gpa, command) catch return error.TerminalFailed;
    defer s.gpa.free(quoted);
    const script = std.fmt.allocPrint(s.gpa, "for t in \"$TERMINAL\" x-terminal-emulator gnome-terminal konsole kitty alacritty foot xterm; do " ++
        "command -v \"$t\" >/dev/null 2>&1 || continue; " ++
        "case \"$t\" in gnome-terminal) \"$t\" -- sh -c {s} </dev/null >/dev/null 2>&1 & ;; " ++
        "*) \"$t\" -e sh -c {s} </dev/null >/dev/null 2>&1 & ;; esac; " ++
        "exit 0; done; exit 127", .{ quoted, quoted }) catch return error.TerminalFailed;
    defer s.gpa.free(script);
    const result = std.process.run(s.gpa, s.io, .{ .argv = &.{ "/bin/sh", "-c", script } }) catch |err| return launchError(err);
    defer s.gpa.free(result.stdout);
    defer s.gpa.free(result.stderr);
    if (!result.term.success()) return error.TerminalFailed;
}

test "quotes survive spaces, quotes and dollar signs" {
    const a = std.testing.allocator;
    const plain = try quote(a, "web");
    defer a.free(plain);
    try std.testing.expectEqualStrings("'web'", plain);

    const tricky = try quote(a, "it's a $HOME `test`");
    defer a.free(tricky);
    try std.testing.expectEqualStrings("'it'\\''s a $HOME `test`'", tricky);
}

test "the exec command names the socket and the container" {
    const a = std.testing.allocator;
    const doc = try execCommand(a, .lima, "/Users/x/.lima/docker/sock/docker.sock", "web");
    defer a.free(doc);
    try std.testing.expectEqualStrings(
        "docker -H 'unix:///Users/x/.lima/docker/sock/docker.sock' exec -it 'web' sh",
        doc,
    );

    const pod = try execCommand(a, .podman_machine, "/run/podman.sock", "api");
    defer a.free(pod);
    try std.testing.expectEqualStrings("podman --url 'unix:///run/podman.sock' exec -it 'api' sh", pod);
}

test "AppleScript escapes only the characters it must" {
    const a = std.testing.allocator;
    const script = try appleScript(a, "echo \"hi\" \\ done");
    defer a.free(script);
    try std.testing.expectEqualStrings(
        "tell application \"Terminal\" to do script \"echo \\\"hi\\\" \\\\ done\"",
        script,
    );
}
