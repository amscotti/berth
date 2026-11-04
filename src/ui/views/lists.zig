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
const metrics = @import("metrics.zig");
const columns = @import("columns.zig");
const bulk = @import("../../bulk.zig");
const create = @import("create.zig");
const create_mod = @import("../../create.zig");
const sorting = @import("sorting.zig");
const shell = @import("../shell.zig");
const theme = @import("../theme.zig");

const ROW = 44.0;

fn loaded(s: *app.State) bool {
    // A failed first pass is still a finished pass: show its error, not a
    // skeleton that would never end.
    return widgets.firstRead(s.last_update, s.error_text) != .reading;
}

/// The search field shrinks with the room the table has. At the smallest size
/// the placeholder becomes one word rather than a clipped phrase.
fn searchWidth(content: f32) f32 {
    if (content >= 760) return 190;
    if (content >= 520) return 160;
    return 132;
}

fn searchPlaceholder(c: *ui.Context, noun: []const u8, width: f32) []const u8 {
    if (width < 150) return "Search";
    return c.print("Search {s}", .{noun});
}

pub fn containers(s: *app.State, c: *ui.Context) void {
    if (!loaded(s)) {
        _ = widgets.pageHeaderSearch(c, "Containers", "Reading engine…", null);
        widgets.loadingRows(c, 6);
        return;
    }

    // The rows this filter keeps, chosen once so the count and the list agree.
    // The search query ANDs with the chip: both narrow what is on screen.
    const q = c.alloc().dupe(u8, s.search.containers.text()) catch "";
    var shown: [512]*const api.Container = undefined;
    var n: usize = 0;
    for (s.containers) |*ct| {
        if (!s.filter.keeps(ct)) continue;
        if (!search.keepContainer(q, ct)) continue;
        if (n == shown.len) break;
        shown[n] = ct;
        n += 1;
    }

    // "N of M shown" only when something is actually hidden; otherwise the
    // running/total summary is the more useful sentence.
    const filtered = s.filter != .all or q.len > 0;
    const subtitle = if (filtered and n < s.containers.len)
        c.print("{d} of {d} shown", .{ n, s.containers.len })
    else
        c.print("{d} running · {d} total", .{ app.runningCount(s), s.containers.len });

    const width = s.layout.content;
    const box_w = searchWidth(width);
    if (widgets.pageHeaderSearch(c, "Containers", subtitle, .{
        .buffer = &s.search.containers,
        .placeholder = searchPlaceholder(c, "containers", box_w),
        .width = box_w,
    }).cleared) s.redraw();

    // Filter chips, in the same place and shape on every view that grows one.
    const chips = ui.div(c).row().wFull().gap(6).pb(8);
    for ([_]app.Filter{ .all, .running, .stopped }) |option| {
        const on = s.filter == option;
        const chip = ui.div(c).row().h(24).px(10).itemsCenter().rounded(12)
            .bg(if (on) c.theme.accent.alpha(0.16) else c.theme.surface_hover);
        defer chip.end();
        ui.text(c, option.label(), .{
            .size = 11.5,
            .weight = if (on) .medium else .regular,
            .color = if (on) theme.accentText(c.theme) else c.theme.text,
        });
        if (chip.clicked()) {
            s.filter = option;
            s.redraw();
        }
    }
    const select_chip = ui.div(c).row().h(24).px(10).itemsCenter().rounded(12)
        .bg(if (s.bulk.enabled) c.theme.accent.alpha(0.16) else c.theme.surface_hover);
    ui.text(c, "Select", .{
        .size = 11.5,
        .weight = if (s.bulk.enabled) .medium else .regular,
        .color = if (s.bulk.enabled) theme.accentText(c.theme) else c.theme.text,
    });
    if (select_chip.clicked() and !s.bulk.running()) {
        s.bulk.toggleMode();
        s.clearPending();
        s.redraw();
    }
    select_chip.end();
    chips.end();

    if (s.bulk.enabled or s.bulk.running()) bulkBar(s, c);

    if (s.error_text.len > 0 and s.containers.len == 0) widgets.errorBanner(c, s.error_text);

    if (n == 0) {
        if (s.containers.len == 0) {
            if (s.error_text.len > 0) {
                _ = widgets.emptyStateAction(c, "cube", "Engine not answering", widgets.engine_help, null);
            } else {
                _ = widgets.emptyStateAction(c, "cube", "No containers yet", "docker run --rm -it alpine sh", null);
            }
        } else if (q.len > 0) {
            if (widgets.emptyStateAction(c, "cube", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every container", "Clear search")) {
                s.search.containers.clear();
                s.redraw();
            }
        } else {
            if (widgets.emptyStateAction(c, "cube", "None match this filter", "All shows every container", "Show all")) {
                s.filter = .all;
                s.redraw();
            }
        }
        return;
    }

    // Only when something is actually grouped: a column of dashes costs width
    // that the ports column needs, and says nothing.
    var any_project = false;
    for (s.containers) |ct| {
        if (ct.project() != null) any_project = true;
    }
    std.mem.sort(*const api.Container, shown[0..n], sorting.ContainerSort{
        .s = s,
        .key = s.sort_containers.key,
        .desc = s.sort_containers.desc,
    }, sorting.ContainerSort.lessThan);

    var col_buf: [11]widgets.Col = undefined;
    const cols = columns.containers(width, any_project, s.bulk.enabled, &col_buf);
    widgets.tableHeader(c, cols.header, &s.sort_containers);

    var rows = ui.list(c, .{ .count = n, .row_height = ROW });
    while (rows.next()) |i| {
        const ct = shown[i].*;
        const is_selected = s.selected != null and std.mem.eql(u8, s.selected.?, ct.Id);
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(10).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) {
            if (s.bulk.enabled and s.bulk.pending == null) {
                s.bulk.toggle(ct.Id);
                s.redraw();
            } else s.select(&ct);
        }

        if (cols.select) checkboxCell(s, c, &ct, cols.check);
        metrics.nameCell(c, &ct, cols.name);
        widgets.pillCell(c, ct.health() orelse ct.State, cols.state);
        const st = app.statFor(s, ct.Id);
        // A stopped container has no stats at all: say so with a dash rather
        // than a zero, which would read as "idle" instead of "unknown". A
        // paused one still reports — frozen, but real.
        if (ct.isUp() and st.sampled and st.cpu_known) {
            widgets.cell(c, format.percent(c, st.cpu), cols.cpu, true);
        } else {
            widgets.cellMuted(c, "—", cols.cpu, true);
        }
        if (cols.mem > 0) {
            if (ct.isUp() and st.sampled) {
                widgets.cell(c, format.bytes(c, st.mem), cols.mem, true);
            } else {
                widgets.cellMuted(c, "—", cols.mem, true);
            }
        }
        if (cols.net > 0) {
            const gap = ui.div(c).w(6);
            gap.end();
            metrics.netCell(c, &ct, st, cols.net);
        }
        if (cols.disk > 0) metrics.diskCell(c, &ct, st, cols.disk);
        if (cols.pids > 0) {
            const gap = ui.div(c).w(6);
            gap.end();
            metrics.pidsCell(c, &ct, st, cols.pids);
        }
        if (cols.uptime > 0) metrics.uptimeCell(c, &ct, cols.uptime);
        if (cols.project > 0) widgets.cellMuted(c, ct.project() orelse "—", cols.project, false);
        if (cols.ports > 0) {
            // A pill per published port; clicking one opens it in a browser.
            if (widgets.portPills(c, &ct, 2, cols.ports, false)) |binding| s.openPort(binding);
        }
        ui.spacer(c);
        actions(s, c, &ct, cols.actions);
    }
    rows.end();
}

fn bulkBar(s: *app.State, c: *ui.Context) void {
    const bar = ui.div(c).row().wFull().itemsCenter().gap(6).pb(8);
    if (s.bulk.running()) {
        const label = if (s.bulk.canceling) "Canceling…" else c.print("{s} containers…", .{bulk.runningLabel(&s.bulk)});
        ui.text(c, label, .{
            .size = 11.5,
            .weight = .medium,
            .color = theme.warning(c.theme),
        });
        if (!s.bulk.canceling) {
            ui.spacer(c);
            if (smallButton(c, "Cancel", false)) bulk.cancel(s);
        }
        bar.end();
        return;
    }
    ui.text(c, c.print("{d} selected", .{s.bulk.count}), .{
        .size = 11.5,
        .weight = .medium,
        .color = if (s.bulk.count > 0) c.theme.text else c.theme.muted,
    });
    // The list already applied the filter and the search; asking it again
    // here keeps the shortcut honest when a query is in the field. A frozen
    // confirmation already names its set, so the bar must not change it.
    if (s.bulk.pending == null) {
        if (smallButton(c, "Select all shown", false)) {
            s.bulk.selectShown(s.containers, s.filter, s.search.containers.text());
            s.redraw();
        }
        if (smallButton(c, "None", false)) {
            s.bulk.selectNone();
            s.redraw();
        }
    }
    ui.spacer(c);
    if (s.bulk.count == 0) {
        ui.text(c, "check rows to start, stop or delete them together", .{
            .size = 11,
            .color = c.theme.muted,
            .truncate = .ellipsis,
        });
    } else {
        if (smallButton(c, "Start", false)) bulk.startSelected(s);
        if (smallButton(c, "Stop", false)) bulk.stopSelected(s);
        if (smallButton(c, "Delete", true)) bulk.deleteSelected(s);
        if (smallButton(c, "Cancel", false)) {
            s.bulk.clear();
            s.redraw();
        }
    }
    bar.end();
}

fn checkboxCell(s: *app.State, c: *ui.Context, ct: *const api.Container, width: f32) void {
    const slot = ui.div(c).row().w(width).h(24).itemsCenter();
    const on = s.bulk.has(ct.Id);
    // A confirmation freezes ticks; dim the boxes so the no-op reads as
    // deliberate rather than broken.
    const frozen = s.bulk.pending != null;
    const tint: f32 = if (frozen) 0.4 else 1;
    const fill = if (on) c.theme.accent else c.theme.surface;
    const edge = if (on) c.theme.accent else c.theme.border;
    const box = ui.div(c).row().w(18).h(18).center().rounded(5)
        .bg(fill.alpha(tint))
        .border(edge.alpha(tint)).control();
    if (on) ui.icon(c, "checkmark", .{ .size = 11, .color = c.theme.accent_text.alpha(tint) });
    if (box.clicked() and !frozen) {
        s.bulk.toggle(ct.Id);
        s.redraw();
    }
    box.end();
    slot.end();
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
    const color = if (danger) theme.dangerText(c.theme) else c.theme.text;
    const box = ui.div(c).row().h(20).px(7).itemsCenter().rounded(10).bg(c.theme.surface_hover);
    defer box.end();
    ui.text(c, label, .{ .size = 10.5, .color = color });
    return box.clicked();
}

pub fn images(s: *app.State, c: *ui.Context) void {
    if (!loaded(s)) {
        _ = widgets.pageHeaderSearch(c, "Images", "Reading engine…", null);
        widgets.loadingRows(c, 5);
        return;
    }

    const q = c.alloc().dupe(u8, s.search.images.text()) catch "";
    const order = c.alloc().alloc(usize, s.images.len) catch return;
    var shown: usize = 0;
    for (s.images, 0..) |im, i| {
        if (!search.keepImage(q, im)) continue;
        order[shown] = i;
        shown += 1;
    }
    std.mem.sort(usize, order[0..shown], sorting.ImageSort{
        .items = s.images,
        .key = s.sort_images.key,
        .desc = s.sort_images.desc,
    }, sorting.ImageSort.lessThan);

    const subtitle = if (q.len > 0 and shown < s.images.len)
        c.print("{d} of {d} shown", .{ shown, s.images.len })
    else
        widgets.count(c, s.images.len, "image", "images");
    const width = s.layout.content;
    const box_w = searchWidth(width);
    if (widgets.pageHeaderSearch(c, "Images", subtitle, .{
        .buffer = &s.search.images,
        .placeholder = searchPlaceholder(c, "images", box_w),
        .width = box_w,
    }).cleared) s.redraw();
    create.card(s, c);
    // Air between the creation card and the table, so the two read as
    // separate surfaces rather than one continuous block.
    ui.div(c).h(8).end();

    if (s.error_text.len > 0 and s.images.len == 0) widgets.errorBanner(c, s.error_text);
    if (s.images.len == 0) {
        if (s.error_text.len > 0) {
            _ = widgets.emptyStateAction(c, "doc", "Engine not answering", widgets.engine_help, null);
        } else {
            _ = widgets.emptyStateAction(c, "doc", "No images", "Pull a reference in the card above", null);
        }
        return;
    }
    if (shown == 0) {
        if (widgets.emptyStateAction(c, "doc", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every image", "Clear search")) {
            s.search.images.clear();
            s.redraw();
        }
        return;
    }

    var col_buf: [5]widgets.Col = undefined;
    const cols = columns.images(width, &col_buf);
    widgets.tableHeader(c, cols.header, &s.sort_images);

    var rows = ui.list(c, .{ .count = shown, .row_height = ROW });
    while (rows.next()) |i| {
        const im = s.images[order[i]];
        const is_selected = s.detail_kind == .image and s.selected != null and std.mem.eql(u8, s.selected.?, im.Id);
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(10).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) s.selectImage(&im);
        if (im.dangling()) {
            widgets.cellMuted(c, im.label(), cols.repo, false);
        } else {
            widgets.cell(c, im.label(), cols.repo, false);
        }
        const id = im.Id;
        const short = if (id.len > 19) id[7..19] else id;
        widgets.cellMuted(c, short, cols.id, false);
        widgets.cell(c, format.bytes(c, im.Size), cols.size, true);
        if (cols.age > 0) {
            widgets.cellMuted(c, format.ago(c, im.Created, format.now()), cols.age, true);
            if (cols.status_gap > 0) {
                const gap = ui.div(c).w(cols.status_gap);
                gap.end();
            }
            // A fixed slot: an intrinsic-width pill shifts everything after it.
            widgets.pillCell(c, if (im.dangling()) "dangling" else "ready", cols.status);
        }
    }
    rows.end();
}

pub fn volumes(s: *app.State, c: *ui.Context) void {
    if (!loaded(s)) {
        _ = widgets.pageHeaderSearch(c, "Volumes", "Reading engine…", null);
        widgets.loadingRows(c, 5);
        return;
    }

    const q = c.alloc().dupe(u8, s.search.volumes.text()) catch "";
    const order = c.alloc().alloc(usize, s.volumes.len) catch return;
    var shown: usize = 0;
    for (s.volumes, 0..) |v, i| {
        if (!search.keepVolume(q, v)) continue;
        order[shown] = i;
        shown += 1;
    }
    std.mem.sort(usize, order[0..shown], sorting.VolumeSort{
        .items = s.volumes,
        .key = s.sort_volumes.key,
        .desc = s.sort_volumes.desc,
    }, sorting.VolumeSort.lessThan);

    const subtitle = if (q.len > 0 and shown < s.volumes.len)
        c.print("{d} of {d} shown", .{ shown, s.volumes.len })
    else
        widgets.count(c, s.volumes.len, "volume", "volumes");
    const width = s.layout.content;
    const box_w = searchWidth(width);
    const header = widgets.pageHeaderSearch(c, "Volumes", subtitle, .{
        .buffer = &s.search.volumes,
        .placeholder = searchPlaceholder(c, "volumes", box_w),
        .width = box_w,
        .action_label = "New volume",
        .action_icon = "plus",
    });
    if (header.action) create_mod.open(s, .volume);
    if (header.cleared) s.redraw();

    if (s.error_text.len > 0 and s.volumes.len == 0) widgets.errorBanner(c, s.error_text);
    if (s.volumes.len == 0) {
        if (s.error_text.len > 0) {
            _ = widgets.emptyStateAction(c, "folder", "Engine not answering", widgets.engine_help, null);
        } else {
            _ = widgets.emptyStateAction(c, "folder", "No volumes", "Create one above, or run a container that declares one", null);
        }
        return;
    }
    if (shown == 0) {
        if (widgets.emptyStateAction(c, "folder", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every volume", "Clear search")) {
            s.search.volumes.clear();
            s.redraw();
        }
        return;
    }

    var col_buf: [5]widgets.Col = undefined;
    const cols = columns.volumes(width, &col_buf);
    widgets.tableHeader(c, cols.header, &s.sort_volumes);

    var rows = ui.list(c, .{ .count = shown, .row_height = ROW });
    while (rows.next()) |i| {
        const v = s.volumes[order[i]];
        const is_selected = s.detail_kind == .volume and s.selected != null and std.mem.eql(u8, s.selected.?, v.Name);
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(10).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) s.selectVolume(&v);
        widgets.cell(c, v.Name, cols.name, false);
        widgets.cellMuted(c, v.Driver, cols.driver, false);
        if (cols.size > 0) {
            if (v.UsageData) |u| {
                if (u.Size >= 0) {
                    widgets.cell(c, format.bytes(c, @as(u64, @intCast(u.Size))), cols.size, true);
                } else {
                    widgets.cellMuted(c, "—", cols.size, true);
                }
                widgets.cell(c, c.print("{d}", .{u.RefCount}), cols.in_use, true);
            } else {
                widgets.cellMuted(c, "—", cols.size, true);
                widgets.cellMuted(c, "—", cols.in_use, true);
            }
        }
        if (cols.mount_gap > 0) {
            const gap = ui.div(c).w(cols.mount_gap);
            gap.end();
        }
        widgets.cellMuted(c, v.Mountpoint, cols.mount, false);
    }
    rows.end();
}

pub fn networks(s: *app.State, c: *ui.Context) void {
    if (!loaded(s)) {
        _ = widgets.pageHeaderSearch(c, "Networks", "Reading engine…", null);
        widgets.loadingRows(c, 5);
        return;
    }

    const q = c.alloc().dupe(u8, s.search.networks.text()) catch "";
    const order = c.alloc().alloc(usize, s.networks.len) catch return;
    var shown: usize = 0;
    for (s.networks, 0..) |n, i| {
        if (!search.keepNetwork(q, n)) continue;
        order[shown] = i;
        shown += 1;
    }
    std.mem.sort(usize, order[0..shown], sorting.NetworkSort{
        .items = s.networks,
        .containers = s.containers,
        .key = s.sort_networks.key,
        .desc = s.sort_networks.desc,
    }, sorting.NetworkSort.lessThan);

    const subtitle = if (q.len > 0 and shown < s.networks.len)
        c.print("{d} of {d} shown", .{ shown, s.networks.len })
    else
        widgets.count(c, s.networks.len, "network", "networks");
    const width = s.layout.content;
    const box_w = searchWidth(width);
    const header = widgets.pageHeaderSearch(c, "Networks", subtitle, .{
        .buffer = &s.search.networks,
        .placeholder = searchPlaceholder(c, "networks", box_w),
        .width = box_w,
        .action_label = "New network",
        .action_icon = "plus",
    });
    if (header.action) create_mod.open(s, .network);
    if (header.cleared) s.redraw();

    if (s.error_text.len > 0 and s.networks.len == 0) widgets.errorBanner(c, s.error_text);
    if (s.networks.len == 0) {
        if (s.error_text.len > 0) {
            _ = widgets.emptyStateAction(c, "network", "Engine not answering", widgets.engine_help, null);
        } else {
            _ = widgets.emptyStateAction(c, "network", "No networks", "Create one above", null);
        }
        return;
    }
    if (shown == 0) {
        if (widgets.emptyStateAction(c, "network", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every network", "Clear search")) {
            s.search.networks.clear();
            s.redraw();
        }
        return;
    }

    const cols = columns.networks(width);
    widgets.tableHeader(c, cols.header, &s.sort_networks);

    var rows = ui.list(c, .{ .count = shown, .row_height = ROW });
    while (rows.next()) |i| {
        const n = s.networks[order[i]];
        const is_selected = s.detail_kind == .network and s.selected != null and
            (std.mem.eql(u8, s.selected.?, n.Id) or std.mem.eql(u8, s.selected.?, n.Name));
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(10).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else if (i % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        defer row.end();
        if (row.clicked()) s.selectNetwork(&n);
        const dim = n.builtin();
        if (dim) {
            widgets.cellMuted(c, n.Name, cols.name, false);
            // The API reports an absent driver as the string "null".
            widgets.cellMuted(c, if (std.mem.eql(u8, n.Driver, "null")) "—" else n.Driver, cols.driver, false);
            widgets.cellMuted(c, n.Scope, cols.scope, false);
        } else {
            widgets.cell(c, n.Name, cols.name, false);
            widgets.cellMuted(c, n.Driver, cols.driver, false);
            widgets.cellMuted(c, n.Scope, cols.scope, false);
        }
        widgets.cellMuted(c, c.print("{d}", .{api.attachedCount(s.containers, n.Name)}), cols.attached, true);
    }
    rows.end();
}
