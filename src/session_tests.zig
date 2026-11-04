//! Session tests that need a fake worker or a timer callback. The pure choices
//! live beside the code in `session.zig`; these cover the hand-off into the
//! state without touching a real socket or waiting on the app loop.
const std = @import("std");
const app = @import("app.zig");
const actions = @import("actions.zig");
const api = @import("engine/api.zig");
const bulk_action = @import("bulk_action.zig");
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
    refresh_mod.autoTick(&state, state.auto_tick_token);
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

test "repeated resumes arm one automatic tick" {
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
        .auto_refresh = true,
        .discovery_test = .{ .scan = Hook.scan, .start_refresh = Hook.refresh },
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/tick.sock");

    session.@"resume"(&state);
    try std.testing.expect(state.auto_tick_pending);
    session.@"resume"(&state);
    try std.testing.expect(state.auto_tick_pending);
    try std.testing.expectEqual(@as(usize, 0), Hook.starts);

    // The tick clears the flag as it takes over; the pass after it can arm a
    // new one.
    refresh_mod.autoTick(&state, state.auto_tick_token);
    try std.testing.expect(!state.auto_tick_pending);
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);
    session.@"resume"(&state);
    try std.testing.expect(state.auto_tick_pending);
}

test "an auto tick stands down while a mutation is running" {
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
        .auto_refresh = true,
        .discovery_test = .{ .scan = Hook.scan, .start_refresh = Hook.refresh },
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/tick-mutation.sock");

    var action_job: actions.Job = undefined;
    state.actions.job = &action_job;
    state.auto_tick_pending = true;
    refresh_mod.autoTick(&state, state.auto_tick_token);
    try std.testing.expect(!state.auto_tick_pending);
    try std.testing.expectEqual(@as(usize, 0), Hook.starts);
    state.actions.job = null;

    var bulk_job: bulk_action.Job = undefined;
    state.bulk.job = &bulk_job;
    state.auto_tick_pending = true;
    refresh_mod.autoTick(&state, state.auto_tick_token);
    try std.testing.expect(!state.auto_tick_pending);
    try std.testing.expectEqual(@as(usize, 0), Hook.starts);
    state.bulk.job = null;

    state.image_tasks.run_job = @ptrFromInt(@alignOf(std.Thread));
    state.auto_tick_pending = true;
    refresh_mod.autoTick(&state, state.auto_tick_token);
    try std.testing.expect(!state.auto_tick_pending);
    try std.testing.expectEqual(@as(usize, 0), Hook.starts);
    state.image_tasks.run_job = null;

    // With the engine idle again, the next tick runs one refresh.
    session.@"resume"(&state);
    try std.testing.expect(state.auto_tick_pending);
    refresh_mod.autoTick(&state, state.auto_tick_token);
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);
}

test "a new pass invalidates an old auto tick" {
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
        .auto_refresh = true,
        .discovery_test = .{ .scan = Hook.scan, .start_refresh = Hook.refresh },
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/tick-stale.sock");

    session.@"resume"(&state);
    const old_token = state.auto_tick_token;
    try std.testing.expect(state.auto_tick_pending);

    // A mutation-started pass replaces the chain the old tick belonged to.
    session.refresh(&state);
    try std.testing.expect(state.auto_tick_token != old_token);
    try std.testing.expect(!state.auto_tick_pending);
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);

    // Its completion arms a replacement tick at the end of the new chain.
    session.@"resume"(&state);
    try std.testing.expect(state.auto_tick_pending);
    const fresh_token = state.auto_tick_token;

    // The old callback stands down instead of refreshing right behind it.
    refresh_mod.autoTick(&state, old_token);
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);
    try std.testing.expect(state.auto_tick_pending);

    refresh_mod.autoTick(&state, fresh_token);
    try std.testing.expectEqual(@as(usize, 2), Hook.starts);
    try std.testing.expect(!state.auto_tick_pending);
    session.@"resume"(&state);
    try std.testing.expect(state.auto_tick_pending);
}

test "a refresh start allocation failure arms a retry" {
    var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    var state: app.State = .{
        .gpa = failing.allocator(),
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = true,
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/refresh-oom.sock");

    refresh_mod.start(&state);
    try std.testing.expectEqualStrings("out of memory preparing a refresh", state.error_text);
    try std.testing.expect(state.auto_tick_pending);
}

test "an engine switch waits for the action job" {
    var runtimes = [_]discovery.Runtime{.{}};
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .runtimes = &runtimes,
        .generation = 4,
    };
    defer state.arena.deinit();
    var action_job: actions.Job = undefined;
    state.actions.job = &action_job;

    session.selectRuntime(&state, 0);
    try std.testing.expectEqual(@as(?usize, 0), state.select_wanted);
    // Nothing moved yet: no client, no generation bump, no clearData.
    try std.testing.expectEqual(@as(u64, 4), state.generation);
    try std.testing.expect(state.client == null);

    state.actions.job = null;
}

test "a finished scan does not adopt under a running action" {
    const Scan = struct {
        fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !discovery.Found {
            _ = io;
            _ = preferred;
            const runtimes = try gpa.alloc(discovery.Runtime, 1);
            runtimes[0] = .{
                .name = try gpa.dupe(u8, "Dockerd"),
                .kind = .dockerd,
                .dialect = .docker,
                .socket = try gpa.dupe(u8, "/nonexistent/berth-scan-test.sock"),
                .reachable = true,
                .info = .{ .ID = try gpa.dupe(u8, "daemon-scan") },
            };
            return .{ .runtimes = runtimes };
        }
    };

    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
        .discovery_test = .{ .scan = Scan.scan, .dispatch = false },
    };
    defer state.arena.deinit();
    defer {
        if (state.discovery_job) |job| session.done(@ptrCast(job));
    }

    session.discover(&state);
    try std.testing.expect(state.discovery_job != null);
    var action_job: actions.Job = undefined;
    state.actions.job = &action_job;
    session.done(@ptrCast(state.discovery_job.?));

    // The scan result is dropped, not adopted: the old engine stays until the
    // action reports, and a rescan is queued for the refresh that follows.
    try std.testing.expect(state.discovery_job == null);
    try std.testing.expect(state.client == null);
    try std.testing.expectEqual(@as(usize, 0), state.runtimes.len);
    try std.testing.expect(state.discover_wanted);
    state.actions.job = null;
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
