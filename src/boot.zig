//! State construction and teardown, split from app.zig so the state machine
//! itself stays inside the 600-line rule.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const refresh = @import("refresh.zig");
const auto_tick = @import("auto_tick.zig");
const runtimes = @import("runtimes.zig");
const ops = @import("ops.zig");
const logs = @import("logs.zig");
const bulk = @import("bulk.zig");
const actions = @import("actions.zig");
const image_tasks = @import("image_tasks.zig");
const detail_job = @import("detail_job.zig");

pub fn create(gpa: std.mem.Allocator, io: std.Io, nanoapp: *nanoui.App) !*app.State {
    const s = try gpa.create(app.State);
    s.* = .{ .gpa = gpa, .io = io, .app = nanoapp, .arena = .init(gpa) };
    return s;
}

pub fn destroy(s: *app.State) void {
    s.closing = true;
    // Close while the state still exists, so the close callback can read it;
    // App.destroy would otherwise close after boot.destroy has freed it.
    if (s.window) |window| window.close();
    s.window = null;
    // A scan can be stuck in a connect, which has no deadline; do not join it
    // at exit. `done` checks the flag before touching state, but the worker
    // may still be inside the io/allocator teardown as the process exits. That
    // exit-only race is accepted, like other detached jobs; joining could hang
    // the quit. If the callback never runs, the job leaks at exit.
    if (s.discovery_job) |job| {
        s.discovery_job = null;
        job.abandoned.store(true, .release);
    }
    runtimes.drop(s);
    if (s.preferred_socket) |p| s.gpa.free(p);
    if (s.last_engine_id) |p| s.gpa.free(p);
    if (s.last_engine_socket) |p| s.gpa.free(p);
    s.socket_input.deinit();
    ops.cancel(s);
    bulk.discard(s);
    actions.discard(s);
    image_tasks.cancel(s);
    if (s.job) |job| refresh.discard(s, job);
    auto_tick.discard(s);
    detail_job.cancel(s);
    s.clearPending();
    s.prune_open = false;
    logs.clear(s);
    s.log_opts.deinit();
    s.image_tasks.deinit();
    s.create_dialog.deinit();
    s.clearInspect();
    s.search.deinit();
    s.df.reset();
    if (s.retired) |*r| r.deinit();
    s.arena.deinit();
    s.gpa.destroy(s);
}

test "teardown frees the df cache arena" {
    const gpa = std.testing.allocator;
    const s = try gpa.create(app.State);
    s.* = .{
        .gpa = gpa,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(gpa),
    };
    var images = [_]api.Image{.{ .Id = "sha256:teardown", .Size = 7 }};
    try std.testing.expect(s.df.adopt(gpa, .{ .Images = &images }, 1));
    // The testing allocator fails this test if either the state arena or the
    // df cache's own arena survives teardown.
    destroy(s);
}
