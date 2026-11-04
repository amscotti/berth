//! The worker behind bulk start/stop/delete. One job runs at a time. A user
//! Cancel asks it to stop between items but keeps the job installed, so its
//! finish can join, report, and free it. Teardown is the only path that
//! abandons a job, deliberately leaking it rather than waiting on a socket.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const bulk_batch = @import("bulk_batch.zig");
const discovery = @import("engine/discovery.zig");
const session = @import("session.zig");

const max = bulk_batch.max;
const id_len = bulk_batch.id_len;

pub const Route = enum {
    start,
    stop,
    delete,

    pub fn verb(self: Route) []const u8 {
        return switch (self) {
            .start => "Started",
            .stop => "Stopped",
            .delete => "Deleted",
        };
    }

    pub fn word(self: Route) []const u8 {
        return switch (self) {
            .start => "Start",
            .stop => "Stop",
            .delete => "Delete",
        };
    }

    fn method(self: Route) []const u8 {
        return switch (self) {
            .start, .stop => "POST",
            .delete => "DELETE",
        };
    }

    /// Whether the route would actually change a container in this state. A
    /// batch that sends start to a running row is a wasted request, and its
    /// answer says nothing about work the user asked for.
    pub fn changes(self: Route, up: bool) bool {
        return switch (self) {
            .start => !up,
            .stop => up,
            .delete => true,
        };
    }
};

pub const Job = struct {
    state: *app.State,
    gpa: std.mem.Allocator,
    io: std.Io,
    route: Route,
    total: usize,
    done: usize = 0,
    socket: [512]u8 = undefined,
    socket_len: usize = 0,
    api_version: [16]u8 = undefined,
    api_len: usize = 0,
    ids: [max][id_len]u8 = undefined,
    lens: [max]u8 = undefined,
    count: usize,
    first_error: []const u8 = "",
    /// The Compose project this batch belongs to, for the completion toast.
    project: [128]u8 = undefined,
    project_len: usize = 0,
    thread: ?std.Thread = null,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    /// Set only by teardown: the worker must not post a callback once the
    /// state is about to disappear.
    abandoned: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

/// Returns whether the worker started; a failed start leaves the caller's
/// selection alone so the user can retry.
pub fn start(s: *app.State, route: Route, ids: []const []const u8, project: []const u8) bool {
    if (s.mutating()) {
        s.notify("wait for the current engine operation to finish", true);
        return false;
    }
    if (ids.len == 0 or ids.len > max) return false;
    var client = s.client orelse {
        s.notify("no engine is selected", true);
        return false;
    };

    const job = s.gpa.create(Job) catch {
        s.notify("could not start the bulk action", true);
        return false;
    };
    job.* = .{
        .state = s,
        .gpa = s.gpa,
        .io = s.io,
        .route = route,
        .total = ids.len,
        .count = ids.len,
    };
    job.socket_len = @min(client.socket.len, job.socket.len);
    @memcpy(job.socket[0..job.socket_len], client.socket[0..job.socket_len]);
    job.api_len = @min(client.api.len, job.api_version.len);
    @memcpy(job.api_version[0..job.api_len], client.api[0..job.api_len]);
    job.project_len = @min(project.len, job.project.len);
    @memcpy(job.project[0..job.project_len], project[0..job.project_len]);
    for (ids, 0..) |id, i| {
        job.lens[i] = @intCast(@min(id.len, id_len));
        @memcpy(job.ids[i][0..job.lens[i]], id[0..job.lens[i]]);
    }

    job.thread = std.Thread.spawn(.{}, work, .{job}) catch {
        s.gpa.destroy(job);
        s.notify("could not start the bulk action", true);
        return false;
    };
    // Any queued confirmation is about a set this batch is replacing. Only
    // clear it once the worker is actually running, or a refused request would
    // close the dialog the user is still deciding on.
    s.bulk.pending = null;
    s.bulk.pending_batch.count = 0;
    s.bulk.project_len = 0;
    s.bulk.job = job;
    s.bulk.canceling = false;
    s.redraw();
    return true;
}

/// Whether the worker may post its finish callback, and whether finish may
/// touch the state. Canceling is not a reason to go quiet: the job still needs
/// joining and freeing. Only teardown abandons it, because the state may be
/// gone before the callback runs.
fn completionWanted(job: *const Job) bool {
    return !job.abandoned.load(.acquire);
}

fn work(job: *Job) void {
    const page = std.heap.page_allocator;
    var arena = std.heap.ArenaAllocator.init(page);
    defer arena.deinit();
    var client = api.Client.init(job.io, page, job.socket[0..job.socket_len]);
    client.api = job.api_version[0..job.api_len];
    client.alloc = arena.allocator();

    var path_buf: [128]u8 = undefined;
    var i: usize = 0;
    while (i < job.count) : (i += 1) {
        if (job.canceled.load(.acquire)) break;
        const id = job.ids[i][0..job.lens[i]];
        const path = switch (job.route) {
            .start => std.fmt.bufPrint(&path_buf, "/containers/{s}/start", .{id}) catch break,
            .stop => std.fmt.bufPrint(&path_buf, "/containers/{s}/stop", .{id}) catch break,
            .delete => std.fmt.bufPrint(&path_buf, "/containers/{s}?force=1", .{id}) catch break,
        };
        client.action(job.route.method(), path) catch {
            if (job.first_error.len == 0 and client.err_body.len > 0) {
                job.first_error = page.dupe(u8, client.err_body) catch "";
            }
            continue;
        };
        job.done += 1;
    }
    if (!completionWanted(job)) return; // teardown owns the leak
    nanoui.App.after(0, job, finishBridge);
}

fn finishBridge(ctx: ?*anyopaque) callconv(.c) void {
    const job: *Job = @ptrCast(@alignCast(ctx.?));
    finish(job);
}

const Done = struct {
    route: Route,
    done: usize,
    total: usize,
    project: [128]u8 = undefined,
    project_len: usize = 0,
    error_text: []const u8 = "",
    canceled: bool = false,
};

/// Joins and frees `job`. A stale callback still frees its own job but returns
/// null, so it cannot report against or clear a newer batch. Everything the
/// report needs is copied before the job dies.
fn retire(s: *app.State, job: *Job) ?Done {
    if (job.thread) |thread| thread.join();

    const current = s.bulk.job == job;
    if (current) {
        s.bulk.job = null;
        s.bulk.canceling = false;
    }
    var done: Done = .{
        .route = job.route,
        .done = job.done,
        .total = job.total,
        .project_len = job.project_len,
        .error_text = if (job.first_error.len > 0) s.gpa.dupe(u8, job.first_error) catch "" else "",
        .canceled = job.canceled.load(.acquire),
    };
    @memcpy(done.project[0..done.project_len], job.project[0..done.project_len]);
    const page = std.heap.page_allocator;
    if (job.first_error.len > 0) page.free(job.first_error);
    s.gpa.destroy(job);
    if (!current) {
        if (done.error_text.len > 0) s.gpa.free(done.error_text);
        return null;
    }
    return done;
}

fn finish(job: *Job) void {
    // Teardown may free the state after this callback was queued.
    if (!completionWanted(job)) return;
    const s = job.state;
    const done = retire(s, job) orelse return;
    defer {
        if (done.error_text.len > 0) s.gpa.free(done.error_text);
    }

    if (done.done == done.total) {
        if (done.project_len > 0) {
            const project = done.project[0..done.project_len];
            s.notifyFmt("{s} {d} in {s}", .{ done.route.verb(), done.done, project }, false);
        } else {
            s.notifyFmt("{s} {d} container{s}", .{ done.route.verb(), done.done, plural(done.done) }, false);
        }
    } else if (done.canceled) {
        s.notify("bulk action canceled", false);
    } else {
        s.notifyFmt("{s} {d} of {d} containers", .{ done.route.verb(), done.done, done.total }, true);
        if (done.error_text.len > 0) s.notifyFmt("{s}", .{firstLine(done.error_text)}, true);
    }
    // Even with nothing queued, the rows just changed; resume must refresh,
    // not merely arm the next automatic tick. The disk cache is invalidated
    // too, because a stopped or deleted container changes reclaimable space.
    s.df.invalidate();
    s.refresh_wanted = true;
    session.@"resume"(s);
    s.redraw();
}

/// User Cancel: stop between items, but keep the job installed so its finish
/// can join, free it, and report what actually happened.
pub fn cancel(s: *app.State) void {
    const job = s.bulk.job orelse return;
    if (s.bulk.canceling) return;
    job.canceled.store(true, .release);
    s.bulk.canceling = true;
    s.redraw();
}

/// Teardown: never wait for a worker that may be blocked on a socket. The job
/// is deliberately leaked; `abandoned` keeps any queued callback away from the
/// state that is about to disappear.
pub fn discard(s: *app.State) void {
    const job = s.bulk.job orelse return;
    job.abandoned.store(true, .release);
    job.canceled.store(true, .release);
    if (job.thread) |thread| thread.detach();
    s.bulk.job = null;
    s.bulk.canceling = false;
}

fn firstLine(text: []const u8) []const u8 {
    const end = std.mem.indexOfScalar(u8, text, '\n') orelse @min(text.len, 90);
    return text[0..@min(end, 90)];
}

pub fn plural(n: usize) []const u8 {
    return if (n == 1) "" else "s";
}

fn fakeJob(state: *app.State) !*Job {
    const job = try state.gpa.create(Job);
    job.* = .{
        .state = state,
        .gpa = state.gpa,
        .io = state.io,
        .route = .stop,
        .total = 3,
        .count = 3,
    };
    return job;
}

test "a refused batch leaves the open confirmation in place" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = undefined,
    };
    state.bulk.pending = .{ .route = .stop };
    state.bulk.pending_batch.count = 2;
    state.bulk.project_len = 4;
    @memcpy(state.bulk.project[0..4], "shop");

    // No client is a refusal before the worker exists; the dialog must survive
    // so the user can pick an engine or retry, not vanish under the pointer.
    try std.testing.expect(!start(&state, .stop, &.{"abc"}, "shop"));
    try std.testing.expect(state.bulk.pending != null);
    try std.testing.expectEqual(@as(usize, 2), state.bulk.pending_batch.count);
    try std.testing.expectEqual(@as(usize, 4), state.bulk.project_len);
}

test "a canceled job is retired once and a stale callback cannot adopt a newer one" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = undefined,
    };

    // The job the bar shows is canceled and then retired. testing.allocator
    // fails the test if the Job leaks or is freed twice.
    const canceled = try fakeJob(&state);
    state.bulk.job = canceled;
    cancel(&state);
    try std.testing.expect(state.bulk.job == canceled);
    try std.testing.expect(state.bulk.canceling);
    try std.testing.expect(canceled.canceled.load(.acquire));
    const done = retire(&state, canceled) orelse return error.JobLost;
    try std.testing.expect(done.canceled);
    try std.testing.expectEqual(@as(usize, 3), done.total);
    try std.testing.expect(state.bulk.job == null);
    try std.testing.expect(!state.bulk.canceling);

    // A callback from an old job frees itself but must leave the installed
    // job and the Canceling bar alone.
    const stale = try fakeJob(&state);
    const current = try fakeJob(&state);
    state.bulk.job = current;
    state.bulk.canceling = true;
    try std.testing.expect(retire(&state, stale) == null);
    try std.testing.expect(state.bulk.job == current);
    try std.testing.expect(state.bulk.canceling);

    _ = retire(&state, current) orelse return error.JobLost;
    try std.testing.expect(state.bulk.job == null);
    try std.testing.expect(!state.bulk.canceling);
}

test "canceling still completes; only teardown goes quiet" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = undefined,
    };
    const job = try fakeJob(&state);
    defer state.gpa.destroy(job);

    try std.testing.expect(completionWanted(job));
    // A user Cancel must still post finish: that is how the job is joined,
    // reported, and freed.
    job.canceled.store(true, .release);
    try std.testing.expect(completionWanted(job));
    // Teardown is the only reason to stay quiet.
    job.abandoned.store(true, .release);
    try std.testing.expect(!completionWanted(job));
}

test "a bulk completion refreshes with nothing queued" {
    const Hook = struct {
        var starts: usize = 0;
        fn refresh(_: *app.State) void {
            starts += 1;
        }
        fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !discovery.Found {
            _ = io;
            _ = preferred;
            return .{ .runtimes = try gpa.alloc(discovery.Runtime, 0) };
        }
    };
    Hook.starts = 0;
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
        .discovery_test = .{ .scan = Hook.scan, .start_refresh = Hook.refresh },
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/bulk-refresh.sock");
    state.df.noteAttempt();

    const job = try fakeJob(&state);
    state.bulk.job = job;
    finish(job);

    try std.testing.expect(state.bulk.job == null);
    try std.testing.expect(!state.refresh_wanted);
    try std.testing.expect(state.df.wanted());
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);
}

test "teardown drops the job without leaving a Canceling bar" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = undefined,
    };
    const job = try fakeJob(&state);
    state.bulk.job = job;
    state.bulk.canceling = true;
    discard(&state);
    try std.testing.expect(state.bulk.job == null);
    try std.testing.expect(!state.bulk.canceling);
    try std.testing.expect(job.abandoned.load(.acquire));
    // Teardown deliberately leaks; the test owns this fake job instead.
    state.gpa.destroy(job);
}
