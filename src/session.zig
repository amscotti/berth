//! The engine session: discovery, choosing a socket, and starting refreshes.
//! Kept out of `app.zig` so the state and its views stay readable, and so the
//! one place that persists a preferred socket is easy to find.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const discovery = @import("engine/discovery.zig");
const refresh_mod = @import("refresh.zig");
const runtimes_mod = @import("runtimes.zig");
const settings = @import("settings.zig");

pub fn selectRuntime(s: *app.State, index: usize) void {
    if (index >= s.runtimes.len) return;
    if (s.ops.running()) {
        s.notify("Wait for the engine command to finish", true);
        return;
    }
    if (s.job != null) {
        // The pass already running belongs to the old engine; switch after it
        // finishes rather than dropping the click or starting a second job.
        s.select_wanted = index;
        return;
    }
    s.active = index;
    s.generation += 1;
    const rt = &s.runtimes[index];
    s.client = api.Client.init(s.io, s.gpa, rt.socket);
    if (rt.api.len > 0) s.client.?.api = rt.api;
    s.clearData();
    rememberEngine(s, rt);
    rememberSocket(s, rt.socket);
    refresh(s);
}

/// Rescans for engines, picks the first that answers, and loads it. The
/// socket last chosen is probed first, so a work Mac with several stale Lima
/// instances does not go back to a different engine on every launch.
pub fn discover(s: *app.State) void {
    if (s.job != null) {
        // A rescan/forget clicked mid-pass runs as soon as that pass ends.
        s.discover_wanted = true;
        return;
    }
    s.discover_wanted = false;
    s.generation += 1;
    const old_id: []const u8 = s.last_engine_id orelse "";
    const old_socket: []const u8 = s.last_engine_socket orelse "";
    // The old client points at a socket owned by `runtimes`; stop using it
    // before those strings are freed.
    s.client = null;
    runtimes_mod.drop(s);
    const found = discovery.scan(s.gpa, s.io, s.preferred_socket orelse "") catch {
        s.error_text = "could not scan for container engines";
        s.sampleHost();
        retryDiscover(s);
        return;
    };
    s.runtimes = found.runtimes;
    // No reachable engine means no active one; leaving 0 selected would mark
    // the first dead socket "active" on the Runtimes page.
    s.active = s.runtimes.len;
    for (s.runtimes, 0..) |r, i| {
        if (r.reachable) {
            s.active = i;
            break;
        }
    }
    if (s.runtimes.len == 0) {
        s.error_text = "no container engine found";
        s.sampleHost();
        retryDiscover(s);
        return;
    }
    if (s.active >= s.runtimes.len) {
        s.error_text = "no container engine is answering";
        s.sampleHost();
        retryDiscover(s);
        return;
    }
    const rt = &s.runtimes[s.active];
    if (!rt.reachable) {
        s.error_text = "no container engine is answering";
        s.sampleHost();
        retryDiscover(s);
        return;
    }
    // Switching engines clears containers, images, stores and selection; a
    // rediscovery of the same engine keeps them. Compare the daemon's ID
    // first: Docker Desktop exposes one engine through several paths.
    const changed = if (old_id.len > 0 and rt.info.ID.len > 0)
        !std.mem.eql(u8, old_id, rt.info.ID)
    else
        old_socket.len > 0 and !std.mem.eql(u8, old_socket, rt.socket);
    if (changed) s.clearData();
    rememberEngine(s, rt);
    s.client = api.Client.init(s.io, s.gpa, rt.socket);
    if (rt.api.len > 0) s.client.?.api = rt.api;
    s.client.?.alloc = s.arena.allocator();
    refresh(s);
}

/// A socket typed by hand. `unix://` is optional; the path is remembered so
/// the next launch probes it before anything else.
pub fn useSocket(s: *app.State, path: []const u8) bool {
    const trimmed = std.mem.trim(u8, path, " \t\r\n");
    const clean = if (std.mem.startsWith(u8, trimmed, "unix://")) trimmed["unix://".len..] else trimmed;
    if (clean.len == 0) {
        s.notify("Enter a socket path", true);
        return false;
    }
    if (s.job != null) {
        s.notify("Wait for the current refresh to finish", true);
        return false;
    }
    if (s.ops.running()) {
        s.notify("Wait for the engine command to finish", true);
        return false;
    }
    // Probe before remembering: a typo must not become the preferred socket
    // that every later launch pays three seconds to reject.
    var probe_client = api.Client.init(s.io, s.gpa, clean);
    if (!discovery.probe(&probe_client)) {
        s.notify("That socket did not answer", true);
        return false;
    }
    rememberSocket(s, clean);
    s.socket_input.set(clean);
    s.engine_menu_open = false;
    s.expanded_kind = null;
    discover(s);
    // `clean` may have been a slice of the text buffer that `set` just moved;
    // compare the remembered copy, not the typed slice.
    const wanted = s.preferred_socket orelse {
        s.notify("Could not remember that socket", true);
        return false;
    };
    for (s.runtimes) |rt| {
        if (!std.mem.eql(u8, rt.socket, wanted)) continue;
        if (!rt.reachable) s.notify("That socket did not answer", true);
        return rt.reachable;
    }
    s.notify("Socket not found", true);
    return false;
}

/// Drop the remembered socket and go back to automatic discovery.
pub fn forgetSocket(s: *app.State) void {
    if (s.ops.running()) {
        s.notify("Wait for the engine command to finish", true);
        return;
    }
    // Forget is the last action: a queued engine choice must not restore the
    // socket the user just dismissed.
    s.select_wanted = null;
    if (s.preferred_socket) |old| s.gpa.free(old);
    s.preferred_socket = null;
    s.socket_input.clear();
    savePrefs(s);
    discover(s);
}

pub fn refresh(s: *app.State) void {
    if (s.client == null) return;
    if (s.job != null) {
        s.refresh_wanted = true;
        return;
    }
    refresh_mod.start(s);
}

/// Two seconds: probes run together and candidates are filtered to sockets
/// that exist, so a Docker Desktop that is still starting is picked up almost
/// at once instead of leaving "can't find the engine" on screen. One timer is
/// enough; a second would make each failure schedule another scan chain.
fn retryDiscover(s: *app.State) void {
    if (!s.auto_refresh or s.retry_pending) return;
    s.retry_pending = true;
    nanoui.App.after(2_000, s, retryTick);
}

fn retryTick(ctx: ?*anyopaque) callconv(.c) void {
    const s: *app.State = @ptrCast(@alignCast(ctx.?));
    s.retry_pending = false;
    if (s.client != null) return;
    if (s.job != null) {
        s.discover_wanted = true;
        return;
    }
    discover(s);
}

fn rememberEngine(s: *app.State, rt: *const discovery.Runtime) void {
    if (s.last_engine_id) |old| {
        if (rt.info.ID.len > 0 and std.mem.eql(u8, old, rt.info.ID)) {
            if (s.last_engine_socket) |old_socket| {
                if (std.mem.eql(u8, old_socket, rt.socket)) return;
            }
        }
        s.gpa.free(old);
        s.last_engine_id = null;
    }
    if (s.last_engine_socket) |old| {
        s.gpa.free(old);
        s.last_engine_socket = null;
    }
    if (rt.info.ID.len > 0) s.last_engine_id = s.gpa.dupe(u8, rt.info.ID) catch null;
    s.last_engine_socket = s.gpa.dupe(u8, rt.socket) catch null;
}

fn rememberSocket(s: *app.State, path: []const u8) void {
    if (s.preferred_socket) |old| {
        if (std.mem.eql(u8, old, path)) return;
        s.gpa.free(old);
        s.preferred_socket = null;
    }
    s.preferred_socket = s.gpa.dupe(u8, path) catch null;
    savePrefs(s);
}

fn savePrefs(s: *app.State) void {
    settings.save(s.gpa, s.io, .{
        .appearance = s.appearance,
        .socket = s.preferred_socket orelse "",
    }) catch {};
}
