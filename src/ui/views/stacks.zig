//! Stacks: containers grouped by their Compose project labels. Nothing is
//! parsed from YAML — Compose puts the project, service and config paths on
//! every container it creates, so grouping is a label lookup.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const bulk = @import("../../bulk.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");

pub fn view(s: *app.State, c: *ui.Context) void {
    var projects: usize = 0;
    for (s.containers) |ct| {
        if (ct.project() == null) continue;
        if (isFirstOfProject(s.containers, ct)) projects += 1;
    }
    const subtitle = widgets.count(c, projects, "project", "projects");
    widgets.pageHeader(c, "Stacks", subtitle);

    if (projects == 0) {
        widgets.emptyState(c, "code", "No Compose stacks", "Containers started by docker compose appear here, grouped by project");
        return;
    }

    const list = ui.div(c).column().wFull().gap(10);
    defer list.end();

    for (s.containers, 0..) |ct, i| {
        const project = ct.project() orelse continue;
        if (!isFirstOfProject(s.containers[0..i], ct)) continue;
        projectCard(s, c, project, ct);
    }
}

fn isFirstOfProject(all: []const @import("../../engine/api.zig").Container, ct: @import("../../engine/api.zig").Container) bool {
    const project = ct.project() orelse return false;
    for (all) |other| {
        if (other.Id.len == 0) continue;
        if (std.mem.eql(u8, other.Id, ct.Id)) return true;
        const p = other.project() orelse continue;
        if (std.mem.eql(u8, p, project)) return false;
    }
    return true;
}

fn projectCard(s: *app.State, c: *ui.Context, project: []const u8, first: @import("../../engine/api.zig").Container) void {
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
    if (running < total and projectButton(c, "Start all", "play.fill", true)) bulk.startProject(s, project);
    if (running > 0 and projectButton(c, "Stop all", "stop.fill", false)) bulk.stopProject(s, project);
    head.end();

    for (s.containers) |ct| {
        const p = ct.project() orelse continue;
        if (!std.mem.eql(u8, p, project)) continue;
        const row = ui.div(c).row().wFull().h(28).itemsCenter().gap(8).px(4);
        defer row.end();
        const svc = ct.service() orelse ct.name();
        ui.text(c, svc, .{ .size = 12, .weight = .medium, .truncate = .ellipsis });
        ui.text(c, ct.name(), .{ .size = 11, .color = c.theme.muted, .truncate = .ellipsis });
        ui.spacer(c);
        widgets.pill(c, ct.State);
    }
}

fn projectButton(c: *ui.Context, label: []const u8, icon_name: []const u8, primary: bool) bool {
    const box = ui.div(c).row().h(24).px(10).itemsCenter().gap(5).rounded(12)
        .bg(if (primary) c.theme.accent else c.theme.surface_hover).control();
    ui.icon(c, icon_name, .{ .size = 9, .color = if (primary) c.theme.accent_text else c.theme.text });
    ui.text(c, label, .{ .size = 11.5, .color = if (primary) c.theme.accent_text else c.theme.text });
    const clicked = box.clicked();
    box.end();
    return clicked;
}

test "a compose project is claimed once, by its first container" {
    var a = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer a.deinit();
    var b = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer b.deinit();
    const items = [_]@import("../../engine/api.zig").Container{
        .{ .Id = "1", .Labels = a.value },
        .{ .Id = "2", .Labels = b.value },
    };
    try std.testing.expect(isFirstOfProject(items[0..1], items[0]));
    try std.testing.expect(!isFirstOfProject(items[0..2], items[1]));
    try std.testing.expect(!isFirstOfProject(items[0..1], .{ .Id = "3" }));
}
