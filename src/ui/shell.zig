//! The shell: navigation rail, header, status bar, and the dispatch to a view.
//! Every view is drawn inside this frame, so the chrome never moves.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../app.zig");
const api = @import("../engine/api.zig");
const format = @import("format.zig");
const widgets = @import("widgets.zig");
const theme = @import("theme.zig");
const lists = @import("views/lists.zig");
const panels = @import("views/panels.zig");
const stacks = @import("views/stacks.zig");

const RAIL = 200.0;

const TABS = [_]app.Tab{
    .overview, .containers, .images,  .volumes, .networks,
    .stacks,   .disk,       .runtimes, .settings,
};

pub fn view(s: *app.State, c: *ui.Context) void {
    // Warm paper and amber, derived from the framework's theme each frame so
    // the system's light/dark switch keeps working.
    c.theme = theme.of(c.theme);
    const root = ui.div(c).row().sizeFull().bg(c.theme.background);
    defer root.end();
    rail(s, c);
    const main = ui.div(c).column().grow().hFull().px(20).pt(16).pb(12).gap(4);
    header(s, c);
    if (s.pending) |p| confirmDialog(s, c, p.verb, p.name);
    if (s.prune_open) pruneDialog(s, c);
    body(s, c);
    main.end(); // before the detail pane, or the pane becomes a child of it

    if (s.selected != null) detailPane(s, c);
}

/// The right-hand pane: what this row is, and what can be done to it.
fn detailPane(s: *app.State, c: *ui.Context) void {
    const pane = ui.div(c).column().w(430).hFull().p(14).gap(8).bg(c.theme.surface).border(c.theme.border);
    defer pane.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, s.selected_name, .{ .size = 15, .weight = .semibold, .truncate = .ellipsis });
    ui.spacer(c);
    // Facts or the engine's own JSON, as Docker Desktop's Inspect tab does.
    const facts_tab = ui.div(c).row().h(22).px(8).itemsCenter().rounded(11)
        .bg(if (!s.pane_inspect) c.theme.accent.alpha(0.16) else c.theme.surface_hover);
    ui.text(c, "Facts", .{
        .size = 11,
        .weight = if (!s.pane_inspect) .medium else .regular,
        .color = if (!s.pane_inspect) c.theme.accent else c.theme.muted,
    });
    if (facts_tab.clicked()) {
        s.pane_inspect = false;
        s.redraw();
    }
    facts_tab.end();
    const json_tab = ui.div(c).row().h(22).px(8).itemsCenter().rounded(11)
        .bg(if (s.pane_inspect) c.theme.accent.alpha(0.16) else c.theme.surface_hover);
    ui.text(c, "Inspect", .{
        .size = 11,
        .weight = if (s.pane_inspect) .medium else .regular,
        .color = if (s.pane_inspect) c.theme.accent else c.theme.muted,
    });
    if (json_tab.clicked()) {
        s.pane_inspect = true;
        if (s.inspect_buf == null) s.reloadInspect();
        s.redraw();
    }
    json_tab.end();
    const close = ui.div(c).row().h(22).px(8).itemsCenter().rounded(11).bg(c.theme.surface_hover);
    ui.text(c, "Close", .{ .size = 11, .color = c.theme.muted });
    if (close.clicked()) s.clearSelection();
    close.end();
    head.end();

    if (s.pane_inspect) {
        inspectView(s, c);
        return;
    }

    switch (s.detail_kind) {
        .container => containerDetail(s, c),
        .image => imageDetail(s, c),
        .volume => volumeDetail(s, c),
        .network => networkDetail(s, c),
    }
}

/// The engine's own JSON for the selected object, fetched on demand and owned
/// by the state so a snapshot swap cannot pull it out from under the pane.
fn inspectView(s: *app.State, c: *ui.Context) void {
    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, "Inspect · as the engine sees it", .{ .size = 11, .weight = .medium, .color = c.theme.muted });
    ui.spacer(c);
    const again = ui.div(c).row().h(20).px(7).itemsCenter().rounded(10).bg(c.theme.surface_hover);
    ui.text(c, "Refresh", .{ .size = 10.5, .color = c.theme.muted });
    if (again.clicked()) s.reloadInspect();
    again.end();
    head.end();

    ui.scroll(c, .{ .width = .fill, .height = .fill, .padding = .{ .left = 10, .right = 10, .top = 8, .bottom = 8 }, .radius = 8, .background = c.theme.background });
    defer ui.end(c);
    if (s.inspect_buf) |text| {
        ui.text(c, text, .{
            .size = 10.5,
            .mono = true,
            .wrap = true,
            .leading = 1.35,
            .color = c.theme.text,
        });
    } else {
        ui.text(c, "No inspect JSON yet. Refresh fetches it from the engine.", .{ .size = 11.5, .color = c.theme.muted });
    }
}

fn containerDetail(s: *app.State, c: *ui.Context) void {
    // The facts that answer "what is this thing", from the list entry we have.
    for (s.containers) |ct| {
        if (s.selected == null or !std.mem.eql(u8, s.selected.?, ct.Id)) continue;
        facts(c, "Image", ct.Image);
        facts(c, "State", ct.State);
        facts(c, "Status", ct.Status);
        if (ct.health()) |h| facts(c, "Health", h);
        if (ct.project()) |project| facts(c, "Project", project);
        if (ct.service()) |service| facts(c, "Service", service);
        if (widgets.portsText(c, &ct, 4).len > 0) {
            const ports_row = ui.div(c).row().wFull().itemsCenter().gap(8);
            ui.text(c, "Ports", .{ .size = 11, .color = c.theme.muted });
            if (widgets.portPills(c, &ct, 4, 200)) |binding| s.openPort(binding);
            ui.text(c, widgets.portsText(c, &ct, 4), .{ .size = 11.5, .color = c.theme.muted, .truncate = .ellipsis });
            ports_row.end();
        }
        // Pause sits here rather than in the row: three buttons fit the row's
        // fixed slot, and this one is reversible enough not to need a confirm.
        if (ct.isPaused()) {
            const verbs = ui.div(c).row().wFull().gap(6).pt(2);
            const b = ui.div(c).row().h(24).px(10).itemsCenter().rounded(12).bg(c.theme.surface_hover);
            ui.text(c, "Resume", .{ .size = 11.5, .color = c.theme.text });
            if (b.clicked()) _ = s.actFmt("POST", "/containers/{s}/unpause", ct.Id, "Resumed");
            b.end();
            verbs.end();
        } else if (ct.isRunning()) {
            const verbs = ui.div(c).row().wFull().gap(6).pt(2);
            const b = ui.div(c).row().h(24).px(10).itemsCenter().rounded(12).bg(c.theme.surface_hover);
            ui.text(c, "Pause", .{ .size = 11.5, .color = c.theme.text });
            if (b.clicked()) _ = s.actFmt("POST", "/containers/{s}/pause", ct.Id, "Paused");
            b.end();
            verbs.end();
        }
        break;
    }

    const logs_head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, "Logs · last 200 lines", .{ .size = 11, .weight = .medium, .color = c.theme.muted });
    ui.spacer(c);
    const again = ui.div(c).row().h(20).px(7).itemsCenter().rounded(10).bg(c.theme.surface_hover);
    ui.text(c, "Refresh", .{ .size = 10.5, .color = c.theme.muted });
    if (again.clicked()) s.reloadLogs();
    again.end();
    logs_head.end();

    ui.scroll(c, .{ .width = .fill, .height = .fill, .padding = .{ .left = 10, .right = 10, .top = 8, .bottom = 8 }, .radius = 8, .background = c.theme.background });
    defer ui.end(c);
    if (s.log_text.len == 0) {
        ui.text(c, "No output yet. A container that has said nothing says nothing.", .{ .size = 11.5, .color = c.theme.muted });
    } else {
        ui.text(c, s.log_text, .{
            .size = 11.5,
            .mono = true,
            .wrap = true,
            .leading = 1.4,
            .color = c.theme.text,
        });
    }
}

fn imageDetail(s: *app.State, c: *ui.Context) void {
    for (s.images) |im| {
        if (s.selected == null or !std.mem.eql(u8, s.selected.?, im.Id)) continue;
        facts(c, "Repository", if (im.dangling()) "untagged" else im.tag());
        facts(c, "ID", shortId(im.Id));
        facts(c, "Size", format.bytes(c, im.Size));
        facts(c, "Created", format.ago(c, im.Created, format.now()));
        facts(c, "Status", if (im.dangling()) "dangling" else "ready");
        action(s, c, "Delete image", true, im.Id, if (im.dangling()) "untagged" else im.tag(), "Delete", "DELETE", "/images/{s}?force=1");
        break;
    }
}

fn volumeDetail(s: *app.State, c: *ui.Context) void {
    for (s.volumes) |v| {
        if (s.selected == null or !std.mem.eql(u8, s.selected.?, v.Name)) continue;
        facts(c, "Name", v.Name);
        facts(c, "Driver", v.Driver);
        facts(c, "Scope", v.Scope);
        facts(c, "Mountpoint", v.Mountpoint);
        if (v.UsageData) |u| {
            facts(c, "Size", if (u.Size >= 0) format.bytes(c, @as(u64, @intCast(u.Size))) else "—");
            facts(c, "In use", c.print("{d}", .{u.RefCount}));
        }
        action(s, c, "Remove volume", true, v.Name, v.Name, "Remove", "DELETE", "/volumes/{s}");
        break;
    }
}

fn networkDetail(s: *app.State, c: *ui.Context) void {
    for (s.networks) |n| {
        if (s.selected == null or (!std.mem.eql(u8, s.selected.?, n.Id) and !std.mem.eql(u8, s.selected.?, n.Name))) continue;
        facts(c, "Name", n.Name);
        facts(c, "ID", shortId(n.Id));
        facts(c, "Driver", if (std.mem.eql(u8, n.Driver, "null")) "—" else n.Driver);
        facts(c, "Scope", n.Scope);
        facts(c, "Attached", c.print("{d}", .{api.attachedCount(s.containers, n.Name)}));
        if (n.builtin()) {
            ui.text(c, "Built-in networks cannot be removed.", .{ .size = 11.5, .color = c.theme.muted });
        } else {
            action(s, c, "Remove network", true, if (n.Id.len > 0) n.Id else n.Name, n.Name, "Remove", "DELETE", "/networks/{s}");
        }
        break;
    }
}

fn shortId(id: []const u8) []const u8 {
    if (std.mem.startsWith(u8, id, "sha256:")) {
        const rest = id["sha256:".len..];
        return if (rest.len > 12) rest[0..12] else rest;
    }
    return if (id.len > 12) id[0..12] else id;
}

fn action(s: *app.State, c: *ui.Context, label: []const u8, danger: bool, id: []const u8, name: []const u8, verb: []const u8, method: []const u8, comptime path_fmt: []const u8) void {
    const color = if (danger) c.theme.danger else c.theme.accent;
    const box = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(color.alpha(0.14));
    defer box.end();
    ui.text(c, label, .{ .size = 12, .weight = .medium, .color = color });
    if (box.clicked()) s.confirm(id, name, verb, method, path_fmt);
}

fn facts(c: *ui.Context, label: []const u8, value: []const u8) void {
    const row = ui.div(c).row().wFull().gap(8).itemsCenter();
    defer row.end();
    ui.text(c, label, .{ .size = 11, .color = c.theme.muted });
    ui.text(c, value, .{ .size = 11.5, .truncate = .middle });
}

fn rail(s: *app.State, c: *ui.Context) void {
    const rail_box = ui.div(c).column().w(RAIL).hFull().py(14).px(10).gap(2).bg(c.theme.surface).border(c.theme.border);
    defer rail_box.end();

    const brand = ui.div(c).row().itemsCenter().gap(8).px(6).pb(14);
    ui.icon(c, "cube", .{ .size = 18, .color = c.theme.accent });
    ui.text(c, "berth", .{ .size = 16, .weight = .semibold });
    brand.end(); // explicitly, because the nav items follow it in this column

    // A runtime loop, not `inline for`: inside an inline for the deferred
    // `.end()` calls all run at the end of the unrolled block, nesting every
    // nav item inside the first one.
    for (TABS) |tab| {
        if (tab == .containers) section(c, "Resources");
        if (tab == .disk) section(c, "Engine");
        railItem(s, c, tab);
    }

    ui.spacer(c);
    railStatus(s, c);
}

fn section(c: *ui.Context, name: []const u8) void {
    const row = ui.div(c).row().wFull().px(8).pt(12).pb(3);
    defer row.end();
    ui.text(c, name, .{ .size = 10.5, .weight = .semibold, .color = c.theme.muted.alpha(0.75) });
}

fn railItem(s: *app.State, c: *ui.Context, tab: app.Tab) void {
    const selected = s.tab == tab;
    const item = ui.div(c)
        .row()
        .wFull()
        .h(32)
        .itemsCenter()
        .gap(8)
        .px(6)
        .rounded(7)
        .bg(if (selected) c.theme.accent else c.theme.surface);
    if (item.clicked()) {
        s.tab = tab;
        s.redraw();
    }
    defer item.end();

    const tint = theme.tint(c.theme, tabKind(tab));
    const chip = ui.div(c).row().w(20).h(20).center().rounded(5)
        .bg(if (selected) c.theme.accent_text.alpha(0.16) else tint.alpha(0.14));
    ui.icon(c, tab.icon(), .{ .size = 12, .color = if (selected) c.theme.accent_text else tint });
    chip.end(); // before the label, or the label becomes a child of the 20pt chip

    ui.text(c, tab.label(), .{
        .size = 13,
        .weight = if (selected) .semibold else .regular,
        .color = if (selected) c.theme.accent_text else c.theme.text,
    });
    ui.spacer(c);
    const figure = railFigure(s, c, tab);
    if (figure.len > 0) ui.text(c, figure, .{
        .size = 10.5,
        .mono = true,
        .color = if (selected) c.theme.accent_text.alpha(0.8) else c.theme.muted,
    });
}

/// The number a page would put in its own header, beside its name: the rail
/// becomes the monitor's spine rather than a list of links.
fn railFigure(s: *app.State, c: *ui.Context, tab: app.Tab) []const u8 {
    return switch (tab) {
        .overview => "",
        .containers => c.print("{d}/{d}", .{ app.runningCount(s), s.containers.len }),
        .images => c.print("{d}", .{s.images.len}),
        .volumes => c.print("{d}", .{s.volumes.len}),
        .networks => c.print("{d}", .{s.networks.len}),
        .stacks => c.print("{d}", .{projectCount(s)}),
        .disk => if (storeBytes(s)) |n| format.bytes(c, n) else "—",
        .runtimes => c.print("{d}/{d}", .{ reachableCount(s), s.runtimes.len }),
        .settings => "",
    };
}

fn projectCount(s: *app.State) usize {
    var n: usize = 0;
    for (s.containers, 0..) |ct, i| {
        const project = ct.project() orelse continue;
        var seen = false;
        for (s.containers[0..i]) |earlier| {
            const other = earlier.project() orelse continue;
            if (std.mem.eql(u8, project, other)) {
                seen = true;
                break;
            }
        }
        if (!seen) n += 1;
    }
    return n;
}

fn reachableCount(s: *app.State) usize {
    var n: usize = 0;
    for (s.runtimes) |rt| {
        if (rt.reachable) n += 1;
    }
    return n;
}

fn storeBytes(s: *app.State) ?u64 {
    const df = s.df orelse return null;
    const imgs = df.imageBytes();
    const conts = if (df.ContainerUsage) |u| u.TotalSize else 0;
    const vols = if (df.VolumeUsage) |u| u.TotalSize else 0;
    const cache = if (df.BuildCacheUsage) |u| u.TotalSize else 0;
    const total = imgs + conts + vols + cache;
    return if (total > 0) @intCast(total) else null;
}

fn tabKind(tab: app.Tab) theme.Kind {
    return switch (tab) {
        .overview => .overview,
        .containers => .containers,
        .images => .images,
        .volumes => .volumes,
        .networks => .networks,
        .stacks => .stacks,
        .disk => .disk,
        .runtimes => .runtimes,
        .settings => .settings,
    };
}

fn railStatus(s: *app.State, c: *ui.Context) void {
    const rt = s.activeRuntime();
    const live = rt != null and rt.?.reachable;
    const busy = s.job != null;
    const status = ui.div(c).column().wFull().gap(3).px(8).pb(6);
    defer status.end();

    const line = ui.div(c).row().itemsCenter().gap(7);
    const dot = ui.div(c).w(7).h(7).rounded(4).bg(if (live) c.theme.success else c.theme.danger);
    dot.end();
    ui.text(c, if (busy) "Refreshing…" else if (live) "Live · every 5 s" else "Engine not answering", .{
        .size = 11,
        .color = if (busy) c.theme.accent else if (live) c.theme.muted else c.theme.danger,
    });
    line.end();

    ui.text(c, if (rt) |r| r.name else "no engine found", .{
        .size = 10.5,
        .color = c.theme.muted,
        .truncate = .middle,
    });
    if (s.toast) |t| {
        ui.text(c, t.text, .{
            .size = 10,
            .color = if (t.bad) c.theme.danger else c.theme.success,
            .truncate = .middle,
        });
    }

    // The timestamp and the rescan live together, under the engine they
    // describe; the bottom bar is left to the actions that apply to the page.
    const meta = ui.div(c).row().wFull().itemsCenter().gap(6);
    ui.text(c, c.print("updated {s}", .{
        if (s.last_update == 0) "not yet" else format.clock(c, s.last_update),
    }), .{ .size = 10, .color = c.theme.muted, .mono = true });
    ui.spacer(c);
    if (s.runtimes.len > 1 and !busy) {
        const again = ui.div(c).row().h(18).px(6).itemsCenter().rounded(9).bg(c.theme.surface_hover);
        ui.text(c, "Rediscover", .{ .size = 10, .color = c.theme.muted });
        if (again.clicked()) s.discover();
        again.end();
    }
    meta.end();
}

fn header(s: *app.State, c: *ui.Context) void {
    _ = s;
    _ = c;
}

fn body(s: *app.State, c: *ui.Context) void {
    const area = ui.div(c).column().wFull().grow();
    defer area.end();
    switch (s.tab) {
        .overview => panels.overview(s, c),
        .containers => lists.containers(s, c),
        .images => lists.images(s, c),
        .volumes => lists.volumes(s, c),
        .networks => lists.networks(s, c),
        .stacks => stacks.view(s, c),
        .disk => panels.disk(s, c),
        .runtimes => panels.runtimes(s, c),
        .settings => panels.settings(s, c),
    }
}



/// The confirmation every destructive action goes through, naming its target.
pub fn confirmDialog(s: *app.State, c: *ui.Context, verb: []const u8, target: []const u8) void {
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.danger);
    defer box.end();
    ui.text(c, verb, .{ .size = 13, .weight = .semibold, .color = c.theme.danger });
    const line = c.print("{s} — {s}", .{ target, verb });
    ui.text(c, line, .{ .size = 12, .color = c.theme.muted });
    const row = ui.div(c).row().gap(8);
    defer row.end();
    const yes = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(c.theme.danger);
    defer yes.end();
    ui.text(c, "Confirm", .{ .size = 12, .weight = .medium, .color = c.theme.accent_text });
    if (yes.clicked()) s.resolvePending(true);
    const no = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(c.theme.surface_hover);
    defer no.end();
    ui.text(c, "Cancel", .{ .size = 12, .color = c.theme.text });
    if (no.clicked()) s.resolvePending(false);
}

/// The cleanup sheet: what Docker Desktop calls "Clean / Purge data". The
/// amounts come from the last `/system/df`, so the user sees what is at stake
/// before the first destructive click.
fn pruneDialog(s: *app.State, c: *ui.Context) void {
    var cont: i64 = 0;
    var imgs: i64 = 0;
    var vols: i64 = 0;
    var cache: i64 = 0;
    if (s.df) |df| {
        if (df.ContainerUsage) |u| cont = u.Reclaimable;
        if (df.ImageUsage) |u| imgs = u.Reclaimable;
        if (df.VolumeUsage) |u| vols = u.Reclaimable;
        if (df.BuildCacheUsage) |u| cache = u.Reclaimable;
    }
    var total: i64 = 0;
    if (s.prune_opts.containers) total += cont;
    if (s.prune_opts.images) total += imgs;
    if (s.prune_opts.volumes) total += vols;
    if (s.prune_opts.build_cache) total += cache;

    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.accent);
    defer box.end();
    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, "Clean up unused data", .{ .size = 13, .weight = .semibold });
    ui.spacer(c);
    ui.text(c, c.print("{s} reclaimable", .{format.bytesSigned(c, @max(0, total))}), .{ .size = 11, .color = c.theme.muted });
    head.end();
    ui.text(c, "Stopped containers, unused images and build cache. Volumes and networks are opt-in; this cannot be undone.", .{ .size = 11.5, .color = c.theme.muted });

    pruneOption(c, "Stopped containers", cont, &s.prune_opts.containers);
    pruneOption(c, "Unused images", imgs, &s.prune_opts.images);
    pruneOption(c, "Unused volumes", vols, &s.prune_opts.volumes);
    pruneOption(c, "Unused networks", -1, &s.prune_opts.networks);
    pruneOption(c, "Build cache", cache, &s.prune_opts.build_cache);

    const buttons = ui.div(c).row().wFull().justifyEnd().gap(8).pt(2);
    defer buttons.end();
    const cancel = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(c.theme.surface_hover);
    defer cancel.end();
    ui.text(c, "Cancel", .{ .size = 12, .color = c.theme.text });
    if (cancel.clicked()) s.closePrune();
    const go = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(c.theme.danger);
    defer go.end();
    ui.text(c, "Clean up", .{ .size = 12, .weight = .medium, .color = c.theme.accent_text });
    if (go.clicked()) s.runPrune();
}

fn pruneOption(c: *ui.Context, label: []const u8, amount: i64, on: *bool) void {
    const row = ui.div(c).row().wFull().h(24).itemsCenter().gap(8).control();
    defer row.end();
    const box = ui.div(c).w(14).h(14).rounded(4).center().bg(if (on.*) c.theme.accent else c.theme.track);
    if (on.*) ui.text(c, "✓", .{ .size = 9, .color = c.theme.accent_text });
    box.end();
    ui.text(c, label, .{ .size = 12, .color = c.theme.text });
    ui.spacer(c);
    ui.text(c, if (amount < 0) "—" else format.bytesSigned(c, amount), .{ .size = 11, .color = c.theme.muted });
    if (row.clicked()) on.* = !on.*;
}
