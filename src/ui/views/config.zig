//! Runtimes and Settings: the engine chooser, socket discovery and the
//! preferences that belong to the machine rather than to a container.
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const discovery = @import("../../engine/discovery.zig");
const ops = @import("../../ops.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");
const prefs = @import("../../settings.zig");

const KINDS = [_]discovery.Kind{
    .docker_desktop,
    .orbstack,
    .rancher,
    .dockerd,
    .colima,
    .lima,
    .podman_machine,
    .podman,
};

pub fn runtimes(s: *app.State, c: *ui.Context) void {
    var live: usize = 0;
    for (s.runtimes) |rt| {
        if (rt.reachable) live += 1;
    }
    widgets.pageHeader(c, "Runtimes", c.print("{d} {s}", .{ live, if (live == 1) "engine" else "engines" }));

    const bar = ui.div(c).row().wFull().itemsCenter().gap(8).pb(8);
    if (s.ops.running()) {
        ui.text(c, s.ops.label(), .{ .size = 11.5, .weight = .medium, .color = .hex(0xff9f0a), .truncate = .ellipsis });
        ui.spacer(c);
    } else {
        ui.text(c, "Sockets are probed in parallel. Choose one, or add a path below.", .{ .size = 11.5, .color = c.theme.muted, .truncate = .ellipsis });
        ui.spacer(c);
        if (settingButton(c, "Rescan sockets", false)) s.discover();
    }
    bar.end();

    customSocket(s, c);

    for (KINDS) |kind| kindCard(s, c, kind);
}

fn customSocket(s: *app.State, c: *ui.Context) void {
    const card = ui.div(c).column().wFull().gap(7).p(14).rounded(10).bg(c.theme.surface).border(c.theme.border);
    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    widgets.chip(c, "terminal", c.theme.accent, 22);
    ui.text(c, "Custom socket", .{ .size = 13, .weight = .semibold });
    ui.spacer(c);
    ui.text(c, "unix:// is optional", .{ .size = 10.5, .color = c.theme.muted });
    head.end();

    const row = ui.div(c).row().wFull().itemsCenter().gap(8).pt(2);
    const field = ui.textField(c, &s.socket_input, .{
        .placeholder = "/path/to/docker.sock",
        .width = .fill,
        .size = 12,
    });
    if (field.submitted) _ = s.useSocket(s.socket_input.text());
    if (settingButton(c, "Connect", true)) _ = s.useSocket(s.socket_input.text());
    row.end();

    if (s.preferred_socket) |p| {
        // Copy to the frame before the Forget button can free the preference.
        const line = c.print("Preferred: {s}", .{p});
        const prefs_row = ui.div(c).row().wFull().itemsCenter().gap(8);
        const slot = ui.div(c).row().grow();
        ui.text(c, line, .{
            .size = 10.5,
            .mono = true,
            .color = c.theme.muted,
            .truncate = .middle,
        });
        slot.end();
        if (settingButton(c, "Forget", false)) s.forgetSocket();
        prefs_row.end();
    }
    card.end();
}

fn kindCard(s: *app.State, c: *ui.Context, kind: discovery.Kind) void {
    var total: usize = 0;
    var live: usize = 0;
    var active = false;
    for (s.runtimes, 0..) |rt, i| {
        if (rt.kind != kind) continue;
        total += 1;
        if (rt.reachable) live += 1;
        if (i == s.active) active = true;
    }
    if (total == 0) return;

    const expanded = s.expanded_kind == kind;
    const card = ui.div(c).column().wFull().gap(7).p(14).rounded(10)
        .bg(if (active) c.theme.accent.alpha(0.06) else c.theme.surface)
        .border(if (active) c.theme.accent else c.theme.border);

    const head = ui.div(c).row().wFull().itemsCenter().gap(10);
    widgets.chip(c, "desktopcomputer", if (active) c.theme.accent else c.theme.muted, 24);
    ui.text(c, kind.label(), .{ .size = 13, .weight = .medium });
    if (active) ui.text(c, "active", .{ .size = 10.5, .weight = .medium, .color = c.theme.accent });
    ui.spacer(c);
    ui.text(c, c.print("{d} answering", .{live}), .{ .size = 11, .color = c.theme.muted });
    if (total > 1) {
        if (settingButton(c, if (expanded) "Hide sockets" else c.print("Show {d} sockets", .{total}), false)) {
            s.expanded_kind = if (expanded) null else kind;
            s.redraw();
        }
    }
    head.end();

    if (expanded or total == 1) {
        for (s.runtimes, 0..) |rt, i| {
            if (rt.kind != kind) continue;
            runtimeRow(s, c, i, &rt);
        }
    } else if (active) {
        if (s.activeRuntime()) |rt| {
            ui.text(c, rt.socket, .{ .size = 10.5, .mono = true, .color = c.theme.muted, .truncate = .middle });
        }
    }
    card.end();
}

fn runtimeRow(s: *app.State, c: *ui.Context, index: usize, rt: *const discovery.Runtime) void {
    const active = index == s.active;
    const row = ui.div(c).row().wFull().itemsCenter().gap(8).h(30).px(2);
    const dot = ui.div(c).w(6).h(6).rounded(3)
        .bg(if (rt.reachable) c.theme.success else c.theme.danger.alpha(0.7));
    dot.end();

    const text = ui.div(c).column().grow().gap(0);
    ui.text(c, rt.name, .{ .size = 11.5, .weight = .medium });
    ui.text(c, rt.socket, .{ .size = 10, .mono = true, .color = c.theme.muted, .truncate = .middle });
    text.end();

    if (active) ui.text(c, "active", .{ .size = 10.5, .weight = .medium, .color = c.theme.accent });
    if (ops.supported(rt.kind) and !s.ops.running()) {
        if (!rt.reachable) {
            if (settingButton(c, "Start", false)) ops.start(s, .start, rt.kind, rt.socket);
        } else if (active) {
            if (settingButton(c, "Restart", false)) ops.start(s, .restart, rt.kind, rt.socket);
        }
    }
    if (!rt.reachable and !ops.supported(rt.kind)) {
        ui.text(c, "not answering", .{ .size = 10.5, .color = c.theme.muted });
    }
    if (!active and rt.reachable and !s.ops.running()) {
        if (settingButton(c, "Use", false)) s.selectRuntime(index);
    }
    if (rt.reachable and ops.supported(rt.kind) and !s.ops.running()) {
        if (settingButton(c, "Stop", false)) ops.start(s, .stop, rt.kind, rt.socket);
    }
    row.end();
}

pub fn settings(s: *app.State, c: *ui.Context) void {
    widgets.pageHeader(c, "Settings", "berth 0.1 · a native container monitor");
    const page = ui.div(c).column().wFull().gap(12);
    defer page.end();

    // About: full width, and every paragraph wraps inside its card. Long prose
    // used to run straight out of the old 560pt box.
    const about = ui.div(c).column().wFull().gap(7).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    ui.text(c, "About berth", .{ .size = 15, .weight = .semibold });
    // A readable measure: a card this wide should not turn body copy into
    // 150-character lines just because the window grew.
    const prose = ui.div(c).column().w(640).gap(6);
    ui.text(c, "berth is a native monitor for Docker, Podman, Colima and Lima. It talks to the engine's HTTP API over its Unix socket directly — no webview, no helper daemon, no telemetry.", .{ .size = 12, .color = c.theme.muted, .wrap = true });
    ui.text(c, "The last complete snapshot stays on screen while a worker refreshes the next one, so the tables and graphs neither blank nor freeze. Container CPU and memory use the same two-sample window as `docker stats`.", .{ .size = 12, .color = c.theme.muted, .wrap = true });
    ui.text(c, "Built in Zig 0.17 on nanoui. On macOS the host figures come from Mach's host statistics; on Linux, from /proc. The warm palette and cell graphs follow nanoui's sibling project, nanomon.", .{ .size = 12, .color = c.theme.muted, .wrap = true });
    ui.text(c, "Every list has live search, containers pause and resume, the inspector carries the engine's own JSON and a copyable `docker run` line, and the Events view remembers what the engine did. Not built yet: Compose up/down, pulling images or running new containers.", .{ .size = 11.5, .color = c.theme.muted.alpha(0.8), .wrap = true });
    prose.end();
    about.end(); // before the row, or the row and everything below nest inside it

    // Appearance left, Engine right. The row stretches both cards to the same
    // height so the shorter one does not leave a hole under it.
    const middle = ui.div(c).row().wFull().itemsStretch().gap(12);
    const appearance = ui.div(c).column().grow().gap(5).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    ui.text(c, "Appearance", .{ .size = 15, .weight = .semibold });
    ui.text(c, "System follows this machine's desktop. The choice is remembered between launches.", .{ .size = 12, .color = c.theme.muted, .wrap = true });
    const row = ui.div(c).row().gap(6).pt(4);
    for ([_]prefs.Appearance{ .system, .light, .dark }) |option| {
        const on = s.appearance == option;
        const button = ui.div(c).row().h(28).px(12).itemsCenter().rounded(14)
            .bg(if (on) c.theme.accent.alpha(0.16) else c.theme.surface_hover);
        ui.text(c, option.label(), .{
            .size = 12,
            .weight = if (on) .medium else .regular,
            .color = if (on) c.theme.accent else c.theme.text,
        });
        if (button.clicked()) s.setAppearance(option);
        button.end();
    }
    row.end(); // before the next section, or it becomes a child of this row
    ui.spacer(c);
    ui.text(c, "Comfortable rows. A compact row height is not available yet.", .{ .size = 11.5, .color = c.theme.muted, .wrap = true });
    appearance.end();

    engineCard(s, c);
    middle.end();

    // The old bottom bar's actions live here now.
    const maint = ui.div(c).column().wFull().gap(8).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer maint.end();
    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, "Maintenance", .{ .size = 15, .weight = .semibold });
    ui.spacer(c);
    ui.text(c, if (s.df) |df| c.print("{s} reclaimable", .{format.bytesSigned(c, df.reclaimable())}) else "reclaimable —", .{
        .size = 11.5,
        .color = c.theme.muted,
    });
    head.end();
    ui.text(c, "Clean up removes stopped containers, unused images and build cache; unused volumes and networks are opt-in. Refresh reloads the engine immediately instead of waiting for the next five-second pass.", .{ .size = 12, .color = c.theme.muted, .wrap = true });
    const actions = ui.div(c).row().wFull().gap(8).pt(2);
    if (settingButton(c, "Clean up unused…", true)) s.openPrune();
    if (settingButton(c, "Refresh now", false)) s.refresh();
    actions.end();
}

fn engineCard(s: *app.State, c: *ui.Context) void {
    const engine = ui.div(c).column().grow().gap(5).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    ui.text(c, "Engine", .{ .size = 15, .weight = .semibold });
    if (s.activeRuntime()) |r| {
        ui.text(c, r.name, .{ .size = 12.5, .weight = .medium });
        ui.text(c, if (r.reachable) c.print("v{s} · API {s}", .{ r.version.Version, r.version.ApiVersion }) else "not answering", .{
            .size = 11.5,
            .color = if (r.reachable) c.theme.muted else c.theme.danger,
        });
        ui.text(c, r.socket, .{ .size = 11, .mono = true, .color = c.theme.muted, .wrap = true });
    } else {
        ui.text(c, "No engine selected.", .{ .size = 12, .color = c.theme.muted });
    }

    if (engineButton(c, if (s.engine_menu_open) "Hide engines" else "Choose engine", s.engine_menu_open)) {
        s.engine_menu_open = !s.engine_menu_open;
        s.redraw();
    }

    if (s.engine_menu_open) {
        var shown: usize = 0;
        for (s.runtimes, 0..) |rt, i| {
            if (!rt.reachable) continue;
            shown += 1;
            const row = ui.div(c).row().wFull().itemsCenter().gap(8).h(30).px(8).rounded(6)
                .bg(if (i == s.active) c.theme.accent.alpha(0.08) else c.theme.surface_hover);
            const text = ui.div(c).column().grow().gap(0);
            ui.text(c, rt.name, .{ .size = 11.5, .weight = .medium });
            ui.text(c, rt.socket, .{ .size = 10, .mono = true, .color = c.theme.muted, .truncate = .middle });
            text.end();
            if (i == s.active) {
                ui.text(c, "active", .{ .size = 10.5, .color = c.theme.accent });
            } else if (settingButton(c, "Use", false)) {
                s.selectRuntime(i);
                s.engine_menu_open = false;
            }
            row.end();
        }
        if (shown == 0) {
            ui.text(c, "No engine is answering. Add a socket on Runtimes.", .{ .size = 11.5, .color = c.theme.muted, .wrap = true });
        }
        if (settingButton(c, "All sockets…", false)) {
            s.tab = .runtimes;
            s.engine_menu_open = false;
            s.redraw();
        }
    }
    engine.end();
}

fn engineButton(c: *ui.Context, label: []const u8, open: bool) bool {
    const box = ui.div(c).row().h(28).px(12).itemsCenter().gap(6).rounded(14).bg(c.theme.surface_hover);
    ui.text(c, label, .{ .size = 12, .weight = .medium, .color = c.theme.text });
    ui.icon(c, if (open) "chevron.up" else "chevron.down", .{ .size = 9, .color = c.theme.muted });
    const clicked = box.clicked();
    box.end();
    return clicked;
}

fn settingButton(c: *ui.Context, label: []const u8, primary: bool) bool {
    const box = ui.div(c).row().h(28).px(12).itemsCenter().rounded(14)
        .bg(if (primary) c.theme.accent else c.theme.surface_hover);
    defer box.end();
    ui.text(c, label, .{
        .size = 12,
        .weight = .medium,
        .color = if (primary) c.theme.accent_text else c.theme.text,
    });
    return box.clicked();
}
