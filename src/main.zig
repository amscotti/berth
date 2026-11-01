//! berth: a native monitor for container runtimes — Docker, Podman, Colima
//! and Lima — on macOS and Linux, built with nanoui on Zig 0.17.
const std = @import("std");
const nanoui = @import("nanoui");
const app_mod = @import("app.zig");
const settings = @import("settings.zig");

extern "c" fn getenv(name: [*:0]const u8) ?[*:0]u8;

/// Opens the window again with the current theme. A new window is created
/// before the old one closes, so the app never sees its last window go.
fn reopen(state: *app_mod.State) void {
    const next = state.app.newWindow(.{
        .title = "berth",
        .width = 1180,
        .height = 760,
        .min_width = 960,
        .min_height = 600,
        .theme = state.themeFor(),
        .content = .{ .view = .init(state, app_mod.State.view) },
    }) catch return;
    if (state.window) |old| old.close();
    state.window = next;
    next.redraw();
}

pub fn main(init: std.process.Init) !void {
    const gpa = init.gpa;
    const app = try nanoui.App.create(init, .{ .name = "berth" });
    defer app.destroy();

    const state = try app_mod.State.create(gpa, init.io, app);
    defer state.destroy();

    // Preferences first: the appearance decides the theme the window opens
    // with, and the theme is fixed at creation.
    state.appearance = settings.load(gpa, init.io).appearance;
    // An explicit override, for screenshots and for anyone whose desktop does
    // not report a theme. It wins over the saved preference for this run only.
    if (getenv("BERTH_THEME")) |p| {
        const name = std.mem.span(p);
        if (std.ascii.eqlIgnoreCase(name, "dark")) state.appearance = .dark;
        if (std.ascii.eqlIgnoreCase(name, "light")) state.appearance = .light;
    }
    state.recreate = &reopen;

    state.window = try app.newWindow(.{
        .title = "berth",
        .width = 1180,
        .height = 760,
        .min_width = 960,
        .min_height = 600,
        .theme = state.themeFor(),
        .content = .{ .view = .init(state, app_mod.State.view) },
    });

    // `BERTH_TAB=images` opens that view: the headless snapshot per view that
    // the design contract asks for needs a way to reach each one.
    if (getenv("BERTH_TAB")) |p| {
        state.tab = app_mod.tabFromName(std.mem.span(p)) orelse state.tab;
    }
    // `BERTH_SELECT=<name>` opens that container's detail pane once the list
    // arrives, so the pane can be snapshotted without a mouse.
    if (getenv("BERTH_SELECT")) |p| {
        state.pending_select = std.mem.span(p);
    }
    // `BERTH_ACT=stop:<name>` runs an action once the list arrives, through the
    // same code path a click uses.
    if (getenv("BERTH_FILTER")) |p| {
        const name = std.mem.span(p);
        if (std.ascii.eqlIgnoreCase(name, "running")) state.filter = .running;
        if (std.ascii.eqlIgnoreCase(name, "stopped")) state.filter = .stopped;
    }
    if (getenv("BERTH_ACT")) |p| {
        state.pending_act = std.mem.span(p);
        state.auto_refresh = false; // one action, then stop refreshing
    }

    // Find an engine and read it before the first frame, so the window opens
    // with data rather than a skeleton that fills in on its own.
    state.discover();
    try app.run();
}
