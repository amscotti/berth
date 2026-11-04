//! Stacks: containers grouped by their Compose project labels. Nothing is
//! parsed from YAML — Compose puts the project, service and config paths on
//! every container it creates, so grouping is a label lookup.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const bulk = @import("../../bulk.zig");
const widgets = @import("../widgets.zig");
const theme = @import("../theme.zig");

pub fn view(s: *app.State, c: *ui.Context) void {
    // Reading lasts until the first snapshot commits; an empty stack list
    // would claim the engine has no compose projects before it has been read.
    const first = widgets.firstRead(s.last_update, s.error_text);
    if (first == .reading) {
        widgets.pageHeader(c, "Stacks", "Reading engine…");
        widgets.loadingRows(c, 4);
        return;
    }
    if (first == .failed) {
        widgets.pageHeader(c, "Stacks", "engine not answering");
        widgets.errorBanner(c, s.error_text);
        widgets.emptyState(c, "code", "Engine not answering", widgets.engine_help);
        return;
    }

    var projects: usize = 0;
    for (s.containers) |ct| {
        if (ct.project() == null) continue;
        if (isFirstOfProject(s.containers, ct)) projects += 1;
    }
    const subtitle = if (s.error_text.len > 0 and projects == 0)
        "engine not answering"
    else
        widgets.count(c, projects, "project", "projects");
    widgets.pageHeader(c, "Stacks", subtitle);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);

    // The bulk worker owns start/stop across projects; while it runs, every
    // card's actions are shown disabled rather than pretending a second batch
    // can join it. A single-container mutation or an open confirmation blocks
    // the same buttons for the same reason.
    const busy = s.bulk.running();
    const actions_enabled = !s.mutating() and s.bulk.pending == null;
    if (busy) {
        const line = ui.div(c).row().wFull().itemsCenter().gap(6).pb(8);
        ui.icon(c, "hourglass", .{ .size = 11, .color = theme.warning(c.theme) });
        ui.text(c, if (s.bulk.canceling) "Canceling…" else c.print("{s} containers…", .{bulk.runningLabel(&s.bulk)}), .{
            .size = 11.5,
            .weight = .medium,
            .color = theme.warning(c.theme),
        });
        line.end();
    }

    if (projects == 0) {
        if (s.error_text.len > 0) {
            widgets.emptyState(c, "code", "Engine not answering", widgets.engine_help);
        } else {
            widgets.emptyState(c, "code", "No Compose stacks", "Containers started by docker compose appear here, grouped by project");
        }
        return;
    }

    const list = ui.div(c).column().wFull().gap(10);
    defer list.end();

    for (s.containers, 0..) |ct, i| {
        const project = ct.project() orelse continue;
        if (!isFirstOfProject(s.containers[0..i], ct)) continue;
        projectCard(s, c, project, ct, actions_enabled);
    }
}

fn stateLabel(ct: api.Container) []const u8 {
    return ct.health() orelse ct.State;
}

fn isFirstOfProject(all: []const api.Container, ct: api.Container) bool {
    const project = ct.project() orelse return false;
    for (all) |other| {
        if (other.Id.len == 0) continue;
        if (std.mem.eql(u8, other.Id, ct.Id)) return true;
        const p = other.project() orelse continue;
        if (std.mem.eql(u8, p, project)) return false;
    }
    return true;
}

fn projectCard(s: *app.State, c: *ui.Context, project: []const u8, first: api.Container, enabled: bool) void {
    const card = ui.div(c).column().wFull().gap(6).p(14).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer card.end();

    var running: usize = 0;
    var total: usize = 0;
    for (s.containers) |ct| {
        const p = ct.project() orelse continue;
        if (!std.mem.eql(u8, p, project)) continue;
        total += 1;
        if (ct.isUp()) running += 1;
    }

    const head = ui.div(c).row().wFull().itemsCenter().gap(10);
    ui.icon(c, "code", .{ .size = 16, .color = c.theme.accent });
    ui.text(c, project, .{ .size = 14, .weight = .semibold, .truncate = .ellipsis });
    ui.text(c, c.print("{d}/{d} running", .{ running, total }), .{ .size = 12, .color = c.theme.muted });
    ui.spacer(c);

    if (s.layout.content >= 760) {
        if (first.label("com.docker.compose.project.working_dir")) |dir| {
            ui.text(c, dir, .{ .size = 11, .color = c.theme.muted, .truncate = .middle });
        }
    }
    // The one action a Compose project needs from a monitor: start everything
    // that is down, or stop everything that is up, through the bulk worker.
    // Start all is the accent primary, like the creation card's Pull and Run.
    if (running < total and projectButton(c, "Start all", "play.fill", true, enabled)) bulk.startProject(s, project);
    if (running > 0 and projectButton(c, "Stop all", "stop.fill", false, enabled)) bulk.stopProject(s, project);
    head.end();

    for (s.containers) |ct| {
        const p = ct.project() orelse continue;
        if (!std.mem.eql(u8, p, project)) continue;
        const is_selected = s.selected != null and std.mem.eql(u8, s.selected.?, ct.Id);
        var row = ui.div(c).row().wFull().h(28).itemsCenter().gap(8).px(4).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        if (row.clicked()) s.select(&ct);
        const svc = ct.service() orelse ct.name();
        ui.text(c, svc, .{ .size = 12, .weight = .medium, .truncate = .ellipsis });
        ui.text(c, ct.name(), .{ .size = 11, .color = c.theme.muted, .truncate = .ellipsis });
        ui.spacer(c);
        // A service whose healthcheck is failing is not "running" in the sense
        // the row means; the health verdict is the status.
        widgets.pill(c, stateLabel(ct));
        row.end();
    }
}

fn projectButton(c: *ui.Context, label: []const u8, icon_name: []const u8, primary: bool, enabled: bool) bool {
    const bg = if (!enabled)
        c.theme.surface_hover.alpha(0.6)
    else if (primary)
        c.theme.accent
    else
        c.theme.surface_hover;
    const fg = if (!enabled)
        c.theme.muted
    else if (primary)
        c.theme.accent_text
    else
        c.theme.text;
    const box = ui.div(c).row().h(24).px(10).itemsCenter().gap(5).rounded(12).bg(bg).control();
    ui.icon(c, icon_name, .{ .size = 9, .color = fg });
    ui.text(c, label, .{ .size = 11.5, .color = fg });
    const clicked = if (enabled) box.clicked() else false;
    box.end();
    return clicked;
}

test "a compose project is claimed once, by its first container" {
    var a = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer a.deinit();
    var b = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer b.deinit();
    const items = [_]api.Container{
        .{ .Id = "1", .Labels = a.value },
        .{ .Id = "2", .Labels = b.value },
    };
    try std.testing.expect(isFirstOfProject(items[0..1], items[0]));
    try std.testing.expect(!isFirstOfProject(items[0..2], items[1]));
    try std.testing.expect(!isFirstOfProject(items[0..1], .{ .Id = "3" }));
}

test "a service row shows its health verdict instead of green running" {
    const unhealthy: api.Container = .{ .State = "running", .Status = "Up 2 minutes (unhealthy)" };
    const plain: api.Container = .{ .State = "running", .Status = "Up 2 minutes" };
    try std.testing.expectEqualStrings("unhealthy", stateLabel(unhealthy));
    try std.testing.expectEqualStrings("running", stateLabel(plain));
}
