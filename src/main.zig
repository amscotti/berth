//! berth: a native monitor for container runtimes — Docker, Podman, Colima
//! and Lima — on macOS and Linux, built with nanoui on Zig 0.17.
const std = @import("std");
const nanoui = @import("nanoui");
const app_mod = @import("app.zig");
const boot = @import("boot.zig");
const settings = @import("settings.zig");
const tray = @import("tray.zig");
const builtin = @import("builtin");

extern "c" fn getenv(name: [*:0]const u8) ?[*:0]u8;

/// Every window berth opens shares these options, so the close callback that
/// keeps `state.window` honest can never be forgotten on one of the paths.
fn windowOptions(state: *app_mod.State, size: [2]f64) nanoui.WindowOptions {
    return .{
        .title = "berth",
        .width = size[0],
        .height = size[1],
        .min_width = 960,
        .min_height = 600,
        .theme = state.themeFor(),
        .content = .{ .view = .init(state, app_mod.State.view) },
        .on_close = .{ .ctx = state, .func = windowClosed },
    };
}

/// Opens the window again with the current theme, at the size it already had:
/// switching appearance must not also resize the window the user arranged.
fn reopen(state: *app_mod.State) void {
    const old = state.window;
    const size: [2]f64 = if (old) |w| w.contentSize() else state.last_size orelse .{ 1280, 760 };
    const next = state.app.newWindow(windowOptions(state, size)) catch return;
    if (old) |w| w.close();
    state.window = next;
    next.redraw();
}

/// The window object is freed on close; the state must stop pointing at it or
/// the tray's next click would show a freed window.
fn windowClosed(ctx: ?*anyopaque, window: *nanoui.Window) void {
    const state: *app_mod.State = @ptrCast(@alignCast(ctx.?));
    // Remember the arranged size: the tray's next click reopens there, not at
    // the default, matching what a theme switch already preserves.
    state.last_size = window.contentSize();
    if (state.closing) return;
    state.window = null;
}

/// `BERTH_SIZE=960x600` opens the window at that content size. It is how the
/// layout is checked at the small end without a mouse, and how snapshots of
/// each breakpoint are taken.
fn sizeOverride() ?[2]f64 {
    const p = getenv("BERTH_SIZE") orelse return null;
    const text = std.mem.span(p);
    const x = std.mem.indexOfScalar(u8, text, 'x') orelse return null;
    const w = std.fmt.parseFloat(f64, text[0..x]) catch return null;
    const h = std.fmt.parseFloat(f64, text[x + 1 ..]) catch return null;
    if (w < 320 or h < 240) return null;
    // The window's own minimum is the real floor; a test override below it
    // would only produce a layout the app promises not to support.
    return .{ @max(w, 960), @max(h, 600) };
}

pub fn main(init: std.process.Init) !void {
    const gpa = init.gpa;
    const app = try nanoui.App.create(init, .{
        .name = "berth",
        // With a tray icon, closing the window leaves the monitor running in
        // the menu bar. Where there is no tray, the last window quits as before.
        .quit_on_last_window_closed = builtin.os.tag != .macos,
    });
    defer app.destroy();

    const state = try boot.create(gpa, init.io, app);
    defer boot.destroy(state);

    // Preferences first: the appearance decides the theme the window opens
    // with, and the theme is fixed at creation. A remembered socket is
    // transferred to the state, which owns it from here.
    const prefs = settings.load(gpa, init.io);
    state.appearance = prefs.appearance;
    if (prefs.socket.len > 0) {
        state.preferred_socket = @constCast(prefs.socket);
        state.socket_input.set(prefs.socket);
    }
    // An explicit override, for screenshots and for anyone whose desktop does
    // not report a theme. It wins over the saved preference for this run only.
    if (getenv("BERTH_THEME")) |p| {
        const name = std.mem.span(p);
        if (std.ascii.eqlIgnoreCase(name, "dark")) state.appearance = .dark;
        if (std.ascii.eqlIgnoreCase(name, "light")) state.appearance = .light;
    }
    state.recreate = &reopen;

    const size = sizeOverride() orelse .{ 1280, 760 };
    state.window = try app.newWindow(windowOptions(state, size));

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
    // `BERTH_SEARCH=web` fills the open view's search field, so a filtered
    // table can be snapshotted headlessly.
    if (getenv("BERTH_SEARCH")) |p| {
        if (state.search.forTab(state.tab)) |buffer| buffer.set(std.mem.span(p));
    }
    // `BERTH_PULL=nginx:alpine` and `BERTH_RUN_IMAGE=…` drive the creation
    // card without a mouse, through the same jobs a click uses.
    if (getenv("BERTH_PULL")) |p| {
        state.image_tasks.pull_input.set(std.mem.span(p));
        state.pending_image = .pull;
    }
    if (getenv("BERTH_RUN_IMAGE")) |p| {
        state.image_tasks.run_image.set(std.mem.span(p));
        state.pending_image = .run;
    }
    if (getenv("BERTH_RUN_NAME")) |p| state.image_tasks.run_name.set(std.mem.span(p));
    if (getenv("BERTH_RUN_PORTS")) |p| state.image_tasks.run_ports.set(std.mem.span(p));
    if (getenv("BERTH_RUN_VOLUMES")) |p| state.image_tasks.run_volumes.set(std.mem.span(p));
    if (getenv("BERTH_RUN_ENV")) |p| state.image_tasks.run_env.set(std.mem.span(p));
    if (getenv("BERTH_ACT")) |p| {
        state.pending_act = std.mem.span(p);
        state.auto_refresh = false; // one action, then stop refreshing
    }
    // `BERTH_SHEET=prune|create-volume|create-error|form-errors|bulk` opens a
    // sheet or a form mistake before the first frame. The values that need a
    // container row wait for the first snapshot (`pending_sheet`).
    if (getenv("BERTH_SHEET")) |p| {
        const name = std.mem.span(p);
        const eq = std.ascii.eqlIgnoreCase;
        // A sheet is a still frame: one snapshot, then no timer racing exit.
        state.auto_refresh = false;
        if (eq(name, "prune")) {
            state.prune_open = true;
        } else if (eq(name, "create-volume") or eq(name, "create-network")) {
            state.tab = if (eq(name, "create-volume")) .volumes else .networks;
            state.create_dialog.open = if (eq(name, "create-volume")) .volume else .network;
            state.create_dialog.generation +%= 1;
            state.create_dialog.name.focus();
        } else if (eq(name, "create-error")) {
            state.tab = .volumes;
            state.create_dialog.open = .volume;
            state.create_dialog.generation +%= 1;
            state.create_dialog.name.set("bad name");
            state.create_dialog.attempted = true;
        } else if (eq(name, "form-errors")) {
            state.tab = .images;
            state.creation_card_open = true;
            state.image_tasks.pull_input.set("bad ref");
            state.image_tasks.pull_attempted = true;
            state.image_tasks.run_image.set("nginx:alpine");
            state.image_tasks.run_ports.set("8080");
            state.image_tasks.run_attempted = true;
        } else if (eq(name, "bulk")) {
            state.tab = .containers;
            state.bulk.enabled = true;
        } else {
            state.pending_sheet = name;
        }
    }

    // Start finding an engine on a worker. The window opens on its skeletons
    // instead of waiting out a stale socket, and the scan's completion starts
    // the first refresh.
    tray.create(state);
    state.discover();
    try app.run();
}
