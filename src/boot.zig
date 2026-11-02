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

pub fn create(gpa: std.mem.Allocator, io: std.Io, nanoapp: *nanoui.App) !*app.State {
    const s = try gpa.create(app.State);
    s.* = .{ .gpa = gpa, .io = io, .app = nanoapp, .arena = .init(gpa) };
    return s;
}

pub fn destroy(s: *app.State) void {
    runtimes.drop(s);
    if (s.preferred_socket) |p| s.gpa.free(p);
    if (s.last_engine_id) |p| s.gpa.free(p);
    if (s.last_engine_socket) |p| s.gpa.free(p);
    s.socket_input.deinit();
    ops.cancel(s);
    bulk.cancel(s);
    if (s.job) |job| refresh.discard(job);
    s.clearPending();
    s.prune_open = false;
    logs.clear(s);
    s.log_opts.deinit();
    s.clearInspect();
    s.search.deinit();
    if (s.retired) |*r| r.deinit();
    s.arena.deinit();
    s.gpa.destroy(s);
}
