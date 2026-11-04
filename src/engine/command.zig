//! A background command, shaped like the refresh job: the worker owns the
//! child, the main thread owns the state, and the two meet in one callback.
//! Used for engine lifecycle commands whose runtime is minutes, not clicks.
const std = @import("std");
const nanoui = @import("nanoui");

pub const Done = fn (ctx: ?*anyopaque) callconv(.c) void;

pub const Job = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    argv: []const []const u8,
    label: []const u8,
    timeout_seconds: i64,
    ctx: ?*anyopaque,
    done: *const Done,
    output: []const u8 = "",
    ok: bool = false,
    thread: ?std.Thread = null,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

pub fn start(
    gpa: std.mem.Allocator,
    io: std.Io,
    argv: []const []const u8,
    label: []const u8,
    timeout_seconds: i64,
    ctx: ?*anyopaque,
    done: *const Done,
) !*Job {
    const job = try gpa.create(Job);
    errdefer gpa.destroy(job);
    const args = try gpa.alloc([]const u8, argv.len);
    errdefer gpa.free(args);
    var copied: usize = 0;
    // Only the strings copied so far, so a failure while duping frees neither
    // uninitialized slots nor the array itself (the errdefer above owns it).
    errdefer for (args[0..copied]) |arg| gpa.free(arg);
    for (argv, 0..) |arg, i| {
        args[i] = try gpa.dupe(u8, arg);
        copied += 1;
    }
    // Copy the label before publishing the job so every later failure is
    // cleaned up by exactly one errdefer chain, including the thread spawn.
    const label_copy = try gpa.dupe(u8, label);
    errdefer gpa.free(label_copy);
    job.* = .{
        .gpa = gpa,
        .io = io,
        .argv = args,
        .label = label_copy,
        .timeout_seconds = timeout_seconds,
        .ctx = ctx,
        .done = done,
    };
    job.thread = try std.Thread.spawn(.{}, work, .{job});
    return job;
}

fn work(job: *Job) void {
    const page = std.heap.page_allocator;
    const result = std.process.run(page, job.io, .{
        .argv = job.argv,
        .stdout_limit = .limited(1024 * 1024),
        .stderr_limit = .limited(1024 * 1024),
        .timeout = .{ .duration = .{
            .raw = .fromSeconds(@max(1, job.timeout_seconds)),
            .clock = .real,
        } },
    }) catch |err| {
        const why = switch (err) {
            error.Timeout => "timed out",
            error.FileNotFound, error.AccessDenied => "command not found",
            else => "could not start the command",
        };
        job.output = message(page, why);
        postIfLive(job);
        return;
    };
    const stdout = result.stdout;
    const stderr = result.stderr;
    defer page.free(stdout);
    defer page.free(stderr);
    job.ok = result.term.success();
    const chosen = if (stderr.len > 0) stderr else stdout;
    // The result buffers are freed below; the job keeps its own copy for the
    // callback that runs after this worker has returned.
    job.output = page.dupe(u8, chosen) catch "";
    postIfLive(job);
}

fn postIfLive(job: *Job) void {
    if (job.canceled.load(.acquire)) return;
    nanoui.App.after(0, job, finishBridge);
}

fn finishBridge(ctx: ?*anyopaque) callconv(.c) void {
    const job: *Job = @ptrCast(@alignCast(ctx.?));
    job.done(job.ctx);
}

fn message(page: std.mem.Allocator, text: []const u8) []const u8 {
    return page.dupe(u8, text) catch "";
}

/// Joins a finished worker. Call from the done callback before reading.
pub fn settle(job: *Job) void {
    if (job.thread) |thread| thread.join();
    job.thread = null;
}

/// Shutdown only: mark the job dead, detach the worker, and deliberately
/// leave its memory behind. Joining here is what made quitting wait for a
/// five-minute `colima start`; the process is on its way out regardless.
pub fn discard(job: *Job) void {
    job.canceled.store(true, .release);
    if (job.thread) |thread| thread.detach();
}

pub fn destroy(job: *Job) void {
    if (job.output.len > 0) std.heap.page_allocator.free(job.output);
    for (job.argv) |arg| job.gpa.free(arg);
    job.gpa.free(job.argv);
    job.gpa.free(job.label);
    job.gpa.destroy(job);
}

fn noopDone(_: ?*anyopaque) callconv(.c) void {}

test "a failed label copy releases every allocation once" {
    // A path that cannot do anything: if the allocation index ever drifts,
    // the worst case is a missing binary instead of a real engine command.
    const argv = [_][]const u8{"/nonexistent/berth-test"};
    var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 3 });
    try std.testing.expectError(
        error.OutOfMemory,
        start(failing.allocator(), std.testing.io, argv[0..], "berth-test label", 60, null, noopDone),
    );
    // Three successful allocations (job, argv, one argument string) pin the
    // fourth as the label, which fails and is then released with the rest.
    try std.testing.expectEqual(@as(usize, 3), failing.allocations);
    try std.testing.expectEqual(failing.allocations, failing.deallocations);
}
