//! A refresh is a worker thread, not a chain of callbacks on the UI thread.
//! The worker builds a complete snapshot over its own socket and arena, then
//! posts one `done` callback to the main queue. The window never waits on an
//! engine request; it keeps painting the previous snapshot until the swap.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const volumes_mod = @import("engine/volumes.zig");
const discovery = @import("engine/discovery.zig");
const format = @import("ui/format.zig");
const stats_pool = @import("stats_pool.zig");
const engine_events = @import("engine/events.zig");
const events_feed = @import("events_feed.zig");
const logs = @import("logs.zig");
const bulk = @import("bulk.zig");
const refresh_selection = @import("refresh_selection.zig");
const tray = @import("tray.zig");
const image_tasks = @import("image_tasks.zig");
const session = @import("session.zig");
const host = @import("host/host.zig");
const Snapshot = @import("snapshot.zig").Snapshot;
const Stat = stats_pool.Sample;

pub const Job = struct {
    state: *app.State,
    io: std.Io,
    gpa: std.mem.Allocator,
    socket: []u8,
    generation: u64,
    snapshot: Snapshot,
    completion: *Completion,
    follow: bool = false,
    follow_id: [64]u8 = undefined,
    follow_len: usize = 0,
    logs_timestamps: bool = false,
    error_text: []const u8 = "",
    engine_dead: bool = false,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    thread: ?std.Thread = null,
};

/// The callback context outlives the job at teardown. `discard` frees the job
/// and nulls `job`; the queued `done` then frees only this small context.
pub const Completion = struct {
    gpa: std.mem.Allocator,
    job: ?*Job = null,
};

pub fn start(s: *app.State) void {
    if (s.client == null or s.job != null) return;
    const client = &s.client.?;
    const socket = s.gpa.dupe(u8, client.socket) catch {
        s.error_text = "out of memory preparing a refresh";
        s.redraw();
        session.armAutoTick(s);
        return;
    };
    const job = s.gpa.create(Job) catch {
        s.gpa.free(socket);
        s.error_text = "out of memory preparing a refresh";
        s.redraw();
        session.armAutoTick(s);
        return;
    };
    const completion = s.gpa.create(Completion) catch {
        s.gpa.destroy(job);
        s.gpa.free(socket);
        s.error_text = "out of memory preparing a refresh";
        s.redraw();
        session.armAutoTick(s);
        return;
    };
    completion.* = .{ .gpa = s.gpa, .job = job };
    job.* = .{
        .state = s,
        .io = s.io,
        .gpa = s.gpa,
        .socket = socket,
        .generation = s.generation,
        .snapshot = Snapshot.init(s.gpa),
        .completion = completion,
        .follow = s.log_opts.follow and s.detail_kind == .container,
        .logs_timestamps = s.log_opts.timestamps,
    };
    if (job.follow) {
        if (s.selected) |id| {
            job.follow_len = @min(id.len, job.follow_id.len);
            @memcpy(job.follow_id[0..job.follow_len], id[0..job.follow_len]);
        }
    }
    job.thread = std.Thread.spawn(.{}, work, .{job}) catch {
        completion.job = null;
        s.gpa.destroy(completion);
        job.snapshot.deinit();
        s.gpa.free(socket);
        s.gpa.destroy(job);
        s.error_text = "could not start a refresh";
        s.redraw();
        session.armAutoTick(s);
        return;
    };
    s.job = job;
    s.error_text = "";
    s.redraw();
}

fn work(job: *Job) void {
    load(job);
    nanoui.App.after(0, job.completion, done);
}

/// The worker checks this between requests. Quitting joins the thread, so a
/// canceled pass must not start another step: the one request already in
/// flight (a stats sample, a list call, `ps`) is the bound on that join.
fn stopRequested(canceled: *const std.atomic.Value(bool)) bool {
    return canceled.load(.acquire);
}

/// Follow is opt-in for the pass: a selected row may put an id in the job, but
/// only a job that captured follow=true may spend a request on its tail.
pub fn loadLogs(job: *Job, client: *api.Client) void {
    if (!job.follow or job.follow_len == 0) return;
    const id = job.follow_id[0..job.follow_len];
    if (client.logs(id, 500, job.logs_timestamps)) |text| {
        job.snapshot.logs = text;
        job.snapshot.logs_id = job.snapshot.arena.allocator().dupe(u8, id) catch "";
        job.snapshot.logs_timestamps = job.logs_timestamps;
    } else |_| {}
}

fn load(job: *Job) void {
    const canceled = &job.canceled;
    if (stopRequested(canceled)) return;

    var client = api.Client.init(job.io, job.gpa, job.socket);
    client.alloc = job.snapshot.arena.allocator();

    const alive = discovery.probe(&client);
    if (stopRequested(canceled)) return;
    if (!alive) {
        job.error_text = "engine stopped answering";
        job.engine_dead = true;
        return;
    }
    const version = client.version() catch {
        if (stopRequested(canceled)) return;
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not read the engine version";
        return;
    };
    if (stopRequested(canceled)) return;
    client.adoptApi(version.ApiVersion);
    job.snapshot.containers = client.containers() catch {
        if (stopRequested(canceled)) return;
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list containers";
        return;
    };
    if (stopRequested(canceled)) return;

    job.snapshot.stats = job.snapshot.arena.allocator().alloc(Stat, job.snapshot.containers.len) catch &.{};
    for (job.snapshot.containers, 0..) |ct, i| {
        if (i < job.snapshot.stats.len) job.snapshot.stats[i] = .{ .id = ct.Id };
    }
    // Stats are sampled on every pass, whichever page is open: the worker is
    // off the UI thread, and a snapshot without them would show 0.0% for the
    // first few seconds after switching to Containers.
    stats_pool.sample(job.io, job.socket, client.api, job.snapshot.containers, job.snapshot.stats, canceled);
    job.snapshot.stats_complete = stats_pool.allSampled(job.snapshot.containers, job.snapshot.stats);
    if (stopRequested(canceled)) return;

    // A failed list must abort the snapshot, not commit an empty one: losing
    // images/volumes/networks for a pass reads as "they were deleted".
    job.snapshot.images = client.images() catch {
        if (stopRequested(canceled)) return;
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list images";
        return;
    };
    if (stopRequested(canceled)) return;
    const volumes = client.volumes() catch {
        if (stopRequested(canceled)) return;
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list volumes";
        return;
    };
    if (stopRequested(canceled)) return;
    job.snapshot.volumes = volumes.Volumes;
    job.snapshot.networks = client.networks() catch {
        if (stopRequested(canceled)) return;
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list networks";
        return;
    };
    if (stopRequested(canceled)) return;
    loadLogs(job, &client);
    if (stopRequested(canceled)) return;

    if (client.df()) |df| {
        job.snapshot.df = df;
    } else |_| {
        job.snapshot.df_error = true;
    }
    if (stopRequested(canceled)) return;

    // Engine helper memory: `ps` on macOS, a /proc walk on Linux. Sampled here
    // on the worker; it is the one host figure that needs a process spawn.
    job.snapshot.engine_rss = host.engineRss(job.io, job.gpa);
    if (stopRequested(canceled)) return;

    // Events are an interval, not a store: a failed call must not abort the
    // snapshot, and the cursor only moves when the interval actually arrived.
    const until_events = format.now();
    var since = job.state.events.since;
    if (since == 0) since = until_events - 600;
    if (client.events(since, until_events)) |body| {
        defer job.gpa.free(body);
        var fresh = engine_events.parse(job.snapshot.arena.allocator(), body);
        // A first load can cover thousands of events; keep the newest Cap so
        // the merge on the main thread stays small.
        if (fresh.len > engine_events.Cap) fresh = fresh[fresh.len - engine_events.Cap ..];
        job.snapshot.events = fresh;
        job.snapshot.events_since = since;
        job.snapshot.events_until = until_events;
    } else |_| {
        job.snapshot.events_error = true;
    }
}

fn done(ctx: ?*anyopaque) callconv(.c) void {
    const completion: *Completion = @ptrCast(@alignCast(ctx.?));
    defer completion.gpa.destroy(completion);
    const job = completion.job orelse return;
    completion.job = null;

    if (job.thread) |thread| thread.join();
    const s = job.state;
    if (s.job != job) {
        cleanup(job);
        return;
    }
    s.job = null;

    if (job.canceled.load(.acquire) or job.generation != s.generation) {
        cleanup(job);
        s.redraw();
        scheduleNext(s);
        return;
    }

    if (job.error_text.len > 0) {
        const n = @min(job.error_text.len, s.error_buf.len);
        @memcpy(s.error_buf[0..n], job.error_text[0..n]);
        s.error_text = s.error_buf[0..n];
        if (job.engine_dead) s.client = null;
        cleanup(job);
        // The tray promised live numbers a moment ago; the pass that just
        // failed is when that promise breaks.
        tray.update(s);
        s.redraw();
        scheduleNext(s);
        return;
    }

    commit(s, job);
    cleanup(job); // the snapshot arena was moved into the state
    // A `BERTH_ACT` verb runs after the commit, through the same path a click
    // uses, with a fresh list to resolve the name against.
    if (s.pending_act.len > 0) {
        const spec = s.pending_act;
        s.pending_act = "";
        s.pending_act_saved = spec;
        // The action job starts its own refresh when it finishes; arming a
        // second timer here would run a redundant pass over the top of it.
        if (pendingAct(s)) return;
    }
    scheduleNext(s);
    s.redraw();
}

fn cleanup(job: *Job) void {
    job.snapshot.deinit();
    job.gpa.free(job.socket);
    job.gpa.destroy(job);
}

/// A click that asked for a refresh during this pass gets one immediately;
/// otherwise the next pass is five seconds after this one finished. Queued
/// engine switches and rescans go through the same state machine as a
/// mutation's completion.
fn scheduleNext(s: *app.State) void {
    session.@"resume"(s);
}

/// Stop a job at shutdown: the worker may be inside a socket read, so join it
/// before the state and its arena disappear. The job, snapshot and socket are
/// freed here; the queued `done` still owns the small completion context and
/// must not touch the state. If the loop stops before dispatching it, at most
/// that one tiny context leaks — never the job or its arena.
pub fn discard(s: *app.State, job: *Job) void {
    if (s.job == job) s.job = null;
    job.canceled.store(true, .release);
    if (job.thread) |thread| {
        thread.join();
        job.thread = null;
    }
    const completion = job.completion;
    completion.job = null;
    cleanup(job);
}

fn commit(s: *app.State, job: *Job) void {
    const snap = &job.snapshot;
    // Decide the df before the old arena dies. A df may survive exactly one
    // rotation (pointing into the then-retired arena); a second timed-out pass
    // must drop it before freeing that arena, or the views read freed memory.
    var next_df: ?api.Df = null;
    if (snap.df) |df| {
        next_df = df;
        s.df_stale = false;
    } else if (s.df != null and !s.df_stale) {
        next_df = s.df;
        s.df_stale = true;
    } else {
        s.df_stale = false;
    }

    if (s.retired) |*old| old.deinit();
    s.retired = s.arena;
    s.arena = snap.arena;
    snap.arena = .init(job.gpa); // the arena now belongs to the state

    s.containers = snap.containers;
    // Once per snapshot, not once per frame: a checkbox tick that survives
    // the refresh must still name a row the engine lists.
    s.bulk.retain(s.containers);
    s.stats = snap.stats;
    s.stats_complete = snap.stats_complete;
    s.images = snap.images;
    s.volumes = snap.volumes;
    s.networks = snap.networks;
    s.df = next_df;
    // A failed `ps` leaves the last known figure rather than flashing zero.
    if (snap.engine_rss) |rss| s.engine_rss = rss;
    // `/volumes` does not carry size; the df does. Use the last good df even
    // when this pass timed out, or every volume row loses its size again.
    if (s.df) |df| volumes_mod.mergeVolumeUsage(s.volumes, df);
    // Engine version/info come from discovery and stay in runtime memory;
    // copying them out of this arena on every pass is how a failed /info used
    // to leave a string pointing into the retired snapshot.
    if (s.client) |*client| client.alloc = s.arena.allocator();

    refresh_selection.resolve(s);
    s.events_error = snap.events_error;
    s.df_error = snap.df_error;
    events_feed.commit(s, snap.events_since, snap.events_until, snap.events);
    logs.commitSnapshot(s, snap.logs, snap.logs_id, snap.logs_timestamps);
    s.error_text = "";
    s.last_update = format.now();
    s.sample();
    tray.update(s);
    if (s.pending_image != .none) {
        const pending = s.pending_image;
        s.pending_image = .none;
        switch (pending) {
            .none => {},
            .pull => image_tasks.startPull(s),
            .run => image_tasks.startRun(s),
        }
    }
}

/// Resolves the container named in `pending_act` and runs the verb against it
/// through the same path the buttons use. `BERTH_ACT=pause:web` is how the
/// action paths are exercised without a mouse.
pub fn pendingAct(s: *app.State) bool {
    const spec = s.pending_act_saved;
    const colon = std.mem.indexOfScalar(u8, spec, ':') orelse return false;
    const verb = spec[0..colon];
    const name = spec[colon + 1 ..];
    for (s.containers) |ct| {
        if (!std.mem.eql(u8, ct.name(), name)) continue;
        var buf: [256]u8 = undefined;
        const Word = enum { stop, start, pause, unpause, resume_, restart, delete };
        const word_raw = if (std.mem.eql(u8, verb, "resume")) "resume_" else verb;
        const word = std.meta.stringToEnum(Word, word_raw) orelse return false;
        const method: []const u8 = if (word == .delete) "DELETE" else "POST";
        const toast: []const u8 = switch (word) {
            .stop => "Stopped",
            .start => "Started",
            .pause => "Paused",
            .unpause, .resume_ => "Resumed",
            .restart => "Restarted",
            .delete => "Deleted",
        };
        // Each branch has a comptime-known format, which bufPrint requires.
        const path: []const u8 = switch (word) {
            .stop => std.fmt.bufPrint(&buf, "/containers/{s}/stop", .{ct.Id}) catch return false,
            .start => std.fmt.bufPrint(&buf, "/containers/{s}/start", .{ct.Id}) catch return false,
            .pause => std.fmt.bufPrint(&buf, "/containers/{s}/pause", .{ct.Id}) catch return false,
            .unpause, .resume_ => std.fmt.bufPrint(&buf, "/containers/{s}/unpause", .{ct.Id}) catch return false,
            .restart => std.fmt.bufPrint(&buf, "/containers/{s}/restart", .{ct.Id}) catch return false,
            .delete => std.fmt.bufPrint(&buf, "/containers/{s}?force=1", .{ct.Id}) catch return false,
        };
        return s.act(method, path, toast);
    }
    return false;
}

/// One refresh, then one timer: scheduled from the end of a pass, never on top
/// of one that is still running. `token` proves this callback belongs to the
/// current chain; an older one stands down and lets the replacement arm.
pub fn autoTick(s: *app.State, token: u64) void {
    if (s.closing) return;
    if (token != s.auto_tick_token) {
        session.@"resume"(s);
        return;
    }
    s.auto_tick_pending = false;
    // A mutation owns the next step; its finish refreshes and arms the next
    // tick, so a pass started here would only validate the pre-mutation state.
    if (s.mutating()) return;
    // A scan in flight owns the next step; its completion starts the refresh
    // (or schedules the retry). Starting another scan from this timer would
    // replace the first refresh after adoption with a redundant rescan.
    if (s.job != null or s.discovery_job != null) return;
    if (s.client == null) {
        s.discover();
        return;
    }
    s.refresh();
}

test "a canceled pass reports stop before the next step" {
    var canceled = std.atomic.Value(bool).init(false);
    try std.testing.expect(!stopRequested(&canceled));
    canceled.store(true, .release);
    try std.testing.expect(stopRequested(&canceled));
}

test "a closing state ignores a queued tick" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = undefined,
        .closing = true,
        .auto_tick_pending = true,
    };
    autoTick(&state, state.auto_tick_token);
    try std.testing.expect(state.auto_tick_pending);
}

test "a canceled load stops before the next request" {
    const MockEngine = @import("engine/test_server.zig").MockEngine;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();
    var completion: Completion = .{ .gpa = std.testing.allocator };
    var job: Job = .{
        .state = &state,
        .io = std.testing.io,
        .gpa = std.testing.allocator,
        .socket = path,
        .generation = 0,
        .snapshot = Snapshot.init(std.testing.allocator),
        .completion = &completion,
    };
    completion.job = &job;
    defer job.snapshot.deinit();

    var mock: MockEngine = .{
        .io = std.testing.io,
        .listener = &listener,
        .cancel_after = 1,
        .canceled = &job.canceled,
    };
    try mock.start();
    defer mock.finish() catch {};

    // The mock cancels while answering /_ping. load must return instead of
    // opening the next connection for /version.
    load(&job);
    try std.testing.expect(job.canceled.load(.acquire));
    try std.testing.expectEqual(@as(usize, 0), job.error_text.len);
    try std.testing.expect(!job.engine_dead);
    try mock.finish();
    try std.testing.expectEqual(@as(usize, 1), mock.count);
    try std.testing.expect(std.mem.indexOf(u8, mock.pathAt(0), "/_ping") != null);
}

test "discard frees the job and leaves the queued done only a context" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    const completion = try state.gpa.create(Completion);
    completion.* = .{ .gpa = state.gpa };
    const job = try state.gpa.create(Job);
    job.* = .{
        .state = &state,
        .io = std.testing.io,
        .gpa = state.gpa,
        .socket = try state.gpa.dupe(u8, "/nonexistent/berth.sock"),
        .generation = 0,
        .snapshot = Snapshot.init(state.gpa),
        .completion = completion,
    };
    completion.job = job;
    state.job = job;
    job.thread = try std.Thread.spawn(.{}, struct {
        fn idle() void {}
    }.idle, .{});
    discard(&state, job);
    try std.testing.expect(state.job == null);
    try std.testing.expect(completion.job == null);
    // The queued done now owns only the context; the testing allocator proves
    // the job, its snapshot arena and its socket were freed exactly once.
    done(@ptrCast(completion));
}

test "a canceled done frees without committing or notifying" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
    };
    defer state.arena.deinit();
    var kept = [_]api.Container{.{ .Id = "kept", .State = "running" }};
    state.containers = &kept;
    state.error_text = "previous";
    state.last_update = 123;

    const completion = try state.gpa.create(Completion);
    completion.* = .{ .gpa = state.gpa };
    const job = try state.gpa.create(Job);
    job.* = .{
        .state = &state,
        .io = std.testing.io,
        .gpa = state.gpa,
        .socket = try state.gpa.dupe(u8, "/nonexistent/berth.sock"),
        .generation = 0,
        .snapshot = Snapshot.init(state.gpa),
        .completion = completion,
        .canceled = std.atomic.Value(bool).init(true),
    };
    var discarded = [_]api.Container{.{ .Id = "discarded", .State = "running" }};
    job.snapshot.containers = &discarded;
    completion.job = job;
    state.job = job;
    done(@ptrCast(completion));

    try std.testing.expect(state.job == null);
    try std.testing.expect(state.containers.ptr == @as([*]api.Container, @ptrCast(&kept)));
    try std.testing.expectEqualStrings("previous", state.error_text);
    try std.testing.expectEqual(@as(i64, 123), state.last_update);
    try std.testing.expectEqual(@as(usize, 0), state.history.count);
    try std.testing.expect(state.toast == null);
}
