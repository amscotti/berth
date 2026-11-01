//! The shell: navigation rail, header, status bar, and the dispatch to a view.
//! Every view is drawn inside this frame, so the chrome never moves.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../app.zig");
const format = @import("format.zig");
const widgets = @import("widgets.zig");
const lists = @import("views/lists.zig");
const panels = @import("views/panels.zig");
const stacks = @import("views/stacks.zig");

const RAIL = 200.0;

const TABS = [_]app.Tab{
    .overview, .containers, .images,  .volumes, .networks,
    .stacks,   .disk,       .runtimes, .settings,
};

pub fn view(s: *app.State, c: *ui.Context) void {
    const root = ui.div(c).row().sizeFull().bg(c.theme.background);
    defer root.end();
    rail(s, c);
    const main = ui.div(c).column().grow().hFull().px(20).pt(16).gap(4);
    header(s, c);
    body(s, c);
    footer(s, c);
    main.end(); // before the detail pane, or the pane becomes a child of it

    if (s.selected != null) detailPane(s, c);
}

/// The right-hand pane: what this container is, and what it has said lately.
fn detailPane(s: *app.State, c: *ui.Context) void {
    const pane = ui.div(c).column().w(430).hFull().p(14).gap(8).bg(c.theme.surface).border(c.theme.border);
    defer pane.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, s.selected_name, .{ .size = 15, .weight = .semibold, .truncate = .ellipsis });
    ui.spacer(c);
    const close = ui.div(c).row().h(22).px(8).itemsCenter().rounded(11).bg(c.theme.surface_hover);
    ui.text(c, "Close", .{ .size = 11, .color = c.theme.muted });
    if (close.clicked()) s.clearSelection();
    close.end();
    head.end();

    // The facts that answer "what is this thing", from the list entry we have.
    for (s.containers) |ct| {
        if (s.selected == null or !std.mem.eql(u8, s.selected.?, ct.Id)) continue;
        facts(c, "Image", ct.Image);
        facts(c, "State", ct.State);
        facts(c, "Status", ct.Status);
        if (ct.project()) |project| facts(c, "Project", project);
        if (ct.service()) |service| facts(c, "Service", service);
        if (widgets.portsText(c, &ct, 4).len > 0) facts(c, "Ports", widgets.portsText(c, &ct, 4));
        break;
    }

    ui.text(c, "Logs · last 200 lines", .{ .size = 11, .weight = .medium, .color = c.theme.muted });
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
        const selected = s.tab == tab;
        const item = ui.div(c)
            .row()
            .wFull()
            .h(32)
            .itemsCenter()
            .gap(8)
            .px(8)
            .rounded(7)
            .bg(if (selected) c.theme.accent.alpha(0.14) else c.theme.surface);
        if (item.clicked()) s.tab = tab;
        defer item.end();
        ui.icon(c, tab.icon(), .{ .size = 15, .color = if (selected) c.theme.accent else c.theme.muted });
        ui.text(c, tab.label(), .{
            .size = 13,
            .weight = if (selected) .medium else .regular,
            .color = if (selected) c.theme.accent else c.theme.text,
        });
    }

    ui.spacer(c);

    // Which engine we are talking to, always visible: it is the question a
    // monitoring app must never leave ambiguous.
    const rt = s.activeRuntime();
    const status = ui.div(c).row().itemsCenter().gap(6).px(6).py(6);
    defer status.end();
    const dot_color = if (rt != null and rt.?.reachable) c.theme.success else c.theme.danger;
    const dot = ui.div(c).w(7).h(7).rounded(4).bg(dot_color);
    defer dot.end();
    ui.text(c, if (rt) |r| r.name else "no engine", .{ .size = 11, .color = c.theme.muted, .truncate = .ellipsis });
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

fn footer(s: *app.State, c: *ui.Context) void {
    const row = ui.div(c).row().wFull().itemsCenter().gap(10).pt(8).border(c.theme.border);
    defer row.end();
    const rt = s.activeRuntime();
    const where = if (rt) |r| r.socket else "—";
    const stamp = if (s.last_update == 0) "not yet" else format.clock(c, s.last_update);
    ui.text(c, where, .{ .size = 11, .color = c.theme.muted, .truncate = .middle });
    ui.spacer(c);
    if (s.toast) |t| {
        ui.text(c, t.text, .{
            .size = 11,
            .weight = .medium,
            .color = if (t.bad) c.theme.danger else c.theme.success,
        });
    }
    ui.text(c, if (s.last_update == 0) "reading engine…" else "updated", .{ .size = 11, .color = c.theme.muted });
    ui.text(c, stamp, .{ .size = 11, .color = c.theme.muted, .tabular = true });

    btn(s, c, "Refresh", .{});
    if (s.runtimes.len > 1) btn(s, c, "Rediscover", .{ .discover = true });
}

pub fn btn(s: *app.State, c: *ui.Context, label: []const u8, opts: struct { discover: bool = false, danger: bool = false }) void {
    const variant: ui.Color = if (opts.danger) c.theme.danger else c.theme.accent;
    const box = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(variant.alpha(0.14));
    defer box.end();
    ui.text(c, label, .{ .size = 12, .weight = .medium, .color = variant });
    if (box.clicked()) {
        if (opts.discover) s.discover() else s.refresh();
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
