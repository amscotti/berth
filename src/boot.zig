//! State construction and teardown, split from app.zig so the state machine
//! itself stays inside the 600-line rule.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const refresh = @import("refresh.zig");

pub fn create(gpa: std.mem.Allocator, io: std.Io, nanoapp: *nanoui.App) !*app.State {
    const s = try gpa.create(app.State);
    s.* = .{ .gpa = gpa, .io = io, .app = nanoapp, .arena = .init(gpa) };
    return s;
}

pub fn destroy(s: *app.State) void {
    for (s.runtimes) |r| {
        s.gpa.free(r.name);
        s.gpa.free(r.socket);
        if (r.error_text.len > 0) s.gpa.free(r.error_text);
    }
    if (s.runtimes.len > 0) s.gpa.free(s.runtimes);
    if (s.job) |job| refresh.discard(job);
    s.clearLogs();
    if (s.retired) |*r| r.deinit();
    s.arena.deinit();
    s.gpa.destroy(s);
}
