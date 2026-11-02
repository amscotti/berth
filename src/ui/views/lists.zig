//! The list views: containers, images, volumes and networks. Same anatomy
//! everywhere — header strip, virtualized rows, one action per row.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");
const search = @import("search.zig");
const shell = @import("../shell.zig");

const ROW = 44.0;

fn numOrder(a: f64, b: f64) std.math.Order {
    if (a < b) return .lt;
    if (a > b) return .gt;
    return .eq;
}

fn ordered(order: std.math.Order, desc: bool) bool {
    return if (desc) order == .gt else order == .lt;
}

const ContainerSort = struct {
    s: *app.State,
    key: u8,
    desc: bool,

    fn lessThan(ctx: ContainerSort, a: *const api.Container, b: *const api.Container) bool {
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(a.name(), b.name()),
            2 => std.ascii.orderIgnoreCase(a.Image, b.Image),
            3 => std.ascii.orderIgnoreCase(a.State, b.State),
            4 => numOrder(app.statFor(ctx.s, a.Id).cpu, app.statFor(ctx.s, b.Id).cpu),
            5 => numOrder(@floatFromInt(app.statFor(ctx.s, a.Id).mem), @floatFromInt(app.statFor(ctx.s, b.Id).mem)),
            6 => std.ascii.orderIgnoreCase(a.project() orelse "", b.project() orelse ""),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};

const ImageSort = struct {
    items: []const api.Image,
    key: u8,
    desc: bool,

    fn lessThan(ctx: ImageSort, a: usize, b: usize) bool {
        const x = ctx.items[a];
        const y = ctx.items[b];
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(x.label(), y.label()),
            2 => std.ascii.orderIgnoreCase(x.Id, y.Id),
            3 => numOrder(@floatFromInt(x.Size), @floatFromInt(y.Size)),
            4 => numOrder(@floatFromInt(x.Created), @floatFromInt(y.Created)),
            5 => std.ascii.orderIgnoreCase(if (x.dangling()) "dangling" else "ready", if (y.dangling()) "dangling" else "ready"),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};

const VolumeSort = struct {
    items: []const api.Volume,
    key: u8,
    desc: bool,

    fn lessThan(ctx: VolumeSort, a: usize, b: usize) bool {
        const x = ctx.items[a];
        const y = ctx.items[b];
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(x.Name, y.Name),
            2 => std.ascii.orderIgnoreCase(x.Driver, y.Driver),
            3 => numOrder(@floatFromInt(volumeSize(x)), @floatFromInt(volumeSize(y))),
            4 => numOrder(@floatFromInt(volumeUse(x)), @floatFromInt(volumeUse(y))),
            5 => std.ascii.orderIgnoreCase(x.Mountpoint, y.Mountpoint),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};

const NetworkSort = struct {
    items: []const api.Network,
    containers: []const api.Container,
    key: u8,
    desc: bool,

    fn lessThan(ctx: NetworkSort, a: usize, b: usize) bool {
        const x = ctx.items[a];
        const y = ctx.items[b];
        const order: std.math.Order = switch (ctx.key) {
            1 => std.ascii.orderIgnoreCase(x.Name, y.Name),
            2 => std.ascii.orderIgnoreCase(x.Driver, y.Driver),
            3 => std.ascii.orderIgnoreCase(x.Scope, y.Scope),
            4 => numOrder(
                @floatFromInt(api.attachedCount(ctx.containers, x.Name)),
                @floatFromInt(api.attachedCount(ctx.containers, y.Name)),
            ),
            else => .eq,
        };
        return ordered(order, ctx.desc);
    }
};


fn volumeSize(v: api.Volume) i64 {
    const u = v.UsageData orelse return -1;
    return u.Size;
}

fn volumeUse(v: api.Volume) i64 {
    const u = v.UsageData orelse return -1;
    return u.RefCount;
}

pub fn containers(s: *app.State, c: *ui.Context) void {
    // The rows this filter keeps, chosen once so the count and the list agree.
    // The search query ANDs with the chip: both narrow what is on screen.
    const q = c.alloc().dupe(u8, s.search.containers.text()) catch "";
    var shown: [512]*const @import("../../engine/api.zig").Container = undefined;
    var n: usize = 0;
    for (s.containers) |*ct| {
        if (!s.filter.keeps(ct)) continue;
        if (!search.keepContainer(q, ct)) continue;
        if (n == shown.len) break;
        shown[n] = ct;
        n += 1;
    }

    const subtitle = if (s.filter == .all and q.len == 0)
        c.print("{d} running · {d} total", .{ app.runningCount(s), s.containers.len })
    else
        c.print("{d} of {d} shown", .{ n, s.containers.len });
    widgets.pageHeader(c, "Containers", subtitle);

    // Filter chips, in the same place and shape on every view that grows one,
    // with the search field at the end of the same row.
    const chips = ui.div(c).row().wFull().gap(6).pb(8);
    for ([_]app.Filter{ .all, .running, .stopped }) |option| {
        const on = s.filter == option;
        const chip = ui.div(c).row().h(24).px(10).itemsCenter().rounded(12)
            .bg(if (on) c.theme.accent.alpha(0.16) else c.theme.surface_hover);
        defer chip.end();
        ui.text(c, option.label(), .{
            .size = 11.5,
            .weight = if (on) .medium else .regular,
            .color = if (on) c.theme.accent else c.theme.text,
        });
        if (chip.clicked()) {
            s.filter = option;
            s.redraw();
        }
    }
    ui.spacer(c);
    _ = search.box(c, &s.search.containers, "Search containers", if (s.selected != null) 120 else 190);
    chips.end();

    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);

    if (n == 0) {
        if (s.containers.len == 0) {
            widgets.emptyState(c, "cube", "No containers yet", "docker run --rm -it alpine sh");
        } else if (q.len > 0) {
            widgets.emptyState(c, "cube", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every container");
        } else {
            widgets.emptyState(c, "cube", "None match this filter", "All shows every container");
        }
        return;
    }

    // With the detail pane open the list has ~430pt less room, so it keeps
    // only the columns that answer "which container, and how busy".
    const compact = s.selected != null;
    // Only when something is actually grouped: a column of dashes costs width
    // that the ports column needs, and says nothing.
    var any_project = false;
    for (s.containers) |ct| {
        if (ct.project() != null) any_project = true;
    }
    const ports_w: f32 = if (any_project) 120 else 180;
    std.mem.sort(*const api.Container, shown[0..n], ContainerSort{
        .s = s,
        .key = s.sort_containers.key,
        .desc = s.sort_containers.desc,
    }, ContainerSort.lessThan);

    const full_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 150, .key = 1 },
        .{ .name = "IMAGE", .width = 140, .key = 2 },
        .{ .name = "STATE", .width = 76, .key = 3 },
        .{ .name = "CPU", .width = 62, .align_end = true, .key = 4 },
        .{ .name = "MEM", .width = 78, .align_end = true, .key = 5 },
        .{ .name = "PROJECT", .width = 90, .skip = !any_project, .key = 6 },
        .{ .name = "PORTS", .width = ports_w },
        .{ .name = "ACTIONS", .width = 160 },
    };
    const narrow_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 120, .key = 1 },
        .{ .name = "STATE", .width = 70, .key = 3 },
        .{ .name = "CPU", .width = 56, .align_end = true, .key = 4 },
        .{ .name = "MEM", .width = 70, .align_end = true, .key = 5 },
        .{ .name = "ACTIONS", .width = 150 },
    };
    widgets.tableHeader(c, if (compact) narrow_cols[0..] else full_cols[0..], &s.sort_containers);

    var rows = ui.list(c, .{ .count = n, .row_height = ROW });
    while (rows.next()) |i| {
        const ct = shown[i].*;
        const is_selected = s.selected != null and std.mem.eql(u8, s.selected.?, ct.Id);
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) s.select(&ct);

        // One line per cell, and no deferred box that siblings fall into:
        // `defer name_cell.end()` would make everything after it a child of
        // the name column, which is why this row rendered vertically.
        widgets.cell(c, ct.name(), if (compact) 120 else 150, false);
        if (!compact) widgets.cellMuted(c, ct.Image, 140, false);
        widgets.pillCell(c, ct.health() orelse ct.State, if (compact) 70 else 76);
        const st = app.statFor(s, ct.Id);
        // A stopped container has no stats at all: say so with a dash rather
        // than a zero, which would read as "idle" instead of "unknown". A
        // paused one still reports — frozen, but real.
        if (ct.isUp()) {
            widgets.cell(c, format.percent(c, st.cpu), if (compact) 56 else 62, true);
            widgets.cell(c, format.bytes(c, st.mem), if (compact) 70 else 78, true);
        } else {
            widgets.cellMuted(c, "—", if (compact) 56 else 62, true);
            widgets.cellMuted(c, "—", if (compact) 70 else 78, true);
        }
        if (!compact) {
            if (any_project) widgets.cellMuted(c, ct.project() orelse "—", 90, false);
            // A pill per published port; clicking one opens it in a browser.
            if (widgets.portPills(c, &ct, if (any_project) 2 else 3, ports_w)) |binding| s.openPort(binding);
        }
        actions(s, c, &ct, if (compact) 150 else 160);
    }
    rows.end();
}

/// Three verbs fit the slot, whichever state the row is in. Pause and Resume
/// live in the detail pane: a fourth button here would shift every column.
fn actions(s: *app.State, c: *ui.Context, ct: *const @import("../../engine/api.zig").Container, width: f32) void {
    const row = ui.div(c).row().w(width).h(24).itemsCenter().gap(4);
    defer row.end();
    if (ct.isPaused()) {
        if (smallButton(c, "Resume", false)) _ = s.actFmt("POST", "/containers/{s}/unpause", ct.Id, "Resumed");
        if (smallButton(c, "Stop", false)) s.confirm(ct.Id, ct.name(), "Stop", "POST", "/containers/{s}/stop");
    } else if (ct.isUp()) {
        if (smallButton(c, "Stop", false)) s.confirm(ct.Id, ct.name(), "Stop", "POST", "/containers/{s}/stop");
        if (smallButton(c, "Restart", false)) _ = s.actFmt("POST", "/containers/{s}/restart", ct.Id, "Restarted");
    } else {
        if (smallButton(c, "Start", false)) _ = s.actFmt("POST", "/containers/{s}/start", ct.Id, "Started");
        if (smallButton(c, "Restart", false)) _ = s.actFmt("POST", "/containers/{s}/restart", ct.Id, "Restarted");
    }
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
    const q = c.alloc().dupe(u8, s.search.images.text()) catch "";
    const order = c.alloc().alloc(usize, s.images.len) catch return;
    var shown: usize = 0;
    for (s.images, 0..) |im, i| {
        if (!search.keepImage(q, im)) continue;
        order[shown] = i;
        shown += 1;
    }
    std.mem.sort(usize, order[0..shown], ImageSort{
        .items = s.images,
        .key = s.sort_images.key,
        .desc = s.sort_images.desc,
    }, ImageSort.lessThan);

    const subtitle = if (q.len > 0)
        c.print("{d} of {d} shown", .{ shown, s.images.len })
    else
        widgets.count(c, s.images.len, "image", "images");
    widgets.pageHeader(c, "Images", subtitle);
    searchRow(c, &s.search.images, "Search images", 190);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);
    if (s.images.len == 0) {
        widgets.emptyState(c, "doc", "No images", "docker pull alpine");
        return;
    }
    if (shown == 0) {
        widgets.emptyState(c, "doc", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every image");
        return;
    }
    const compact = s.selected != null and s.detail_kind == .image;
    const full_cols = [_]widgets.Col{
        .{ .name = "REPOSITORY:TAG", .width = 300, .key = 1 },
        .{ .name = "ID", .width = 110, .key = 2 },
        .{ .name = "SIZE", .width = 100, .align_end = true, .key = 3 },
        .{ .name = "AGE", .width = 100, .align_end = true, .key = 4 },
        .{ .name = "STATUS", .width = 120, .key = 5 },
    };
    const compact_cols = [_]widgets.Col{
        .{ .name = "REPOSITORY:TAG", .width = 240, .key = 1 },
        .{ .name = "ID", .width = 100, .key = 2 },
        .{ .name = "SIZE", .width = 80, .align_end = true, .key = 3 },
    };
    widgets.tableHeader(c, if (compact) &compact_cols else &full_cols, &s.sort_images);

    var rows = ui.list(c, .{ .count = shown, .row_height = ROW });
    while (rows.next()) |i| {
        const im = s.images[order[i]];
        const is_selected = s.detail_kind == .image and s.selected != null and std.mem.eql(u8, s.selected.?, im.Id);
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) s.selectImage(&im);
        const label_w: f32 = if (compact) 240 else 300;
        if (im.dangling()) {
            widgets.cellMuted(c, im.label(), label_w, false);
        } else {
            widgets.cell(c, im.label(), label_w, false);
        }
        const id = im.Id;
        const short = if (id.len > 19) id[7..19] else id;
        widgets.cellMuted(c, short, if (compact) 100 else 110, false);
        widgets.cell(c, format.bytes(c, im.Size), if (compact) 80 else 100, true);
        if (!compact) {
            widgets.cellMuted(c, format.ago(c, im.Created, format.now()), 100, true);
            // A fixed slot: an intrinsic-width pill shifts everything after it.
            widgets.pillCell(c, if (im.dangling()) "dangling" else "ready", 120);
        }
    }
    rows.end();
}

/// The row above every list that has no filter chips: the search field on the
/// left, so an empty list keeps it too.
fn searchRow(c: *ui.Context, buffer: *ui.TextBuffer, placeholder: []const u8, width: f32) void {
    const row = ui.div(c).row().wFull().gap(6).pb(8);
    _ = search.box(c, buffer, placeholder, width);
    row.end();
}


pub fn volumes(s: *app.State, c: *ui.Context) void {
    const q = c.alloc().dupe(u8, s.search.volumes.text()) catch "";
    const order = c.alloc().alloc(usize, s.volumes.len) catch return;
    var shown: usize = 0;
    for (s.volumes, 0..) |v, i| {
        if (!search.keepVolume(q, v)) continue;
        order[shown] = i;
        shown += 1;
    }
    std.mem.sort(usize, order[0..shown], VolumeSort{
        .items = s.volumes,
        .key = s.sort_volumes.key,
        .desc = s.sort_volumes.desc,
    }, VolumeSort.lessThan);

    const subtitle = if (q.len > 0)
        c.print("{d} of {d} shown", .{ shown, s.volumes.len })
    else
        widgets.count(c, s.volumes.len, "volume", "volumes");
    widgets.pageHeader(c, "Volumes", subtitle);
    searchRow(c, &s.search.volumes, "Search volumes", 190);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);
    if (s.volumes.len == 0) {
        widgets.emptyState(c, "folder", "No volumes", "Volumes appear here once a container declares one");
        return;
    }
    if (shown == 0) {
        widgets.emptyState(c, "folder", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every volume");
        return;
    }
    const compact = s.selected != null and s.detail_kind == .volume;
    const full_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 260, .key = 1 },
        .{ .name = "DRIVER", .width = 100, .key = 2 },
        .{ .name = "SIZE", .width = 100, .align_end = true, .key = 3 },
        .{ .name = "IN USE", .width = 90, .align_end = true, .key = 4 },
        .{ .name = "MOUNTPOINT", .width = 260, .key = 5 },
    };
    const compact_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 180, .key = 1 },
        .{ .name = "DRIVER", .width = 70, .key = 2 },
        .{ .name = "MOUNTPOINT", .width = 180, .key = 5 },
    };
    widgets.tableHeader(c, if (compact) &compact_cols else &full_cols, &s.sort_volumes);

    var rows = ui.list(c, .{ .count = shown, .row_height = ROW });
    while (rows.next()) |i| {
        const v = s.volumes[order[i]];
        const is_selected = s.detail_kind == .volume and s.selected != null and std.mem.eql(u8, s.selected.?, v.Name);
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) s.selectVolume(&v);
        widgets.cell(c, v.Name, if (compact) 180 else 260, false);
        widgets.cellMuted(c, v.Driver, if (compact) 70 else 100, false);
        if (!compact) {
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
        }
        widgets.cellMuted(c, v.Mountpoint, if (compact) 180 else 260, false);
    }
    rows.end();
}


pub fn networks(s: *app.State, c: *ui.Context) void {
    const q = c.alloc().dupe(u8, s.search.networks.text()) catch "";
    const order = c.alloc().alloc(usize, s.networks.len) catch return;
    var shown: usize = 0;
    for (s.networks, 0..) |n, i| {
        if (!search.keepNetwork(q, n)) continue;
        order[shown] = i;
        shown += 1;
    }
    std.mem.sort(usize, order[0..shown], NetworkSort{
        .items = s.networks,
        .containers = s.containers,
        .key = s.sort_networks.key,
        .desc = s.sort_networks.desc,
    }, NetworkSort.lessThan);

    const subtitle = if (q.len > 0)
        c.print("{d} of {d} shown", .{ shown, s.networks.len })
    else
        widgets.count(c, s.networks.len, "network", "networks");
    widgets.pageHeader(c, "Networks", subtitle);
    searchRow(c, &s.search.networks, "Search networks", 190);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);
    if (s.networks.len == 0) {
        widgets.emptyState(c, "network", "No networks", "");
        return;
    }
    if (shown == 0) {
        widgets.emptyState(c, "network", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every network");
        return;
    }
    const compact = s.selected != null and s.detail_kind == .network;
    const full_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 240, .key = 1 },
        .{ .name = "DRIVER", .width = 100, .key = 2 },
        .{ .name = "SCOPE", .width = 90, .key = 3 },
        .{ .name = "ATTACHED", .width = 90, .align_end = true, .key = 4 },
    };
    const compact_cols = [_]widgets.Col{
        .{ .name = "NAME", .width = 180, .key = 1 },
        .{ .name = "DRIVER", .width = 80, .key = 2 },
        .{ .name = "SCOPE", .width = 70, .key = 3 },
        .{ .name = "ATTACHED", .width = 70, .align_end = true, .key = 4 },
    };
    widgets.tableHeader(c, if (compact) &compact_cols else &full_cols, &s.sort_networks);

    var rows = ui.list(c, .{ .count = shown, .row_height = ROW });
    while (rows.next()) |i| {
        const n = s.networks[order[i]];
        const is_selected = s.detail_kind == .network and s.selected != null and
            (std.mem.eql(u8, s.selected.?, n.Id) or std.mem.eql(u8, s.selected.?, n.Name));
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(8).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) s.selectNetwork(&n);
        const dim = n.builtin();
        const name_w: f32 = if (compact) 180 else 240;
        const driver_w: f32 = if (compact) 80 else 100;
        const scope_w: f32 = if (compact) 70 else 90;
        if (dim) {
            widgets.cellMuted(c, n.Name, name_w, false);
            // The API reports an absent driver as the string "null".
            widgets.cellMuted(c, if (std.mem.eql(u8, n.Driver, "null")) "—" else n.Driver, driver_w, false);
            widgets.cellMuted(c, n.Scope, scope_w, false);
        } else {
            widgets.cell(c, n.Name, name_w, false);
            widgets.cellMuted(c, n.Driver, driver_w, false);
            widgets.cellMuted(c, n.Scope, scope_w, false);
        }
        widgets.cellMuted(c, c.print("{d}", .{api.attachedCount(s.containers, n.Name)}), if (compact) 70 else 90, true);
    }
    rows.end();
}


test "container sorting is case-insensitive and reversible" {
    const items = [_]api.Container{
        .{ .Id = "b", .Names = &.{"/Web"}, .Image = "nginx", .State = "running" },
        .{ .Id = "a", .Names = &.{"/api"}, .Image = "alpine", .State = "exited" },
    };
    const name_asc = ContainerSort{ .s = undefined, .key = 1, .desc = false };
    const name_desc = ContainerSort{ .s = undefined, .key = 1, .desc = true };
    try std.testing.expect(name_asc.lessThan(&items[1], &items[0]));
    try std.testing.expect(name_desc.lessThan(&items[0], &items[1]));

    const image_asc = ContainerSort{ .s = undefined, .key = 2, .desc = false };
    try std.testing.expect(image_asc.lessThan(&items[1], &items[0]));

    const state_asc = ContainerSort{ .s = undefined, .key = 3, .desc = false };
    try std.testing.expect(state_asc.lessThan(&items[1], &items[0]));
}

test "image sorting follows tag, size and dangling state" {
    const items = [_]api.Image{
        .{ .Id = "2", .RepoTags = &.{"nginx:alpine"}, .Size = 90 },
        .{ .Id = "1", .RepoTags = &.{"alpine:3.22"}, .Size = 12 },
        .{ .Id = "3", .RepoTags = &.{}, .Size = 1 },
    };
    const tag = ImageSort{ .items = items[0..], .key = 1, .desc = false };
    try std.testing.expect(tag.lessThan(1, 0)); // alpine < nginx
    try std.testing.expect(tag.lessThan(0, 2)); // nginx < untagged
    try std.testing.expect(!tag.lessThan(2, 0));

    const size = ImageSort{ .items = items[0..], .key = 3, .desc = false };
    try std.testing.expect(size.lessThan(2, 1)); // 1 B < 12
    try std.testing.expect(size.lessThan(1, 0)); // 12 < 90

    const status = ImageSort{ .items = items[0..], .key = 5, .desc = false };
    try std.testing.expect(status.lessThan(2, 0)); // dangling < ready
}

test "volume sorting handles size, use and missing usage data" {
    const items = [_]api.Volume{
        .{ .Name = "b", .Driver = "local", .Mountpoint = "/b", .UsageData = .{ .Size = 20, .RefCount = 1 } },
        .{ .Name = "a", .Driver = "nfs", .Mountpoint = "/a", .UsageData = .{ .Size = 10, .RefCount = 5 } },
        .{ .Name = "c", .Driver = "local", .Mountpoint = "/c" },
    };
    const name = VolumeSort{ .items = items[0..], .key = 1, .desc = false };
    try std.testing.expect(name.lessThan(1, 0));
    const size = VolumeSort{ .items = items[0..], .key = 3, .desc = false };
    try std.testing.expect(size.lessThan(1, 0)); // 10 < 20
    try std.testing.expect(size.lessThan(2, 1)); // -1 (unknown) < 10
    const used = VolumeSort{ .items = items[0..], .key = 4, .desc = false };
    try std.testing.expect(used.lessThan(0, 1)); // 1 < 5
}

test "network sorting follows name, driver and scope" {
    const items = [_]api.Network{
        .{ .Name = "shop", .Driver = "bridge", .Scope = "local" },
        .{ .Name = "bridge", .Driver = "bridge", .Scope = "local" },
        .{ .Name = "host", .Driver = "host", .Scope = "local" },
    };
    const name = NetworkSort{ .items = items[0..], .containers = &.{}, .key = 1, .desc = false };
    try std.testing.expect(name.lessThan(1, 0));
    const driver = NetworkSort{ .items = items[0..], .containers = &.{}, .key = 2, .desc = false };
    try std.testing.expect(driver.lessThan(0, 2)); // bridge < host
    const desc = NetworkSort{ .items = items[0..], .containers = &.{}, .key = 1, .desc = true };
    try std.testing.expect(desc.lessThan(0, 1));
}
