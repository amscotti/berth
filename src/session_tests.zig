//! Session tests that need a fake worker or a timer callback. The pure choices
//! live beside the code in `session.zig`; these cover the hand-off into the
//! state without touching a real socket or waiting on the app loop.
const std = @import("std");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const discovery = @import("engine/discovery.zig");
const refresh_mod = @import("refresh.zig");
const runtimes_mod = @import("runtimes.zig");
const session = @import("session.zig");

test "a discovery request during a scan queues one rescan" {
    var job: session.Job = undefined;
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .discovery_job = &job,
    };
    defer state.arena.deinit();

    // Forget and Connect call discover after changing the preference; during a
    // scan it must queue, not block or replace the running job.
    session.discover(&state);
    try std.testing.expect(state.discover_wanted);
    try std.testing.expect(state.discovery_job == &job);
}

test "timers do not arm a scan while one is running" {
    var job: session.Job = undefined;
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .discovery_job = &job,
        .retry_pending = true,
    };
    defer state.arena.deinit();

    // The scan's completion owns the next step; neither timer may queue a
    // redundant scan behind it.
    session.retryTick(&state);
    try std.testing.expect(!state.retry_pending);
    try std.testing.expect(!state.discover_wanted);
    refresh_mod.autoTick(&state);
    try std.testing.expect(!state.discover_wanted);
}

test "a successful scan adopts one runtime and starts one refresh" {
    const One = struct {
        var scans: usize = 0;
        var refreshes: usize = 0;

        fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !discovery.Found {
            _ = io;
            _ = preferred;
            scans += 1;
            const runtimes = try gpa.alloc(discovery.Runtime, 1);
            runtimes[0] = .{
                .name = try gpa.dupe(u8, "Dockerd"),
                .kind = .dockerd,
                .dialect = .docker,
                .socket = try gpa.dupe(u8, "/nonexistent/berth-test.sock"),
                .reachable = true,
                .info = .{ .ID = try gpa.dupe(u8, "daemon-one") },
            };
            return .{ .runtimes = runtimes };
        }

        fn refresh(_: *app.State) void {
            refreshes += 1;
        }
    };
    One.scans = 0;
    One.refreshes = 0;

    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
        .discovery_test = .{ .scan = One.scan, .start_refresh = One.refresh },
    };
    defer {
        if (state.discovery_job) |job| session.done(@ptrCast(job));
        state.client = null;
        if (state.last_engine_id) |p| state.gpa.free(p);
        state.last_engine_id = null;
        if (state.last_engine_socket) |p| state.gpa.free(p);
        state.last_engine_socket = null;
        runtimes_mod.drop(&state);
        state.arena.deinit();
    }

    session.discover(&state);
    try std.testing.expect(state.discovery_job != null);
    session.done(@ptrCast(state.discovery_job.?));

    try std.testing.expectEqual(@as(usize, 1), One.scans);
    try std.testing.expectEqual(@as(usize, 1), One.refreshes);
    try std.testing.expectEqual(@as(usize, 0), state.active);
    try std.testing.expectEqual(@as(usize, 1), state.runtimes.len);
    try std.testing.expect(state.client != null);
    try std.testing.expectEqualStrings("/nonexistent/berth-test.sock", state.client.?.socket);
    try std.testing.expect(state.discovery_job == null);
    try std.testing.expect(!state.discover_wanted);
}

test "a failed scan reports its error and drops the old client" {
    const Failing = struct {
        fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !discovery.Found {
            _ = gpa;
            _ = io;
            _ = preferred;
            return error.ScanFailed;
        }
    };

    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = true,
        .discovery_test = .{ .scan = Failing.scan },
    };
    defer state.arena.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/old.sock");
    state.error_text = "previous";

    session.discover(&state);
    try std.testing.expect(state.discovery_job != null);
    session.done(@ptrCast(state.discovery_job.?));

    try std.testing.expect(state.client == null);
    try std.testing.expectEqual(@as(usize, 0), state.runtimes.len);
    try std.testing.expectEqualStrings("could not scan for container engines", state.error_text);
    try std.testing.expect(state.discovery_job == null);
    // One failure arms one retry; the timer itself is not run here.
    try std.testing.expect(state.retry_pending);
    try std.testing.expect(!state.discover_wanted);
}
