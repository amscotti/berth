//! Tests for the action worker: plan shapes, worker requests against a mock
//! engine, and the completion that reports them. Kept apart from the module so
//! the worker itself stays inside the file-size rule.
const std = @import("std");
const app = @import("app.zig");
const actions = @import("actions.zig");
const api = @import("engine/api.zig");
const command = @import("engine/command.zig");
const discovery = @import("engine/discovery.zig");
const prune = @import("engine/prune.zig");
const bulk_action = @import("bulk_action.zig");
const session = @import("session.zig");

const Job = actions.Job;
const Kind = actions.Kind;
const StepOutcome = actions.StepOutcome;
const PruneOptions = actions.PruneOptions;

fn copy(target: []u8, len: *usize, text: []const u8) bool {
    if (text.len > target.len) return false;
    @memcpy(target[0..text.len], text);
    len.* = text.len;
    return true;
}

fn testState() app.State {
    return .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
    };
}

fn testJob(s: *app.State, kind: Kind, socket: []const u8) !*Job {
    const job = try s.gpa.create(Job);
    job.* = .{ .state = s, .gpa = s.gpa, .io = s.io, .kind = kind };
    if (!copy(&job.socket, &job.socket_len, socket) or
        !copy(&job.api_version, &job.api_len, "v1.43"))
    {
        s.gpa.destroy(job);
        return error.PathTooLong;
    }
    return job;
}

fn mockResponse(buf: []u8, status: u16, reason: []const u8, body: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "HTTP/1.1 {d} {s}\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}", .{ status, reason, body.len, body });
}

test "the prune plan keeps order and leaves unsupported routes out" {
    const all = PruneOptions{ .volumes = true, .networks = true };
    const docker = actions.planPrune(all, .docker);
    try std.testing.expectEqual(@as(usize, 5), docker.count);
    try std.testing.expectEqual(prune.Kind.containers, docker.steps[0]);
    try std.testing.expectEqual(prune.Kind.images, docker.steps[1]);
    try std.testing.expectEqual(prune.Kind.volumes, docker.steps[2]);
    try std.testing.expectEqual(prune.Kind.networks, docker.steps[3]);
    try std.testing.expectEqual(prune.Kind.build_cache, docker.steps[4]);

    // Podman has no build-cache route; the hidden row is not offered, so the
    // run is exactly these four steps and has no skip to report.
    const podman = actions.planPrune(all, .podman);
    try std.testing.expectEqual(@as(usize, 4), podman.count);
    try std.testing.expectEqual(prune.Kind.containers, podman.steps[0]);
    try std.testing.expectEqual(prune.Kind.images, podman.steps[1]);
    try std.testing.expectEqual(prune.Kind.volumes, podman.steps[2]);
    try std.testing.expectEqual(prune.Kind.networks, podman.steps[3]);

    const no_cache = actions.planPrune(PruneOptions{ .build_cache = false }, .docker);
    try std.testing.expectEqual(@as(usize, 2), no_cache.count);
}

test "a successful action reports its verb after the worker returns" {
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

    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener };
    try mock.start();
    defer mock.finish() catch {};

    const job = try testJob(&state, .action, path);
    try std.testing.expect(copy(&job.method, &job.method_len, "POST"));
    try std.testing.expect(copy(&job.path, &job.path_len, "/containers/abc/stop"));
    try std.testing.expect(copy(&job.verb, &job.verb_len, "Stopped"));
    state.actions.job = job;
    var worker: ?std.Thread = try std.Thread.spawn(.{}, actions.run, .{job});
    defer if (worker) |thread| thread.join();
    try mock.waitFor(1);
    worker.?.join();
    worker = null;
    try std.testing.expect(job.ok);
    try std.testing.expectEqual(@as(usize, 0), job.error_text.len);
    try mock.finish();
    try std.testing.expectEqualStrings("/v1.43/containers/abc/stop", mock.pathAt(0));

    actions.finish(job);
    try std.testing.expect(state.actions.job == null);
    try std.testing.expectEqualStrings("Stopped", state.toast.?.text);
    try std.testing.expect(!state.toast.?.bad);
}

test "the action entry point starts one job and completes it" {
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

    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.actions.dispatch = false; // the test joins and calls finish
    state.client = api.Client.init(state.io, state.gpa, path);
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener };

    try std.testing.expect(state.act("POST", "/containers/abc/stop", "Stopped"));
    const job = state.actions.job orelse return error.ActionJobMissing;
    try std.testing.expectEqual(@as(usize, 0), mock.served.load(.acquire));
    var worker: ?std.Thread = job.thread;
    defer if (worker) |thread| thread.join();

    // The worker may connect before the mock accepts; starting the server now
    // proves the entry point returned without waiting for an answer.
    try mock.start();
    defer mock.finish() catch {};
    try mock.waitFor(1);
    worker.?.join();
    worker = null;
    job.thread = null;

    state.client = null;
    actions.finish(job);
    try std.testing.expect(state.actions.job == null);
    try std.testing.expectEqual(@as(usize, 1), mock.count);
    try std.testing.expectEqualStrings("/v1.43/containers/abc/stop", mock.pathAt(0));
    try std.testing.expectEqualStrings("Stopped", state.toast.?.text);
    try std.testing.expect(!state.toast.?.bad);
}

test "teardown abandons the action job without touching state" {
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    const Idle = struct {
        fn run(done: *std.atomic.Value(bool)) void {
            done.store(true, .release);
        }
    };
    var done = std.atomic.Value(bool).init(false);
    const job = try state.gpa.create(Job);
    job.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .action };
    job.thread = try std.Thread.spawn(.{}, Idle.run, .{&done});
    state.actions.job = job;
    state.toast = .{ .text = "before", .bad = false };

    actions.discard(&state);
    try std.testing.expect(job.abandoned.load(.acquire));
    try std.testing.expect(state.actions.job == null);
    try std.testing.expectEqualStrings("before", state.toast.?.text);

    while (!done.load(.acquire)) std.Thread.yield() catch {};
    state.gpa.destroy(job);
}

test "an action completion refreshes with nothing queued" {
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
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/action-refresh.sock");
    state.discovery_test = .{ .scan = Hook.scan, .start_refresh = Hook.refresh };

    const job = try state.gpa.create(Job);
    job.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .action, .ok = true };
    try std.testing.expect(copy(&job.verb, &job.verb_len, "Stopped"));
    state.actions.job = job;

    actions.finish(job);
    try std.testing.expect(state.actions.job == null);
    try std.testing.expect(!state.refresh_wanted);
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);

    // A second resume must not start another pass by itself.
    session.@"resume"(&state);
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);
}

test "an action completion applies a queued engine switch exactly once" {
    const Counter = struct {
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
    Counter.starts = 0;
    var runtimes = [_]discovery.Runtime{.{ .socket = "/nonexistent/switch.sock", .reachable = true }};
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    defer {
        state.client = null;
        if (state.retired) |*old| old.deinit();
        if (state.preferred_socket) |p| state.gpa.free(p);
        if (state.last_engine_id) |p| state.gpa.free(p);
        if (state.last_engine_socket) |p| state.gpa.free(p);
    }
    state.runtimes = &runtimes;
    state.generation = 10;
    // The same path makes rememberSocket return early, so this state-machine
    // test never reaches the settings writer.
    state.preferred_socket = try state.gpa.dupe(u8, runtimes[0].socket);
    state.discovery_test = .{ .scan = Counter.scan, .start_refresh = Counter.refresh };

    const job = try state.gpa.create(Job);
    job.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .action, .ok = true };
    try std.testing.expect(copy(&job.verb, &job.verb_len, "Stopped"));
    state.actions.job = job;
    state.select_wanted = 0;
    // A pending refresh must not run before the switch; its one follow-up is
    // the refresh the switch itself starts.
    state.refresh_wanted = true;

    actions.finish(job);
    try std.testing.expect(state.select_wanted == null);
    try std.testing.expect(!state.refresh_wanted);
    try std.testing.expect(state.client != null);
    try std.testing.expectEqual(@as(u64, 11), state.generation);
    try std.testing.expectEqual(@as(usize, 1), Counter.starts);

    // A second resume must not arm or start a duplicate pass.
    session.@"resume"(&state);
    try std.testing.expectEqual(@as(usize, 1), Counter.starts);
}

test "an action completion runs a queued rescan exactly once" {
    const Scan = struct {
        var scans: usize = 0;
        fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !discovery.Found {
            _ = io;
            _ = preferred;
            scans += 1;
            return .{ .runtimes = try gpa.alloc(discovery.Runtime, 0) };
        }
    };
    Scan.scans = 0;
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    defer if (state.discovery_job) |job| session.done(@ptrCast(job));
    state.discovery_test = .{ .scan = Scan.scan, .dispatch = false };
    state.discover_wanted = true;

    const job = try state.gpa.create(Job);
    job.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .action, .ok = true };
    try std.testing.expect(copy(&job.verb, &job.verb_len, "Stopped"));
    state.actions.job = job;

    actions.finish(job);
    try std.testing.expect(state.discovery_job != null);
    try std.testing.expect(!state.discover_wanted);

    // A second resume must not start a second scan behind the first.
    session.@"resume"(&state);
    try std.testing.expect(state.discovery_job != null);

    session.done(@ptrCast(state.discovery_job.?));
    try std.testing.expect(state.discovery_job == null);
    try std.testing.expectEqual(@as(usize, 1), Scan.scans);
}

test "an engine error becomes the failure toast" {
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

    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    var response_buf: [256]u8 = undefined;
    const body = "{\"message\":\"cannot pause a stopped container\"}";
    const response = try mockResponse(&response_buf, 500, "Internal Server Error", body);
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    const job = try testJob(&state, .action, path);
    try std.testing.expect(copy(&job.method, &job.method_len, "POST"));
    try std.testing.expect(copy(&job.path, &job.path_len, "/containers/abc/pause"));
    try std.testing.expect(copy(&job.verb, &job.verb_len, "Paused"));
    state.actions.job = job;
    var worker: ?std.Thread = try std.Thread.spawn(.{}, actions.run, .{job});
    defer if (worker) |thread| thread.join();
    try mock.waitFor(1);
    worker.?.join();
    worker = null;
    try std.testing.expect(!job.ok);
    // The client unwraps Docker's message envelope, so the diagnosis, not the
    // JSON around it, is what reaches the toast.
    try std.testing.expectEqualStrings("cannot pause a stopped container", job.error_text);
    try mock.finish();

    actions.finish(job);
    try std.testing.expect(state.toast.?.bad);
    try std.testing.expectEqualStrings("cannot pause a stopped container", state.toast.?.text);
}

test "a second mutation is refused while one is running" {
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    const job = try state.gpa.create(Job);
    job.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .action };
    state.actions.job = job;
    defer {
        state.actions.job = null;
        state.gpa.destroy(job);
    }

    try std.testing.expect(!actions.act(&state, "POST", "/containers/abc/stop", "Stopped"));
    try std.testing.expect(state.actions.job == job);
    try std.testing.expect(state.toast.?.bad);
    try std.testing.expectEqualStrings("wait for the current engine operation to finish", state.toast.?.text);

    // The other mutation jobs are the same guard, not separate rules.
    state.actions.job = null;
    var bulk_job: bulk_action.Job = undefined;
    state.bulk.job = &bulk_job;
    try std.testing.expect(!actions.act(&state, "POST", "/containers/abc/start", "Started"));
    try std.testing.expectEqualStrings("wait for the current engine operation to finish", state.toast.?.text);
    state.bulk.job = null;

    var ops_job: command.Job = undefined;
    state.ops.job = &ops_job;
    try std.testing.expect(!actions.act(&state, "POST", "/containers/abc/start", "Started"));
    try std.testing.expectEqualStrings("wait for the current engine operation to finish", state.toast.?.text);
    state.ops.job = null;

    state.image_tasks.run_job = @ptrFromInt(@alignOf(std.Thread));
    try std.testing.expect(state.mutating());
    try std.testing.expect(!actions.act(&state, "POST", "/containers/abc/start", "Started"));
    state.image_tasks.run_job = null;

    // A pull is read-only: it must not stand in the way of an action.
    state.image_tasks.pull_job = @ptrFromInt(@alignOf(std.Thread));
    try std.testing.expect(state.image_tasks.pulling());
    try std.testing.expect(!state.mutating());
    try std.testing.expect(!actions.act(&state, "POST", "/containers/abc/start", "Started"));
    try std.testing.expectEqualStrings("no engine is selected", state.toast.?.text);
    state.image_tasks.pull_job = null;
}

test "a partially failed prune keeps the bytes it reclaimed" {
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

    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    var r0: [160]u8 = undefined;
    var r1: [160]u8 = undefined;
    var r2: [160]u8 = undefined;
    const responses = [_][]const u8{
        try mockResponse(&r0, 200, "OK", "{\"SpaceReclaimed\":100}"),
        try mockResponse(&r1, 500, "Internal Server Error", "{\"message\":\"busy\"}"),
        try mockResponse(&r2, 200, "OK", "{\"SpaceReclaimed\":50}"),
    };
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener, .responses = &responses };
    try mock.start();
    defer mock.finish() catch {};

    const job = try testJob(&state, .prune, path);
    job.steps[0] = .containers;
    job.steps[1] = .images;
    job.steps[2] = .volumes;
    job.step_count = 3;
    state.actions.job = job;
    var worker: ?std.Thread = try std.Thread.spawn(.{}, actions.run, .{job});
    defer if (worker) |thread| thread.join();
    try mock.waitFor(3);
    worker.?.join();
    worker = null;
    try mock.finish();
    try std.testing.expectEqual(@as(u64, 150), job.reclaimed);
    try std.testing.expect(job.failed);
    try std.testing.expectEqual(StepOutcome.done, job.outcomes[0]);
    try std.testing.expectEqual(StepOutcome.failed, job.outcomes[1]);
    try std.testing.expectEqual(StepOutcome.done, job.outcomes[2]);

    actions.finish(job);
    try std.testing.expect(state.toast.?.bad);
    try std.testing.expectEqualStrings("Cleanup freed 150 B, with errors", state.toast.?.text);
}

test "a rejected build-cache step is a skip, not a failure" {
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

    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    var response_buf: [160]u8 = undefined;
    const response = try mockResponse(&response_buf, 404, "Not Found", "{\"message\":\"not found\"}");
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    // Docker offers the build-cache row, so a 404 from an older engine is a
    // skip the completion can name. Podman never runs this step at all.
    const job = try testJob(&state, .prune, path);
    job.steps[0] = .build_cache;
    job.step_count = 1;
    state.actions.job = job;
    var worker: ?std.Thread = try std.Thread.spawn(.{}, actions.run, .{job});
    defer if (worker) |thread| thread.join();
    try mock.waitFor(1);
    worker.?.join();
    worker = null;
    try mock.finish();
    try std.testing.expect(!job.failed);
    try std.testing.expect(job.skipped_build_cache);
    try std.testing.expectEqual(StepOutcome.skipped, job.outcomes[0]);

    actions.finish(job);
    try std.testing.expect(!state.toast.?.bad);
    try std.testing.expectEqualStrings("Freed 0 B · build cache skipped", state.toast.?.text);
}
