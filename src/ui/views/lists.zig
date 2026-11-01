//! The list views: containers, images, volumes and networks. Same anatomy
//! everywhere — header strip, virtualized rows, one action per row.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");
const shell = @import("../shell.zig");

const ROW = 44.0;

pub fn containers(s: *app.State, c: *ui.Context) void {
    const count = c.print("{d} running · {d} total", .{ app.runningCount(s), s.containers.len });
    widgets.pageHeader(c, "Containers", count);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);

    if (s.containers.len == 0) {
        widgets.emptyState(c, "cube", "No containers yet", "docker run --rm -it alpine sh");
        return;
    }

    // With the detail pane open the list has ~430pt less room, so it keeps
    // only the columns that answer "which container, and how busy".
    const compact = s.selected != null;
    const full_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 150 },
        .{ .name = "IMAGE", .width = 140 },
        .{ .name = "STATE", .width = 76 },
        .{ .name = "CPU", .width = 62, .align_end = true },
        .{ .name = "MEM", .width = 78, .align_end = true },
        .{ .name = "PROJECT", .width = 90 },
        .{ .name = "PORTS", .width = 100 },
        .{ .name = "ACTIONS", .width = 160 },
    };
    const narrow_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 120 },
        .{ .name = "STATE", .width = 70 },
        .{ .name = "CPU", .width = 56, .align_end = true },
        .{ .name = "MEM", .width = 70, .align_end = true },
        .{ .name = "ACTIONS", .width = 150 },
    };
    widgets.tableHeader(c, if (compact) narrow_cols[0..] else full_cols[0..]);

    var rows = ui.list(c, .{ .count = s.containers.len, .row_height = ROW });
    while (rows.next()) |i| {
        const ct = s.containers[i];
        const is_selected = s.selected != null and std.mem.eql(u8, s.selected.?, ct.Id);
        const row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(7)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else c.theme.surface);
        defer row.end();
        if (row.clicked()) s.select(&ct);

        // One line per cell, and no deferred box that siblings fall into:
        // `defer name_cell.end()` would make everything after it a child of
        // the name column, which is why this row rendered vertically.
        widgets.cell(c, ct.name(), if (compact) 120 else 150, false);
        if (!compact) widgets.cellMuted(c, ct.Image, 140, false);
        widgets.pillCell(c, ct.State, if (compact) 70 else 76);
        const st = app.statFor(s, ct.Id);
        // A stopped container has no stats at all: say so with a dash rather
        // than a zero, which would read as "idle" instead of "unknown".
        if (ct.isRunning()) {
            widgets.cell(c, format.percent(c, st.cpu), if (compact) 56 else 62, true);
            widgets.cell(c, format.bytes(c, st.mem), if (compact) 70 else 78, true);
        } else {
            widgets.cellMuted(c, "—", if (compact) 56 else 62, true);
            widgets.cellMuted(c, "—", if (compact) 70 else 78, true);
        }
        if (!compact) {
            widgets.cellMuted(c, ct.project() orelse "—", 90, false);
            widgets.cellMuted(c, widgets.portsText(c, &ct, 2), 100, false);
        }
        actions(s, c, &ct);
    }
    rows.end();
}

fn actions(s: *app.State, c: *ui.Context, ct: *const @import("../../engine/api.zig").Container) void {
    const row = ui.div(c).row().w(160).h(24).itemsCenter().gap(4);
    defer row.end();
    if (ct.isRunning()) {
        if (smallButton(c, "Stop", false)) s.confirm(ct.Id, ct.name(), "Stop", "POST", "/containers/{s}/stop");
    } else {
        if (smallButton(c, "Start", false)) s.act("POST", "/containers/{s}/start", "Started");
    }
    if (smallButton(c, "Restart", false)) s.act("POST", "/containers/{s}/restart", "Restarted");
    if (smallButton(c, "Delete", true)) s.confirm(ct.Id, ct.name(), "Delete", "DELETE", "/containers/{s}?force=1");
}

fn smallButton(c: *ui.Context, label: []const u8, danger: bool) bool {
    const color = if (danger) c.theme.danger else c.theme.text;
    const box = ui.div(c).row().h(20).px(7).itemsCenter().rounded(10).bg(c.theme.surface_hover);
    defer box.end();
    ui.text(c, label, .{ .size = 10.5, .color = color });
    return box.clicked();
}

pub fn images(s: *app.State, c: *ui.Context) void {
    const count = c.print("{d} images", .{s.images.len});
    widgets.pageHeader(c, "Images", count);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);
    if (s.images.len == 0) {
        widgets.emptyState(c, "doc", "No images", "docker pull alpine");
        return;
    }
    const cols = [_]widgets.Col{
        .{ .name = "REPOSITORY:TAG", .width = 300 },
        .{ .name = "ID", .width = 110 },
        .{ .name = "SIZE", .width = 100, .align_end = true },
        .{ .name = "AGE", .width = 100, .align_end = true },
        .{ .name = "STATUS", .width = 120 },
    };
    widgets.tableHeader(c, &cols);

    var rows = ui.list(c, .{ .count = s.images.len, .row_height = ROW });
    while (rows.next()) |i| {
        const im = s.images[i];
        const row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(7).bg(c.theme.surface);
        defer row.end();
        widgets.cell(c, im.tag(), 300, false);
        const id = im.Id;
        const short = if (id.len > 19) id[7..19] else id;
        widgets.cellMuted(c, short, 110, false);
        widgets.cell(c, format.bytes(c, im.Size), 100, true);
        widgets.cellMuted(c, format.ago(c, im.Created, format.now()), 100, true);
        widgets.pill(c, if (im.dangling()) "dangling" else "ready");
    }
    rows.end();
}

pub fn volumes(s: *app.State, c: *ui.Context) void {
    const count = c.print("{d} volumes", .{s.volumes.len});
    widgets.pageHeader(c, "Volumes", count);
    if (s.volumes.len == 0) {
        widgets.emptyState(c, "folder", "No volumes", "Volumes appear here once a container declares one");
        return;
    }
    const cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 260 },
        .{ .name = "DRIVER", .width = 100 },
        .{ .name = "SIZE", .width = 100, .align_end = true },
        .{ .name = "IN USE", .width = 90, .align_end = true },
        .{ .name = "MOUNTPOINT", .width = 260 },
    };
    widgets.tableHeader(c, &cols);

    var rows = ui.list(c, .{ .count = s.volumes.len, .row_height = ROW });
    while (rows.next()) |i| {
        const v = s.volumes[i];
        const row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(7).bg(c.theme.surface);
        defer row.end();
        widgets.cell(c, v.Name, 260, false);
        widgets.cellMuted(c, v.Driver, 100, false);
        if (v.UsageData) |u| {
            if (u.Size >= 0) {
                widgets.cell(c, format.bytes(c, @as(u64, @intCast(u.Size))), 100, true);
            } else {
                widgets.cellMuted(c, "—", 100, true);
            }
            widgets.cell(c, c.print("{d}", .{u.RefCount}), 90, true);
        } else {
            widgets.cellMuted(c, "—", 100, true);
            widgets.cellMuted(c, "—", 90, true);
        }
        widgets.cellMuted(c, v.Mountpoint, 260, false);
    }
    rows.end();
}

pub fn networks(s: *app.State, c: *ui.Context) void {
    const count = c.print("{d} networks", .{s.networks.len});
    widgets.pageHeader(c, "Networks", count);
    if (s.networks.len == 0) {
        widgets.emptyState(c, "network", "No networks", "");
        return;
    }
    const cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 240 },
        .{ .name = "DRIVER", .width = 100 },
        .{ .name = "SCOPE", .width = 90 },
        .{ .name = "ATTACHED", .width = 90, .align_end = true },
    };
    widgets.tableHeader(c, &cols);

    var rows = ui.list(c, .{ .count = s.networks.len, .row_height = ROW });
    while (rows.next()) |i| {
        const n = s.networks[i];
        const row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(7).bg(c.theme.surface);
        defer row.end();
        const dim = n.builtin();
        if (dim) {
            widgets.cellMuted(c, n.Name, 240, false);
            widgets.cellMuted(c, n.Driver, 100, false);
            widgets.cellMuted(c, n.Scope, 90, false);
        } else {
            widgets.cell(c, n.Name, 240, false);
            widgets.cellMuted(c, n.Driver, 100, false);
            widgets.cellMuted(c, n.Scope, 90, false);
        }
        widgets.cellMuted(c, c.print("{d}", .{n.attached()}), 90, true);
    }
    rows.end();
}
