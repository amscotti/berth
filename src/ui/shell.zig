//! The shell: navigation rail, header, status bar, and the dispatch to a view.
//! Every view is drawn inside this frame, so the chrome never moves.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../app.zig");
const format = @import("format.zig");
const widgets = @import("widgets.zig");
const theme = @import("theme.zig");
const layout = @import("layout.zig");
const detail = @import("detail.zig");
const lists = @import("views/lists.zig");
const panels = @import("views/panels.zig");
const config = @import("views/config.zig");
const stacks = @import("views/stacks.zig");

const TABS = [_]app.Tab{
    .overview, .containers, .images,   .volumes,  .networks,
    .stacks,   .disk,       .runtimes, .settings,
};

pub fn view(s: *app.State, c: *ui.Context) void {
    // One place decides how much room the rail and the inspector take; the
    // views read the result back from the state.
    s.layout = layout.of(c.size.w, s.selected != null);
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

    if (s.selected != null) detail.pane(s, c);
}

fn rail(s: *app.State, c: *ui.Context) void {
    const narrow = s.layout.narrow_rail;
    const rail_box = ui.div(c).column().w(s.layout.rail).hFull().py(14).px(if (narrow) 8 else 10).gap(if (narrow) 3 else 2).bg(c.theme.surface).border(c.theme.border);
    defer rail_box.end();

    const brand = ui.div(c).row().wFull().itemsCenter().gap(8).justifyCenter().px(if (narrow) 0 else 6).pb(if (narrow) 12 else 14);
    ui.icon(c, "cube", .{ .size = 18, .color = c.theme.accent });
    if (!narrow) ui.text(c, "berth", .{ .size = 16, .weight = .semibold });
    brand.end(); // explicitly, because the nav items follow it in this column

    // A runtime loop, not `inline for`: inside an inline for the deferred
    // `.end()` calls all run at the end of the unrolled block, nesting every
    // nav item inside the first one.
    for (TABS) |tab| {
        if (tab == .containers) {
            if (narrow) divider(c) else section(c, "Resources");
        }
        if (tab == .disk) {
            if (narrow) divider(c) else section(c, "Engine");
        }
        railItem(s, c, tab, narrow);
    }

    ui.spacer(c);
    railStatus(s, c, narrow);
}

/// A hairline between groups when there are no headings to do it.
fn divider(c: *ui.Context) void {
    const box = ui.div(c).column().wFull().py(6);
    defer box.end();
    const line = ui.div(c).wFull().h(1).bg(c.theme.border);
    line.end();
}

fn section(c: *ui.Context, name: []const u8) void {
    const row = ui.div(c).row().wFull().px(8).pt(12).pb(3);
    defer row.end();
    ui.text(c, name, .{ .size = 10.5, .weight = .semibold, .color = c.theme.muted.alpha(0.75) });
}

fn railItem(s: *app.State, c: *ui.Context, tab: app.Tab, narrow: bool) void {
    const selected = s.tab == tab;
    var item = ui.div(c)
        .row()
        .wFull()
        .h(if (narrow) 36 else 32)
        .itemsCenter()
        .gap(if (narrow) 0 else 8)
        .rounded(7)
        .bg(if (selected) c.theme.accent else c.theme.surface);
    item = if (narrow) item.justifyCenter() else item.px(6);
    if (item.clicked()) {
        // A row from one view has no meaning in another; the inspector closes
        // with the tab rather than following the user around.
        if (s.tab != tab and s.selected != null) s.clearSelection();
        s.tab = tab;
        s.redraw();
    }
    defer item.end();

    const tint = theme.tint(c.theme, tabKind(tab));
    const chip = ui.div(c).row().w(if (narrow) 24 else 20).h(if (narrow) 24 else 20).center().rounded(5)
        .bg(if (selected) c.theme.accent_text.alpha(0.16) else tint.alpha(0.14));
    ui.icon(c, tab.icon(), .{ .size = if (narrow) 13 else 12, .color = if (selected) c.theme.accent_text else tint });
    chip.end(); // before the label, or the label becomes a child of the 20pt chip

    if (narrow) return;
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
    // Nothing has arrived yet: an em dash is honest, "0/0" reads as an empty
    // engine rather than one being read.
    if (s.last_update == 0 and tab != .settings) return "—";
    return switch (tab) {
        .overview => "",
        .containers => c.print("{d}/{d}", .{ app.runningCount(s), s.containers.len }),
        .images => c.print("{d}", .{s.images.len}),
        .volumes => c.print("{d}", .{s.volumes.len}),
        .networks => c.print("{d}", .{s.networks.len}),
        .stacks => c.print("{d}", .{projectCount(s)}),
        .disk => if (storeBytes(s)) |n| format.bytes(c, n) else "—",
        .runtimes => c.print("{d}", .{reachableCount(s)}),
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

fn railStatus(s: *app.State, c: *ui.Context, narrow: bool) void {
    const rt = s.activeRuntime();
    const live = rt != null and rt.?.reachable;
    // A partial refresh leaves usable data on screen; it belongs here as a
    // warning, not as a banner over the table. The busy state deliberately
    // does not change the line: the updated clock is the refresh indicator,
    // and a status that changes width every five seconds makes the rail twitch.
    const warn = s.error_text.len > 0;
    var status = ui.div(c).column().wFull().gap(3).pb(6);
    status = if (narrow) status.itemsCenter() else status.px(8);
    defer status.end();

    const line = ui.div(c).row().itemsCenter().gap(if (narrow) 5 else 7);
    const dot = ui.div(c).w(7).h(7).rounded(4).bg(if (warn) c.theme.danger else if (live) .hex(0xff9f0a) else c.theme.danger);
    dot.end();
    ui.text(c, if (warn) (if (narrow) "Warning" else "Engine warning") else if (live) (if (narrow) "Refresh" else "Refresh · every 5 s") else "Engine not answering", .{
        .size = if (narrow) 9.5 else 11,
        .color = if (warn) .hex(0xff9f0a) else if (live) c.theme.muted else c.theme.danger,
    });
    line.end();

    if (narrow) {
        if (s.toast) |t| {
            ui.text(c, t.text, .{
                .size = 9.5,
                .color = if (t.bad) c.theme.danger else c.theme.success,
                .wrap = true,
            });
        }
        return;
    }

    // Frame-owned copy: a Rescan/Connect later in this frame frees the
    // runtime strings, and the rail is painted after the page.
    const engine_line = if (warn) s.error_text else if (rt) |r| c.print("{s}", .{r.name}) else "no engine found";
    ui.text(c, engine_line, .{
        .size = 10.5,
        .color = if (warn) .hex(0xff9f0a) else c.theme.muted,
        .truncate = .middle,
    });
    if (s.toast) |t| {
        ui.text(c, t.text, .{
            .size = 10,
            .color = if (t.bad) c.theme.danger else c.theme.success,
            .truncate = .middle,
        });
    }

    // The timestamp is the refresh indicator; the rescan lives with the
    // sockets it rescans, under Runtimes.
    ui.text(c, c.print("updated {s}", .{
        if (s.last_update == 0) "not yet" else format.clock(c, s.last_update),
    }), .{ .size = 10, .color = c.theme.muted, .mono = true });
}

fn header(s: *app.State, c: *ui.Context) void {
    _ = s;
    _ = c;
}

fn body(s: *app.State, c: *ui.Context) void {
    const area = ui.div(c).column().wFull().grow();
    defer area.end();
    switch (s.tab) {
        .containers => lists.containers(s, c),
        .images => lists.images(s, c),
        .volumes => lists.volumes(s, c),
        .networks => lists.networks(s, c),
        // The lists scroll themselves; the panel pages are one column that
        // can be taller than a short window, so they get a page scroll.
        .overview, .stacks, .disk, .runtimes, .settings => {
            ui.scroll(c, .{ .width = .fill, .height = .fill });
            defer ui.end(c);
            switch (s.tab) {
                .overview => panels.overview(s, c),
                .stacks => stacks.view(s, c),
                .disk => panels.disk(s, c),
                .runtimes => config.runtimes(s, c),
                .settings => config.settings(s, c),
                else => unreachable,
            }
        },
    }
}

/// The confirmation every destructive action goes through, naming its target.
pub fn confirmDialog(s: *app.State, c: *ui.Context, verb: []const u8, target: []const u8) void {
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.danger);
    defer box.end();
    ui.text(c, verb, .{ .size = 13, .weight = .semibold, .color = c.theme.danger });
    const line = c.print("{s} — {s}", .{ target, verb });
    ui.text(c, line, .{ .size = 12, .color = c.theme.muted });
    const row = ui.div(c).row().wFull().justifyEnd().gap(8);
    if (dialogButton(c, "Cancel", false, false)) s.resolvePending(false);
    if (dialogButton(c, "Confirm", true, true)) s.resolvePending(true);
    row.end();
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
    ui.text(c, c.print("{s} selected", .{format.bytesSigned(c, @max(0, total))}), .{ .size = 11, .color = c.theme.muted });
    head.end();
    ui.text(c, "Stopped containers, unused images and build cache. Volumes and networks are opt-in; this cannot be undone.", .{ .size = 11.5, .color = c.theme.muted });

    pruneOption(c, "Stopped containers", cont, &s.prune_opts.containers);
    pruneOption(c, "Unused images", imgs, &s.prune_opts.images);
    pruneOption(c, "Unused volumes", vols, &s.prune_opts.volumes);
    pruneOption(c, "Unused networks", -1, &s.prune_opts.networks);
    pruneOption(c, "Build cache", cache, &s.prune_opts.build_cache);

    const buttons = ui.div(c).row().wFull().justifyEnd().gap(8).pt(2);
    if (dialogButton(c, "Cancel", false, false)) s.closePrune();
    if (dialogButton(c, "Clean up", true, true)) s.runPrune();
    buttons.end();
}

/// One shape for every dialog action: same width, same height, so Cancel and
/// Confirm sit on one baseline instead of one sizing itself to its word.
fn dialogButton(c: *ui.Context, label: []const u8, danger: bool, primary: bool) bool {
    const bg = if (!primary) c.theme.surface_hover else if (danger) c.theme.danger else c.theme.accent;
    const fg = if (primary) c.theme.accent_text else c.theme.text;
    const box = ui.div(c).row().w(92).h(28).center().rounded(14).bg(bg).control();
    ui.text(c, label, .{ .size = 12, .weight = if (primary) .medium else .regular, .color = fg });
    const clicked = box.clicked();
    box.end();
    return clicked;
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
