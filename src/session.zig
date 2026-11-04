//! The engine session: discovery, choosing a socket, and starting refreshes.
//! Kept out of `app.zig` so the state and its views stay readable, and so the
//! one place that persists a preferred socket is easy to find.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const discovery = @import("engine/discovery.zig");
const refresh_mod = @import("refresh.zig");
const auto_tick = @import("auto_tick.zig");
const runtimes_mod = @import("runtimes.zig");
const settings = @import("settings.zig");
const tray = @import("tray.zig");

/// A socket scan, replaceable in tests so the asynchronous path can be
/// exercised without a listener.
pub const ScanFn = *const fn (std.mem.Allocator, std.Io, []const u8) anyerror!discovery.Found;

pub const TestHook = struct {
    scan: ScanFn,
    /// The worker does not post `done`; the test joins and calls it itself.
    dispatch: bool = false,
    /// Counts a production `refresh` call from adoption without spawning a
    /// worker at a fake socket. Null means the real refresh module.
    start_refresh: ?*const fn (*app.State) void = null,
};

/// One scan on a worker. The job owns copies of everything the worker reads:
/// the preferred socket and the old engine identity. It never reads mutable
/// `State`, and the main thread only touches this record after the join.
pub const Job = struct {
    /// Read on the main thread in `done`; the worker never dereferences it.
    state: *app.State,
    gpa: std.mem.Allocator,
    io: std.Io,
    preferred: []u8,
    old_id: []u8,
    old_socket: []u8,
    old_dialect: ?discovery.Dialect,
    scan: ScanFn,
    dispatch: bool,
    found: ?discovery.Found = null,
    /// Teardown cannot wait for a scan that may be stuck in a connect, so it
    /// abandons the job and the completion callback frees only the job.
    abandoned: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    thread: ?std.Thread = null,
};

pub fn selectRuntime(s: *app.State, index: usize) void {
    if (index >= s.runtimes.len) return;
    if (s.mutating()) {
        // The mutation still talks to the engine on screen. Switch after it
        // finishes, through the resume that its completion runs.
        s.select_wanted = index;
        return;
    }
    if (s.discovery_job != null) {
        s.notify("Wait for engine discovery to finish", true);
        return;
    }
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
///
/// The scan runs on a worker: a stale socket costs three seconds per probe and
/// version/info another ten, so doing it here used to freeze startup and
/// every rescan. The old runtimes and client stay usable until the result is
/// adopted, so a rescan keeps painting the current engine.
pub fn discover(s: *app.State) void {
    if (s.mutating()) {
        // Adopting a different engine would move the client under the job in
        // flight. Run the scan once that job has reported.
        s.discover_wanted = true;
        return;
    }
    if (s.job != null or s.discovery_job != null) {
        // A rescan/forget clicked mid-pass runs as soon as that pass ends.
        s.discover_wanted = true;
        return;
    }
    s.discover_wanted = false;
    start(s);
}

fn start(s: *app.State) void {
    // A scan is a new chain; an armed automatic pass belongs to the old one.
    s.auto_tick_token +%= 1;
    s.auto_tick_pending = false;
    const preferred = s.gpa.dupe(u8, s.preferred_socket orelse "") catch {
        startFailed(s, "out of memory preparing a scan");
        return;
    };
    const old_id = s.gpa.dupe(u8, s.last_engine_id orelse "") catch {
        s.gpa.free(preferred);
        startFailed(s, "out of memory preparing a scan");
        return;
    };
    const old_socket = s.gpa.dupe(u8, s.last_engine_socket orelse "") catch {
        s.gpa.free(preferred);
        s.gpa.free(old_id);
        startFailed(s, "out of memory preparing a scan");
        return;
    };
    const job = s.gpa.create(Job) catch {
        s.gpa.free(preferred);
        s.gpa.free(old_id);
        s.gpa.free(old_socket);
        startFailed(s, "out of memory preparing a scan");
        return;
    };
    const hook = s.discovery_test;
    job.* = .{
        .state = s,
        .gpa = s.gpa,
        .io = s.io,
        .preferred = preferred,
        .old_id = old_id,
        .old_socket = old_socket,
        .old_dialect = s.last_engine_dialect,
        .scan = if (hook) |h| h.scan else discovery.scan,
        .dispatch = if (hook) |h| h.dispatch else true,
    };
    job.thread = std.Thread.spawn(.{}, work, .{job}) catch {
        cleanup(job);
        startFailed(s, "could not start engine discovery");
        return;
    };
    s.discovery_job = job;
    s.generation += 1;
    tray.update(s);
    s.redraw();
}

fn startFailed(s: *app.State, message: []const u8) void {
    s.error_text = message;
    s.sampleHost();
    retryDiscover(s);
    tray.update(s);
}

fn work(job: *Job) void {
    job.found = job.scan(job.gpa, job.io, job.preferred) catch null;
    if (!job.dispatch) return;
    nanoui.App.after(0, job, done);
}

pub fn done(ctx: ?*anyopaque) callconv(.c) void {
    const job: *Job = @ptrCast(@alignCast(ctx.?));
    if (job.thread) |thread| {
        thread.join();
        job.thread = null;
    }
    // Teardown abandons the job instead of joining it; this callback then owns
    // only the job's own copies and must not dereference the freed state.
    if (job.abandoned.load(.acquire)) {
        cleanup(job);
        return;
    }
    const s = job.state;
    if (s.discovery_job != job) {
        cleanup(job);
        return;
    }
    s.discovery_job = null;
    if (s.mutating()) {
        // The scan is stale the moment a mutation starts: keep the engine on
        // screen until it reports, then scan again rather than moving its
        // toast and refresh onto a different engine.
        s.discover_wanted = true;
        cleanup(job);
        tray.update(s);
        s.redraw();
        return;
    }
    adopt(s, job);
    cleanup(job);
    tray.update(s);
    s.redraw();
}

fn adopt(s: *app.State, job: *Job) void {
    const maybe_found = job.found;
    job.found = null;
    // The old client points at a socket owned by `runtimes`; stop using it
    // before those strings are freed. Until this point both stayed alive so a
    // rescan did not blank the page.
    s.client = null;
    runtimes_mod.drop(s);

    const found = maybe_found orelse {
        discoveryFailed(s, "could not scan for container engines");
        return;
    };
    if (found.runtimes.len == 0) {
        var empty = found;
        empty.deinit(s.gpa);
        s.active = 0;
        discoveryFailed(s, "no container engine found");
        return;
    }
    s.runtimes = found.runtimes;
    // No reachable engine means no active one; leaving 0 selected would mark
    // the first dead socket "active" on the Runtimes page.
    s.active = chooseActive(s.runtimes);
    if (s.active >= s.runtimes.len) {
        discoveryFailed(s, "no container engine is answering");
        return;
    }
    const rt = &s.runtimes[s.active];
    if (!rt.reachable) {
        discoveryFailed(s, "no container engine is answering");
        return;
    }
    // Switching engines clears containers, images, stores and selection; a
    // rediscovery of the same engine keeps them. Compare the daemon's ID
    // first: Docker Desktop exposes one engine through several paths.
    if (engineChanged(job.old_id, job.old_socket, job.old_dialect, rt)) s.clearData();
    rememberEngine(s, rt);
    s.client = api.Client.init(s.io, s.gpa, rt.socket);
    if (rt.api.len > 0) s.client.?.api = rt.api;
    s.client.?.alloc = s.arena.allocator();
    s.generation += 1;
    if (takeRescan(s)) {
        s.refresh_wanted = false;
        discover(s);
        return;
    }
    s.refresh_wanted = false;
    refresh(s);
}

fn discoveryFailed(s: *app.State, message: []const u8) void {
    s.error_text = message;
    s.sampleHost();
    retryDiscover(s);
    if (takeRescan(s)) {
        s.refresh_wanted = false;
        discover(s);
    }
}

/// The first reachable runtime, or one past the end when none answers. That
/// sentinel keeps "nothing selected" from marking the first dead socket active.
pub fn chooseActive(runtimes: []const discovery.Runtime) usize {
    for (runtimes, 0..) |rt, i| {
        if (rt.reachable) return i;
    }
    return runtimes.len;
}

/// True when the engine behind the socket is not the one on screen. A dialect
/// change is always a new engine even on the same path. Docker Desktop exposes
/// one engine through several paths, so the daemon ID wins there; the socket
/// is the fallback. Podman's compat `/info` returns a fresh UUID for every
/// request, so its ID can never prove continuity and the socket has to decide.
pub fn engineChanged(old_id: []const u8, old_socket: []const u8, old_dialect: ?discovery.Dialect, rt: *const discovery.Runtime) bool {
    if (old_dialect) |old| {
        if (old != rt.dialect) return true;
    }
    if (rt.dialect != .podman and old_id.len > 0 and rt.info.ID.len > 0) {
        return !std.mem.eql(u8, old_id, rt.info.ID);
    }
    return old_socket.len > 0 and !std.mem.eql(u8, old_socket, rt.socket);
}

/// Take the rescan request queued while this scan ran. Separate so the re-arm
/// rule can be tested without starting a worker.
pub fn takeRescan(s: *app.State) bool {
    const wanted = s.discover_wanted;
    s.discover_wanted = false;
    return wanted;
}

fn cleanup(job: *Job) void {
    if (job.found) |found| found.deinit(job.gpa);
    job.gpa.free(job.preferred);
    job.gpa.free(job.old_id);
    job.gpa.free(job.old_socket);
    job.gpa.destroy(job);
}

/// A socket typed by hand. `unix://` is optional; the path is remembered so
/// the next launch probes it before anything else. The synchronous probe is
/// the remaining UI-thread discovery step; the rescan itself is on a worker,
/// so a successful call has no immediate verdict beyond "scan started".
pub fn useSocket(s: *app.State, path: []const u8) void {
    const trimmed = std.mem.trim(u8, path, " \t\r\n");
    const clean = if (std.mem.startsWith(u8, trimmed, "unix://")) trimmed["unix://".len..] else trimmed;
    if (clean.len == 0) {
        s.notify("Enter a socket path", true);
        return;
    }
    if (s.job != null) {
        s.notify("Wait for the current refresh to finish", true);
        return;
    }
    if (s.ops.running()) {
        s.notify("Wait for the engine command to finish", true);
        return;
    }
    // Probe before remembering: a typo must not become the preferred socket
    // that every later launch pays three seconds to reject.
    var probe_client = api.Client.init(s.io, s.gpa, clean);
    if (!discovery.probe(&probe_client)) {
        s.notify("That socket did not answer", true);
        return;
    }
    rememberSocket(s, clean);
    s.socket_input.set(clean);
    s.engine_menu_open = false;
    s.expanded_kind = null;
    // `clean` may have been a slice of the text buffer that `set` just moved;
    // the remembered copy is the one the asynchronous scan reads. During a
    // scan this queues another one behind it instead of blocking recovery.
    discover(s);
}

/// Drop the remembered socket and go back to automatic discovery. During a
/// scan this clears the preference immediately and queues the scan that will
/// use it, so a hung probe cannot lock the form out.
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
    // A pass over the old engine must not land on top of a scan about to
    // install a new one; the discovery completion starts the first pass.
    if (s.discovery_job != null) {
        s.refresh_wanted = true;
        return;
    }
    if (s.job != null) {
        s.refresh_wanted = true;
        return;
    }
    // A new pass replaces whatever tick was waiting on the old chain.
    s.auto_tick_token +%= 1;
    s.auto_tick_pending = false;
    if (s.discovery_test) |hook| {
        if (hook.start_refresh) |start_refresh| {
            start_refresh(s);
            return;
        }
    }
    refresh_mod.start(s);
}

/// Runs whatever a finished mutation or pass left queued, in priority order:
/// an engine switch, then a rescan, then a refresh. A mutation or pass that is
/// still running keeps the queue and calls this again when it finishes, so a
/// deferred choice is not lost and no second timer is armed. With nothing
/// queued, this is the one place that arms the next automatic pass.
pub fn @"resume"(s: *app.State) void {
    if (s.mutating()) return; // that mutation's finish calls resume
    if (s.job != null or s.discovery_job != null) return; // their done calls resume
    if (s.select_wanted) |index| {
        s.select_wanted = null;
        s.refresh_wanted = false;
        selectRuntime(s, index);
        return;
    }
    if (s.discover_wanted) {
        s.discover_wanted = false;
        s.refresh_wanted = false;
        discover(s);
        return;
    }
    if (s.refresh_wanted and s.client != null) {
        s.refresh_wanted = false;
        refresh(s);
        return;
    }
    s.refresh_wanted = false;
    armAutoTick(s);
}

/// The queue's timer entry point. The timer and its ownership live in
/// `auto_tick.zig`; the cadence decision stays beside the refresh completion.
pub fn armAutoTick(s: *app.State) void {
    auto_tick.arm(s);
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

pub fn retryTick(ctx: ?*anyopaque) callconv(.c) void {
    const s: *app.State = @ptrCast(@alignCast(ctx.?));
    s.retry_pending = false;
    // A scan owns the next step: its completion starts the refresh or
    // schedules the retry. Arming another scan here would queue a redundant
    // pass after the one already running.
    if (s.discovery_job != null) return;
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
                if (std.mem.eql(u8, old_socket, rt.socket) and s.last_engine_dialect == rt.dialect) return;
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
    s.last_engine_dialect = rt.dialect;
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

test "the active runtime is the first one that answers" {
    var none = [_]discovery.Runtime{
        .{ .reachable = false },
        .{ .reachable = false },
    };
    try std.testing.expectEqual(@as(usize, none.len), chooseActive(&none));

    var mixed = [_]discovery.Runtime{
        .{ .reachable = false },
        .{ .reachable = true },
        .{ .reachable = true },
    };
    try std.testing.expectEqual(@as(usize, 1), chooseActive(&mixed));

    const empty = [_]discovery.Runtime{};
    try std.testing.expectEqual(@as(usize, 0), chooseActive(&empty));
}

test "engine identity prefers the daemon id and falls back to the socket" {
    var rt = discovery.Runtime{
        .socket = "/run/docker.sock",
        .info = .{ .ID = "daemon-a" },
    };
    // Same daemon through another path is the same engine.
    try std.testing.expect(!engineChanged("daemon-a", "/old.sock", .docker, &rt));
    try std.testing.expect(engineChanged("daemon-b", "/run/docker.sock", .docker, &rt));

    // Without an ID on both sides, the socket decides.
    rt.info.ID = "";
    try std.testing.expect(!engineChanged("daemon-a", "/run/docker.sock", .docker, &rt));
    try std.testing.expect(engineChanged("daemon-a", "/other.sock", .docker, &rt));

    // Podman's compat /info returns a fresh UUID per request; the same socket
    // must still count as the same engine.
    var podman = discovery.Runtime{
        .dialect = .podman,
        .socket = "/run/podman/podman.sock",
        .info = .{ .ID = "request-42" },
    };
    try std.testing.expect(!engineChanged("request-41", "/run/podman/podman.sock", .podman, &podman));
    try std.testing.expect(engineChanged("request-41", "/other.sock", .podman, &podman));

    // A Docker engine replaced by Podman on the same path is a new engine,
    // even though Podman's id cannot be compared with Docker's.
    var podman_alias = discovery.Runtime{
        .dialect = .podman,
        .socket = "/run/docker.sock",
        .info = .{ .ID = "request-43" },
    };
    try std.testing.expect(engineChanged("daemon-a", "/run/docker.sock", .docker, &podman_alias));

    // First discovery has no old identity and is not a switch.
    try std.testing.expect(!engineChanged("", "", null, &rt));
}

test "a rescan requested during a scan is taken exactly once" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();
    state.discover_wanted = true;
    try std.testing.expect(takeRescan(&state));
    try std.testing.expect(!state.discover_wanted);
    try std.testing.expect(!takeRescan(&state));
}

test "discover returns before the socket scan finishes" {
    const Fake = struct {
        var main_thread: std.Thread.Id = undefined;
        var saw_main = std.atomic.Value(bool).init(false);
        var release = std.atomic.Value(bool).init(false);
        /// Only a test that deliberately asks for the synchronous path may
        /// accept the scan running on the caller's thread.
        var allow_sync = false;

        fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !discovery.Found {
            _ = io;
            _ = preferred;
            if (std.Thread.getCurrentId() == main_thread) {
                if (!allow_sync) return error.ScanRanOnCallerThread;
                saw_main.store(true, .release);
                return .{ .runtimes = try gpa.alloc(discovery.Runtime, 0) };
            }
            while (!release.load(.acquire)) std.Thread.yield() catch {};
            return .{ .runtimes = try gpa.alloc(discovery.Runtime, 0) };
        }
    };
    Fake.main_thread = std.Thread.getCurrentId();
    Fake.saw_main.store(false, .release);
    Fake.release.store(false, .release);
    Fake.allow_sync = false;

    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
        .discovery_test = .{ .scan = Fake.scan },
    };
    defer state.arena.deinit();

    discover(&state);
    const job = state.discovery_job orelse {
        // A missing job is only acceptable when the fake deliberately asked
        // for the synchronous path; otherwise discover failed to spawn.
        try std.testing.expect(Fake.allow_sync);
        return;
    };
    try std.testing.expect(state.discovery_job != null);
    Fake.release.store(true, .release);
    done(@ptrCast(job));

    try std.testing.expect(!Fake.saw_main.load(.acquire));
    try std.testing.expect(state.discovery_job == null);
    try std.testing.expectEqualStrings("no container engine found", state.error_text);
}
