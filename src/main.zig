//! berth: a native monitor for container runtimes — Docker, Podman, Colima
//! and Lima — on macOS and Linux, built with nanoui on Zig 0.17.
const std = @import("std");
const nanoui = @import("nanoui");
const app_mod = @import("app.zig");

extern "c" fn getenv(name: [*:0]const u8) ?[*:0]u8;

pub fn main(init: std.process.Init) !void {
    const gpa = init.gpa;
    const app = try nanoui.App.create(init, .{ .name = "berth" });
    defer app.destroy();

    const state = try app_mod.State.create(gpa, init.io, app);
    defer state.destroy();

    const window = try app.newWindow(.{
        .title = "berth",
        .width = 1180,
        .height = 760,
        .min_width = 960,
        .min_height = 600,
        .content = .{ .view = .init(state, app_mod.State.view) },
    });
    state.window = window;

    // `BERTH_TAB=images` opens that view: the headless snapshot per view that
    // the design contract asks for needs a way to reach each one.
    if (getenv("BERTH_TAB")) |p| {
        state.tab = app_mod.tabFromName(std.mem.span(p)) orelse state.tab;
    }

    // Find an engine and read it before the first frame, so the window opens
    // with data rather than a skeleton that fills in on its own.
    state.discover();
    try app.run();
}
