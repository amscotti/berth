//! State construction and teardown, split from app.zig so the state machine
//! itself stays inside the 600-line rule.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const refresh = @import("refresh.zig");
const runtimes = @import("runtimes.zig");
const ops = @import("ops.zig");
const logs = @import("logs.zig");
const bulk = @import("bulk.zig");
const image_tasks = @import("image_tasks.zig");

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
    image_tasks.cancel(s);
    if (s.job) |job| refresh.discard(s, job);
    s.clearPending();
    s.prune_open = false;
    logs.clear(s);
    s.log_opts.deinit();
    s.image_tasks.deinit();
    s.create_dialog.deinit();
    s.clearInspect();
    s.search.deinit();
    if (s.retired) |*r| r.deinit();
    s.arena.deinit();
    s.gpa.destroy(s);
}
