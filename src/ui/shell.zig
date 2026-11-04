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
const layout = @import("layout.zig");
const detail = @import("detail.zig");
const lists = @import("views/lists.zig");
const panels = @import("views/panels.zig");
const config = @import("views/config.zig");
const stacks = @import("views/stacks.zig");
const events_view = @import("views/events.zig");
const bulk = @import("../bulk.zig");
const create_mod = @import("../create.zig");
const ops = @import("../ops.zig");
const prune = @import("../engine/prune.zig");
const sheets = @import("../sheets.zig");
const shortcuts = @import("../shortcuts.zig");
const tabs = @import("../tabs.zig");
const kit = nanoui.kit;

pub fn view(s: *app.State, c: *ui.Context) void {
    // Window keys are offered before any view or sheet paints: a tab switch
    // must not race a dialog's own Enter/Escape handling.
    shortcuts.handle(s, c);
    // One place decides how much room the rail and the inspector take; the
    // views read the result back from the state.
    s.layout = layout.of(c.size.w, s.selected != null);
    // Warm paper and amber, derived from the framework's theme each frame so
    // the system's light/dark switch keeps working.
    c.theme = theme.of(c.theme);
    const sheet_open = sheets.open(s);
    // A sheet owns the keyboard too: a text field left focused on the page
    // must not receive the Enter or Escape meant for the sheet. The create
    // form's own field is the one exception, because Enter submits it.
    if (sheet_open) clearBackgroundFocus(s, c);
    // Hold this event's text/key until the sheet is built: the page below is
    // context only, so it must not consume typing or Enter on the way there.
    const sheet_key = if (sheet_open) c.input.key else null;
    const sheet_chars = if (sheet_open) c.input.chars else "";
    const sheet_ops = if (sheet_open) c.input.ops else &.{};
    if (sheet_open) {
        c.input.key = null;
        c.input.chars = "";
        c.input.ops = &.{};
    }

    const root = ui.div(c).row().sizeFull().bg(c.theme.background);
    defer root.end();
    // One wrapper for everything a sheet covers. Hiding this box from the
    // accessibility tree takes the rail, page and inspector with it.
    const page = ui.div(c).row().sizeFull();
    rail(s, c);
    const main = ui.div(c).column().grow().hFull().px(20).pt(16).pb(12).gap(4);
    header(s, c);
    body(s, c);
    main.end(); // before the detail pane, or the pane becomes a child of it

    if (s.selected != null) detail.pane(s, c);
    page.end();
    // Drawn last, on its own layer: the page stays visible for context but
    // every pointer event lands on the dimming or the panel, never behind it.
    if (sheet_open) {
        ui.access.hide(c, page.index);
        c.input.key = sheet_key;
        c.input.chars = sheet_chars;
        c.input.ops = sheet_ops;
        sheetOverlay(s, c);
    }
}

fn clearBackgroundFocus(s: *app.State, c: *ui.Context) void {
    if (c.focus == 0) return;
    const own_field = s.create_dialog.open != null and c.focus == s.create_dialog.name.last_id;
    if (!own_field) c.focus = 0;
}

/// A sheet is a window-level layer, matching nanoui's dialog layers. The
/// backdrop takes every click outside the panel, so row actions and rail
/// clicks cannot run behind an unanswered destructive question.
const sheet_backdrop_layer = 10;
const sheet_panel_layer = 11;

fn sheetOverlay(s: *app.State, c: *ui.Context) void {
    // The page was built first and may have re-focused a field of its own;
    // clear it again before the sheet takes this frame's keys.
    clearBackgroundFocus(s, c);
    const dim: f32 = if (c.theme.dark) 0.58 else 0.38;
    const backdrop = ui.div(c).fixed().w(c.size.w).h(c.size.h).layer(sheet_backdrop_layer)
        .bg(ui.Color.black.alpha(dim)).control().scrollY();
    // The interaction registers the backdrop as the top hit and clears the
    // page's text focus on press; it deliberately does not close the sheet.
    // `scrollY` makes the same hit the wheel target, so the page behind does
    // not scroll while the question is up.
    _ = backdrop.interaction();
    ui.access.hide(c, backdrop.index);
    backdrop.end();

    const panel_id = c.peekId(null);
    const h = if (c.rects.get(panel_id)) |r| r.h else 260;
    const w = @min(620, c.size.w - 64);
    const top = @max(16, (c.size.h - h) / 2 - c.size.h * 0.04);
    const panel = ui.div(c).fixed().offset((c.size.w - w) / 2, top).w(w).layer(sheet_panel_layer).column();
    if (s.bulk.pending != null) {
        bulk.confirmDialog(s, c);
    } else if (s.pending) |p| {
        confirmDialog(s, c, p.verb, p.name);
    } else if (s.ops.pending) |p| {
        engineConfirmDialog(s, c, &p);
    } else if (s.prune_open) {
        pruneDialog(s, c);
    } else if (s.create_dialog.open != null) {
        create_mod.dialog(s, c);
    }
    panel.end();
}

/// Whether a sheet's Enter/Escape may be taken this frame. A focused field
/// belongs to the sheet only for the create form; anything else is page
/// focus that slipped through and must be ignored.
fn sheetKeysAllowed(s: *app.State, c: *ui.Context) bool {
    if (c.focus == 0) return true;
    return s.create_dialog.open != null and c.focus == s.create_dialog.name.last_id;
}

/// A sheet title is the one explicitly described element. Describing the
/// panel itself would claim every control inside it, leaving VoiceOver with
/// a labelled box and no buttons.
fn sheetHeading(c: *ui.Context, text: []const u8, color: ui.Color, wrap: bool) void {
    const at: u32 = @intCast(c.nodes.items.len);
    ui.text(c, text, .{ .size = 13, .weight = .semibold, .color = color, .wrap = wrap });
    ui.access.describe(c, at, .{ .role = .static_text, .label = text, .header = true });
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
    for (tabs.all) |tab| {
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
    const it = item.interaction();
    if (it.clicked) {
        c.changed = true;
        shortcuts.switchTab(s, tab);
    }
    defer item.end();

    const tint = theme.tint(c.theme, tabKind(tab));
    const chip = ui.div(c).row().w(if (narrow) 24 else 20).h(if (narrow) 24 else 20).center().rounded(5)
        .bg(if (selected) c.theme.accent_text.alpha(0.16) else tint.alpha(0.14));
    ui.icon(c, tab.icon(), .{ .size = if (narrow) 13 else 12, .color = if (selected) c.theme.accent_text else tint });
    chip.end(); // before the label, or the label becomes a child of the 20pt chip

    // The icon-only rail still has to say what each glyph means, both to a
    // pointer at rest and to VoiceOver. The figure is part of the name so the
    // tooltip carries the same monitor reading the wide rail prints.
    const figure = railFigure(s, c, tab);
    const accessible = if (figure.len > 0)
        c.print("{s} · {s}", .{ tab.label(), figure })
    else
        tab.label();
    kit.tooltip(c, it, accessible, .{ .placement = .right });
    ui.access.describe(c, item.index, .{
        .role = .tab,
        .label = accessible,
        .selected = selected,
        .hint = c.print("Switch to {s}", .{tab.label()}),
    });

    if (narrow) return;
    ui.text(c, tab.label(), .{
        .size = 13,
        .weight = if (selected) .semibold else .regular,
        .color = if (selected) c.theme.accent_text else c.theme.text,
    });
    ui.spacer(c);
    if (figure.len > 0) ui.text(c, figure, .{
        .size = 10.5,
        .mono = true,
        .color = if (selected) c.theme.accent_text else c.theme.muted,
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
        .events => c.print("{d}", .{s.events.items.len}),
        .stacks => c.print("{d}", .{s.projects.len}),
        .disk => if (storeBytes(s)) |n| format.bytes(c, n) else "—",
        .runtimes => c.print("{d}", .{reachableCount(s)}),
        .settings => "",
    };
}

fn reachableCount(s: *app.State) usize {
    var n: usize = 0;
    for (s.runtimes) |rt| {
        if (rt.reachable) n += 1;
    }
    return n;
}

fn storeBytes(s: *app.State) ?u64 {
    const df = s.df.value orelse return null;
    const total = df.stores().total();
    return if (total > 0) @intCast(total) else null;
}

fn tabKind(tab: app.Tab) theme.Kind {
    return switch (tab) {
        .overview => .overview,
        .containers => .containers,
        .images => .images,
        .volumes => .volumes,
        .networks => .networks,
        .events => .events,
        .stacks => .stacks,
        .disk => .disk,
        .runtimes => .runtimes,
        .settings => .settings,
    };
}

fn railStatus(s: *app.State, c: *ui.Context, narrow: bool) void {
    const rt = s.activeRuntime();
    const live = rt != null and rt.?.reachable;
    const scanning = s.discovery_job != null;
    // A partial refresh leaves usable data on screen; it belongs here as a
    // warning, not as a banner over the table. The busy state deliberately
    // does not change the line: the updated clock is the refresh indicator,
    // and a status that changes width every five seconds makes the rail twitch.
    // The first scan is the exception: before it finishes, "not answering"
    // would be a verdict on a question that is still being asked.
    const warn = s.error_text.len > 0;
    const label = if (warn)
        (if (narrow) "Warning" else "Engine warning")
    else if (scanning)
        (if (narrow) "Reading" else if (rt != null) "Rescanning sockets…" else "Reading engine…")
    else if (live)
        (if (narrow) "Refresh" else "Refresh · every 5 s")
    else
        "Engine not answering";
    var status = ui.div(c).column().wFull().gap(3).pb(6);
    status = if (narrow) status.itemsCenter() else status.px(8);
    const status_it = status.interaction();
    defer status.end();

    // Frame-owned copy: the runtime strings belong to the state and a scan may
    // replace them between frames, while this painted slice must stay valid.
    const engine_line = if (warn)
        s.error_text
    else if (scanning and rt == null)
        "probing known sockets…"
    else if (rt) |r|
        c.print("{s}", .{r.name})
    else
        "no engine found";
    const clock = if (s.last_update == 0) "not yet" else format.clock(c, s.last_update);
    const updated_line = c.print("updated {s}", .{clock});
    // The bottom line is one monitor readout wherever the rail is: status,
    // engine name and the clock that proves a refresh happened. In narrow
    // form the last two stack, because 88 points cannot hold a sentence.
    const status_label = c.print("{s} · {s} · {s}", .{ label, engine_line, updated_line });
    kit.tooltip(c, status_it, status_label, .{ .placement = .right });
    // The tooltip needs an interaction for hover, but the readout is not a
    // control: describe it as static text or VoiceOver announces a button.
    ui.access.describe(c, status.index, .{ .role = .static_text, .label = status_label });

    const line = ui.div(c).row().itemsCenter().gap(if (narrow) 5 else 7);
    const dot = ui.div(c).w(7).h(7).rounded(4).bg(if (warn) c.theme.danger else if (scanning or live) theme.warning_fill else c.theme.danger);
    dot.end();
    ui.text(c, label, .{
        .size = if (narrow) 9.5 else 11,
        .color = if (warn) theme.warning(c.theme) else if (scanning or live) c.theme.muted else theme.dangerText(c.theme),
    });
    line.end();

    if (narrow) {
        ui.text(c, engine_line, .{
            .size = 9,
            .color = if (warn) theme.warning(c.theme) else c.theme.muted,
            .truncate = .middle,
        });
        // 88 points fit the clock, not "updated" plus seconds; the tooltip
        // above still carries the full sentence.
        ui.text(c, clock, .{
            .size = 9,
            .mono = true,
            .color = c.theme.muted,
        });
        if (s.toast) |t| {
            // One clipped line: a long engine message must not grow the
            // status block until it pushes the clock off the window.
            ui.text(c, t.text, .{
                .size = 9.5,
                .color = if (t.bad) theme.dangerText(c.theme) else theme.successText(c.theme),
                .truncate = .middle,
            });
        }
        return;
    }

    ui.text(c, engine_line, .{
        .size = 10.5,
        .color = if (warn) theme.warning(c.theme) else c.theme.muted,
        .truncate = .middle,
    });
    if (s.toast) |t| {
        ui.text(c, t.text, .{
            .size = 10,
            .color = if (t.bad) theme.dangerText(c.theme) else theme.successText(c.theme),
            .truncate = .middle,
        });
    }

    // The timestamp is the refresh indicator; the rescan lives with the
    // sockets it rescans, under Runtimes.
    ui.text(c, updated_line, .{ .size = 10, .color = c.theme.muted, .mono = true });
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
        .events => events_view.view(s, c),
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
    const keys = sheetKeysAllowed(s, c);
    if (keys and c.shortcut(.{ .key = ui.Shortcut.escape })) {
        s.resolvePending(false);
        return;
    }
    const enter = keys and c.input.key == .enter;
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.danger);
    defer box.end();
    sheetHeading(c, verb, theme.dangerText(c.theme), false);
    const line = c.print("{s} — {s}", .{ target, verb });
    ui.text(c, line, .{ .size = 12, .color = c.theme.muted, .wrap = true });
    const row = ui.div(c).row().wFull().justifyEnd().gap(8);
    if (widgets.dialogButton(c, "Cancel", false, false)) s.resolvePending(false);
    if (enter or widgets.dialogButton(c, "Confirm", true, true)) s.resolvePending(true);
    row.end();
}

/// Stopping or restarting a machine stops the containers inside it. The
/// sentence names the engine and the count captured when the button was
/// pressed, so the answer is about the engine the user pointed at.
fn engineConfirmDialog(s: *app.State, c: *ui.Context, p: *const ops.Pending) void {
    const keys = sheetKeysAllowed(s, c);
    if (keys and c.shortcut(.{ .key = ui.Shortcut.escape })) {
        ops.resolve(s, false);
        return;
    }
    const enter = keys and c.input.key == .enter;
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.danger);
    defer box.end();
    // A machine name can be long; the title wraps inside the 620-point panel
    // instead of running under the buttons.
    sheetHeading(c, c.print("{s} {s}?", .{ p.verb.title(), p.nameText() }), theme.dangerText(c.theme), true);
    var line_buf: [256]u8 = undefined;
    ui.text(c, c.print("{s}", .{ops.consequence(p, &line_buf)}), .{ .size = 12, .color = c.theme.muted, .wrap = true });
    const row = ui.div(c).row().wFull().justifyEnd().gap(8);
    if (widgets.dialogButton(c, "Cancel", false, false)) ops.resolve(s, false);
    if (enter or widgets.dialogButton(c, p.verb.title(), true, true)) ops.resolve(s, true);
    row.end();
}

/// The cleanup sheet: what Docker Desktop calls "Clean / Purge data". The
/// amounts come from the last `/system/df`, so the user sees what is at stake
/// before the first destructive click.
fn pruneDialog(s: *app.State, c: *ui.Context) void {
    const keys = sheetKeysAllowed(s, c);
    if (keys and c.shortcut(.{ .key = ui.Shortcut.escape })) {
        s.closePrune();
        return;
    }
    const enter = keys and c.input.key == .enter;
    const engine: prune.Engine = if (s.activeRuntime()) |rt|
        switch (rt.dialect) {
            .podman => .podman,
            .docker => .docker,
        }
    else
        .docker;
    const build_cache_supported = prune.supported(.build_cache, engine);
    // The shared preference is kept for Docker; only the effective value is
    // false while a Podman engine is active.
    const build_cache_on = prune.buildCacheEnabled(s.prune_opts.build_cache, engine);
    var cont: i64 = 0;
    var imgs: i64 = 0;
    var vols: i64 = 0;
    var cache: i64 = 0;
    if (s.df.value) |df| {
        const free = df.reclaimableByStore();
        cont = free.containers;
        imgs = free.images;
        vols = free.volumes;
        cache = free.build_cache;
    }
    const selected = pruneSelectedBytes(s.df.value, s.prune_opts, build_cache_on);

    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.accent);
    defer box.end();
    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    // The amounts below come from the df cache; name its age in the heading so
    // a stale figure cannot read as a fresh one.
    const title = if (s.df.at > 0 and s.df.value != null)
        c.print("Clean up unused data · disk {s} ago", .{format.ago(c, s.df.at, format.now())})
    else
        "Clean up unused data";
    sheetHeading(c, title, c.theme.text, false);
    ui.spacer(c);
    ui.text(c, c.print("{s} selected", .{format.bytesSigned(c, @max(0, selected))}), .{ .size = 11, .color = c.theme.muted });
    head.end();
    ui.text(c, if (build_cache_supported)
        "Stopped containers, unused images and build cache. Volumes and networks are opt-in; this cannot be undone."
    else
        "Stopped containers and unused images. Volumes and networks are opt-in; this cannot be undone.", .{ .size = 11.5, .color = c.theme.muted, .wrap = true });

    pruneOption(c, "Stopped containers", cont, &s.prune_opts.containers);
    pruneOption(c, "Unused images", imgs, &s.prune_opts.images);
    pruneOption(c, "Unused volumes", vols, &s.prune_opts.volumes);
    pruneOption(c, "Unused networks", -1, &s.prune_opts.networks);
    if (build_cache_supported) pruneOption(c, "Build cache", cache, &s.prune_opts.build_cache);

    const buttons = ui.div(c).row().wFull().justifyEnd().gap(8).pt(2);
    if (widgets.dialogButton(c, "Cancel", false, false)) s.closePrune();
    if (enter or widgets.dialogButton(c, "Clean up", true, true)) s.runPrune();
    buttons.end();
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

/// The dialog's "selected" figure must count only steps cleanup will attempt;
/// `build_cache_on` is the engine-aware effective flag, not the stored
/// preference, so a hidden Podman row cannot contribute bytes.
pub fn pruneSelectedBytes(df: ?api.Df, opts: app.PruneOptions, build_cache_on: bool) i64 {
    const free = if (df) |d| d.reclaimableByStore() else return 0;
    var total: i64 = 0;
    if (opts.containers) total = total +| free.containers;
    if (opts.images) total = total +| free.images;
    if (opts.volumes) total = total +| free.volumes;
    if (build_cache_on) total = total +| free.build_cache;
    return total;
}

test "selected prune bytes saturate across stores" {
    const max = std.math.maxInt(i64);
    const df = api.Df{
        .ContainerUsage = .{ .Reclaimable = max },
        .ImageUsage = .{ .Reclaimable = max },
        .VolumeUsage = .{ .Reclaimable = max },
        .BuildCacheUsage = .{ .Reclaimable = max },
    };
    const opts = app.PruneOptions{ .containers = true, .images = true, .volumes = true, .build_cache = true };
    try std.testing.expectEqual(max, pruneSelectedBytes(df, opts, true));
}

test "a podman visit does not clear the docker build-cache preference" {
    const df = api.Df{
        .ContainerUsage = .{ .Reclaimable = 10 },
        .BuildCacheUsage = .{ .Reclaimable = 100 },
    };
    const opts = app.PruneOptions{ .build_cache = true };
    const podman_on = prune.buildCacheEnabled(opts.build_cache, .podman);
    const docker_on = prune.buildCacheEnabled(opts.build_cache, .docker);
    try std.testing.expectEqual(@as(i64, 10), pruneSelectedBytes(df, opts, podman_on));
    try std.testing.expectEqual(@as(i64, 110), pruneSelectedBytes(df, opts, docker_on));
    try std.testing.expect(opts.build_cache);
}
