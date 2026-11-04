//! The detail worker against a mock engine: what it fetches, and what the
//! completion is allowed to apply. The tests join every worker before calling
//! the completion, so there is no sleep or queue timing involved.
const std = @import("std");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const detail_job = @import("detail_job.zig");
const lifetime = @import("engine/lifetime.zig");
const logs = @import("logs.zig");
const format = @import("ui/format.zig");

const MockEngine = @import("engine/test_server.zig").MockEngine;
const Job = detail_job.Job;

const TestEnv = struct {
    tmp: std.testing.TmpDir,
    listener: std.Io.net.Server,
    pathbuf: [320]u8 = undefined,
    path_len: usize = 0,

    fn init() !TestEnv {
        var tmp = std.testing.tmpDir(.{});
        errdefer tmp.cleanup();
        var env: TestEnv = .{ .tmp = tmp, .listener = undefined };
        var dirbuf: [256]u8 = undefined;
        const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
        const socket_path = try std.fmt.bufPrint(&env.pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});
        env.path_len = socket_path.len;
        var addr = try std.Io.net.UnixAddress.init(socket_path);
        env.listener = try addr.listen(std.testing.io, .{});
        return env;
    }

    fn path(self: *const TestEnv) []const u8 {
        return self.pathbuf[0..self.path_len];
    }

    fn deinit(self: *TestEnv) void {
        self.listener.deinit(std.testing.io);
        self.tmp.cleanup();
    }
};

fn testState() app.State {
    return .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
    };
}

fn cleanup(s: *app.State) void {
    if (s.detail_job) |job| {
        join(job);
        detail_job.finish(job);
    }
    s.clearInspect();
    logs.clear(s);
    s.arena.deinit();
}

fn mockResponse(buf: []u8, status: u16, reason: []const u8, body: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "HTTP/1.1 {d} {s}\r\nContent-Length: {d}\r\nConnection: close\r\n\r\n{s}", .{ status, reason, body.len, body });
}

fn copyFixed(target: []u8, len: *usize, text: []const u8) bool {
    if (text.len > target.len) return false;
    @memcpy(target[0..text.len], text);
    len.* = text.len;
    return true;
}

fn makeJob(s: *app.State, path: []const u8, id: []const u8) !*Job {
    const job = try s.gpa.create(Job);
    job.* = .{
        .state = s,
        .gpa = s.gpa,
        .io = s.io,
        .dispatch = false,
        .generation = s.detail_generation,
    };
    if (!copyFixed(&job.socket, &job.socket_len, path) or
        !copyFixed(&job.api_version, &job.api_len, "v1.43") or
        !copyFixed(&job.id, &job.id_len, id))
    {
        s.gpa.destroy(job);
        return error.PathTooLong;
    }
    return job;
}

fn join(job: *Job) void {
    if (job.thread) |thread| {
        thread.join();
        job.thread = null;
    }
}

/// Runs the worker on its own thread, waits for the expected number of mock
/// requests, then joins. The wait has a spin bound, so a broken worker fails
/// the test instead of hanging it.
fn runToCompletion(job: *Job, mock: *MockEngine, requests: usize) !void {
    var worker: ?std.Thread = try std.Thread.spawn(.{}, detail_job.run, .{job});
    defer if (worker) |thread| thread.join();
    try mock.waitFor(requests);
    worker.?.join();
    worker = null;
}

test "a detail read applies lifetime and logs and clears loading" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [512]u8 = undefined;
    var r1: [256]u8 = undefined;
    const responses = [_][]const u8{
        try mockResponse(&r0, 200, "OK", "{\"Name\":\"/web\",\"Config\":{\"Image\":\"nginx:alpine\"},\"RestartCount\":3,\"State\":{\"StartedAt\":\"2026-09-22T18:13:20Z\"}}"),
        try mockResponse(&r1, 200, "OK", "line one\nline two"),
    };
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .responses = &responses };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "abc";
    state.lifetime_loading = true;
    state.logs_loading = true;
    state.detail_generation = 4;
    state.detail_lifetime = .{
        .restarts = 1,
        .run_cmd = try std.heap.page_allocator.dupe(u8, "old command"),
    };
    state.inspect_buf = try std.heap.page_allocator.dupe(u8, "old json");

    const job = try makeJob(&state, env.path(), "abc");
    job.fetch_lifetime = true;
    job.fetch_logs = true;
    job.generation = 4;
    state.detail_job = job;

    try runToCompletion(job, &mock, 2);
    try mock.finish();
    detail_job.finish(job);

    try std.testing.expect(state.detail_job == null);
    try std.testing.expect(!state.lifetime_loading);
    try std.testing.expect(!state.logs_loading);
    try std.testing.expect(state.detail_lifetime != null);
    try std.testing.expectEqual(@as(u32, 3), state.detail_lifetime.?.restarts);
    try std.testing.expectEqualStrings("docker run -d --name web nginx:alpine", state.detail_lifetime.?.run_cmd.?);
    try std.testing.expectEqualStrings("line one\nline two", state.log_text);
    // The raw JSON belongs to the Inspect tab and was not part of this read.
    try std.testing.expectEqualStrings("old json", state.inspect_buf.?);
}

test "selecting a container starts the read and clears loading when it lands" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [512]u8 = undefined;
    var r1: [256]u8 = undefined;
    const responses = [_][]const u8{
        try mockResponse(&r0, 200, "OK", "{\"Name\":\"/web\",\"Config\":{\"Image\":\"nginx:alpine\"},\"RestartCount\":2,\"State\":{\"StartedAt\":\"2026-09-22T18:13:20Z\"}}"),
        try mockResponse(&r1, 200, "OK", "hello"),
    };
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .responses = &responses };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_dispatch = false;
    state.client = api.Client.init(state.io, state.gpa, env.path());
    var ct = api.Container{ .Id = "abc", .State = "running", .Status = "Up 2 hours" };

    state.select(&ct);
    try std.testing.expect(state.lifetime_loading);
    try std.testing.expect(state.logs_loading);
    try std.testing.expect(state.detail_job != null);
    const job = state.detail_job.?;
    try std.testing.expect(job.fetch_lifetime);
    try std.testing.expect(job.fetch_logs);
    try std.testing.expect(!job.fetch_inspect);

    try mock.waitFor(2);
    join(job);
    detail_job.finish(job);

    try std.testing.expect(!state.lifetime_loading);
    try std.testing.expect(!state.logs_loading);
    try std.testing.expectEqual(@as(u32, 2), state.detail_lifetime.?.restarts);
    try std.testing.expectEqualStrings("hello", state.log_text);
}

test "a job for the old selection cannot apply after another row is selected" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [512]u8 = undefined;
    var r1: [256]u8 = undefined;
    const responses = [_][]const u8{
        try mockResponse(&r0, 200, "OK", "{\"Name\":\"/a\",\"Config\":{\"Image\":\"alpine\"},\"RestartCount\":9,\"State\":{\"StartedAt\":\"2026-09-22T18:13:20Z\"}}"),
        try mockResponse(&r1, 200, "OK", "a's logs"),
    };
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .responses = &responses };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "aaa";
    state.detail_generation = 10;

    const job = try makeJob(&state, env.path(), "aaa");
    job.fetch_lifetime = true;
    job.fetch_logs = true;
    job.generation = 10;
    state.detail_job = job;
    try runToCompletion(job, &mock, 2);
    try mock.finish();

    // Another row was selected while the worker was still reading; the new
    // selection bumps the generation and owns the pane.
    state.selected = "bbb";
    state.detail_generation += 1;
    detail_job.finish(job);

    try std.testing.expectEqualStrings("bbb", state.selected.?);
    try std.testing.expect(state.detail_lifetime == null);
    try std.testing.expect(state.log_buf == null);
    try std.testing.expectEqualStrings("", state.log_text);
}

test "a new selection supersedes the read already in flight" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [512]u8 = undefined;
    var r1: [256]u8 = undefined;
    const responses = [_][]const u8{
        try mockResponse(&r0, 200, "OK", "{\"Name\":\"/b\",\"Config\":{\"Image\":\"alpine\"},\"RestartCount\":1,\"State\":{\"StartedAt\":\"2026-09-22T18:13:20Z\"}}"),
        try mockResponse(&r1, 200, "OK", "b's logs"),
    };
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .responses = &responses };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_dispatch = false;
    state.detail_kind = .container;
    state.selected = "aaa";
    state.client = api.Client.init(state.io, state.gpa, env.path());

    const old = try makeJob(&state, env.path(), "aaa");
    old.fetch_logs = true;
    state.detail_job = old;
    old.thread = try std.Thread.spawn(.{}, struct {
        fn idle() void {}
    }.idle, .{});

    var ct = api.Container{ .Id = "bbb", .State = "running", .Status = "Up 1 hour" };
    state.select(&ct);
    try std.testing.expect(state.detail_job != null);
    try std.testing.expect(state.detail_job.? != old);
    try std.testing.expect(old.abandoned.load(.acquire));
    // The old worker's only remaining job is to release its own record.
    detail_job.finish(old);

    const job = state.detail_job.?;
    try mock.waitFor(2);
    join(job);
    detail_job.finish(job);

    try std.testing.expectEqualStrings("bbb", state.selected.?);
    try std.testing.expectEqualStrings("b's logs", state.log_text);
}

test "log reload is logs-only and refuses to stack a second job" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [256]u8 = undefined;
    const response = try mockResponse(&r0, 200, "OK", "fresh logs");
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_dispatch = false;
    state.detail_kind = .container;
    state.selected = "abc";
    state.client = api.Client.init(state.io, state.gpa, env.path());
    state.log_buf = try std.heap.page_allocator.dupe(u8, "stale logs");
    state.log_text = state.log_buf.?;
    state.detail_lifetime = .{
        .restarts = 5,
        .run_cmd = try std.heap.page_allocator.dupe(u8, "kept command"),
    };
    state.inspect_buf = try std.heap.page_allocator.dupe(u8, "kept json");

    logs.reload(&state);
    const job = state.detail_job orelse return error.NoJob;
    try std.testing.expect(job.fetch_logs);
    try std.testing.expect(!job.fetch_lifetime);
    try std.testing.expect(!job.fetch_inspect);
    // A second Refresh during the read does not replace the job or its flags.
    logs.reload(&state);
    try std.testing.expect(state.detail_job == job);

    try mock.waitFor(1);
    join(job);
    detail_job.finish(job);

    try std.testing.expectEqualStrings("fresh logs", state.log_text);
    try std.testing.expect(!state.lifetime_loading);
    try std.testing.expect(!state.logs_loading);
    try std.testing.expectEqual(@as(u32, 5), state.detail_lifetime.?.restarts);
    try std.testing.expectEqualStrings("kept command", state.detail_lifetime.?.run_cmd.?);
    try std.testing.expectEqualStrings("kept json", state.inspect_buf.?);
}

test "a Times toggle during a read re-fetches with the new clock" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [256]u8 = undefined;
    const response = try mockResponse(&r0, 200, "OK", "2026-09-22T18:13:20.500Z switched");
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "abc";
    state.client = api.Client.init(state.io, state.gpa, env.path());
    state.logs_loading = true;

    const job = try makeJob(&state, env.path(), "abc");
    job.fetch_logs = true;
    job.timestamps = false;
    job.log_ok = true;
    job.log_text = try std.heap.page_allocator.dupe(u8, "old clock");
    state.detail_job = job;

    state.log_opts.timestamps = true;
    logs.reload(&state); // refused: remember the clock the pane now wants
    try std.testing.expect(state.logs_wanted);

    detail_job.finish(job); // drops the stale text and starts the retry
    const retry = state.detail_job orelse return error.NoJob;
    try std.testing.expect(retry.fetch_logs);
    try std.testing.expect(retry.timestamps);
    try std.testing.expect(!state.logs_wanted);

    try mock.waitFor(1);
    join(retry);
    detail_job.finish(retry);

    const unix = lifetime.rfc3339("2026-09-22T18:13:20Z").?;
    var clock_buf: [16]u8 = undefined;
    const clock = format.clockBuf(&clock_buf, unix);
    try std.testing.expect(std.mem.startsWith(u8, state.log_text, clock));
    try std.testing.expect(std.mem.indexOf(u8, state.log_text, "switched") != null);
    try std.testing.expect(std.mem.indexOf(u8, state.log_text, "old clock") == null);
}

test "inspect reload fills the raw pane and clears its loading flag" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [256]u8 = undefined;
    const response = try mockResponse(&r0, 200, "OK", "{\"Id\":\"abc\",\"State\":{\"Running\":true}}");
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_dispatch = false;
    state.detail_kind = .container;
    state.selected = "abc";
    state.selected_name = "web";
    state.client = api.Client.init(state.io, state.gpa, env.path());
    state.inspect_buf = try std.heap.page_allocator.dupe(u8, "stale json");

    state.reloadInspect();
    const job = state.detail_job orelse return error.NoJob;
    try std.testing.expect(job.fetch_inspect);
    try std.testing.expect(!job.fetch_lifetime);
    try std.testing.expect(state.inspect_loading);
    try std.testing.expect(state.inspect_buf == null);

    try mock.waitFor(1);
    join(job);
    detail_job.finish(job);

    try std.testing.expect(!state.inspect_loading);
    try std.testing.expect(state.inspect_buf != null);
    try std.testing.expect(std.mem.indexOf(u8, state.inspect_buf.?, "\"Id\": \"abc\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, state.inspect_buf.?, "\"Running\": true") != null);
}

test "opening Inspect during the selection read retries exactly once" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [256]u8 = undefined;
    const response = try mockResponse(&r0, 200, "OK", "{\"Id\":\"abc\"}");
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "abc";
    state.selected_name = "web";
    state.pane_inspect = true;
    state.client = api.Client.init(state.io, state.gpa, env.path());

    const job = try makeJob(&state, env.path(), "abc");
    job.fetch_lifetime = true;
    job.lifetime_ok = true;
    job.fetch_logs = true;
    job.log_ok = true;
    job.log_text = try std.heap.page_allocator.dupe(u8, "logs");
    state.detail_job = job;

    state.reloadInspect(); // refused: the selection read owns the slot
    try std.testing.expect(state.inspect_wanted);
    try std.testing.expect(state.detail_job == job);

    detail_job.finish(job); // consumes the want and starts the JSON read
    const retry = state.detail_job orelse return error.NoJob;
    try std.testing.expect(retry.fetch_inspect);
    try std.testing.expect(!state.inspect_wanted);
    try std.testing.expect(state.inspect_loading);

    try mock.waitFor(1);
    join(retry);
    detail_job.finish(retry);

    try std.testing.expect(!state.inspect_loading);
    try std.testing.expect(state.inspect_buf != null);
    try std.testing.expect(std.mem.indexOf(u8, state.inspect_buf.?, "\"Id\": \"abc\"") != null);
    // One-shot: the retry completion does not queue another.
    try std.testing.expect(state.detail_job == null);
    try std.testing.expect(!state.inspect_wanted);
}

test "a result is dropped by an id mismatch alone or a generation mismatch alone" {
    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.detail_generation = 50;

    const by_id = try makeJob(&state, "/nonexistent/id.sock", "aaa");
    by_id.fetch_logs = true;
    by_id.log_ok = true;
    by_id.log_text = try std.heap.page_allocator.dupe(u8, "wrong id");
    state.detail_job = by_id;
    state.selected = "bbb"; // generation still matches
    detail_job.finish(by_id);
    try std.testing.expect(state.log_buf == null);

    state.selected = "aaa";
    state.detail_generation = 51;
    const by_generation = try makeJob(&state, "/nonexistent/generation.sock", "aaa");
    by_generation.fetch_logs = true;
    by_generation.generation = 50; // id still matches
    by_generation.log_ok = true;
    by_generation.log_text = try std.heap.page_allocator.dupe(u8, "wrong generation");
    state.detail_job = by_generation;
    detail_job.finish(by_generation);
    try std.testing.expect(state.log_buf == null);
    try std.testing.expectEqualStrings("", state.log_text);
}

test "a failed log read keeps the synchronous error message" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [256]u8 = undefined;
    const response = try mockResponse(&r0, 500, "Internal Server Error", "{\"message\":\"engine exploded\"}");
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "abc";
    state.log_buf = try std.heap.page_allocator.dupe(u8, "old logs");
    state.log_text = state.log_buf.?;

    const job = try makeJob(&state, env.path(), "abc");
    job.fetch_logs = true;
    state.detail_job = job;
    try runToCompletion(job, &mock, 1);
    try mock.finish();
    detail_job.finish(job);

    try std.testing.expect(state.log_buf == null);
    try std.testing.expectEqualStrings("could not read this container's logs", state.log_text);
    try std.testing.expect(state.toast == null);
}

test "a failed inspect leaves lifetime absent without blocking the logs" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [256]u8 = undefined;
    var r1: [256]u8 = undefined;
    const responses = [_][]const u8{
        try mockResponse(&r0, 404, "Not Found", "{\"message\":\"no such container\"}"),
        try mockResponse(&r1, 200, "OK", "logs still arrive"),
    };
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .responses = &responses };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "abc";

    const job = try makeJob(&state, env.path(), "abc");
    job.fetch_lifetime = true;
    job.fetch_logs = true;
    state.detail_job = job;
    try runToCompletion(job, &mock, 2);
    try mock.finish();
    detail_job.finish(job);

    try std.testing.expect(state.detail_lifetime == null);
    try std.testing.expectEqualStrings("logs still arrive", state.log_text);
    try std.testing.expect(state.toast == null);
}

test "cancel detaches the read and a queued completion leaves the state alone" {
    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "abc";
    state.logs_loading = true;

    const job = try makeJob(&state, "/nonexistent/berth.sock", "abc");
    state.detail_job = job;
    job.thread = try std.Thread.spawn(.{}, struct {
        fn idle() void {}
    }.idle, .{});

    detail_job.cancel(&state);
    try std.testing.expect(state.detail_job == null);
    try std.testing.expect(job.abandoned.load(.acquire));
    try std.testing.expect(job.detached.load(.acquire));

    // The completion that was already queued (or a test's own late call) only
    // releases the job; the selection and loading flag are the state's.
    detail_job.finish(job);
    try std.testing.expectEqualStrings("abc", state.selected.?);
    try std.testing.expect(state.logs_loading);
}

test "timestamps are localized before the log text reaches the state" {
    var env = try TestEnv.init();
    defer env.deinit();
    var r0: [256]u8 = undefined;
    const response = try mockResponse(&r0, 200, "OK", "2026-09-22T18:13:20.123456789Z hello\nsecond line");
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &env.listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    var state = testState();
    defer cleanup(&state);
    state.detail_kind = .container;
    state.selected = "abc";
    state.log_opts.timestamps = true;

    const job = try makeJob(&state, env.path(), "abc");
    job.fetch_logs = true;
    job.timestamps = true;
    state.detail_job = job;
    try runToCompletion(job, &mock, 1);
    try mock.finish();
    detail_job.finish(job);

    const unix = lifetime.rfc3339("2026-09-22T18:13:20Z").?;
    var clock_buf: [16]u8 = undefined;
    const clock = format.clockBuf(&clock_buf, unix);
    try std.testing.expect(std.mem.startsWith(u8, state.log_text, clock));
    try std.testing.expect(std.mem.indexOf(u8, state.log_text, "hello") != null);
    try std.testing.expect(std.mem.indexOf(u8, state.log_text, "2026-09-22T") == null);
    try std.testing.expect(std.mem.indexOf(u8, state.log_text, "second line") != null);
}
