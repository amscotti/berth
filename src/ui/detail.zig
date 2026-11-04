//! The right-hand inspector: curated facts, the engine's raw JSON, logs, and
//! the actions that belong to one object. Split from `shell.zig` so the frame
//! around every view stays small enough to read.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../app.zig");
const api = @import("../engine/api.zig");
const lifetime = @import("../engine/lifetime.zig");
const image_tasks = @import("../image_tasks.zig");
const terminal = @import("../terminal.zig");
const logs = @import("../logs.zig");
const format = @import("format.zig");
const widgets = @import("widgets.zig");

/// The pane's label column: facts line up on their values instead of starting
/// wherever the longest label happens to end.
const LABEL_W = 64.0;

pub fn pane(s: *app.State, c: *ui.Context) void {
    const box = ui.div(c).column().w(s.layout.pane).hFull().p(14).gap(8).bg(c.theme.surface).border(c.theme.border);
    defer box.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, s.selected_name, .{ .size = 15, .weight = .semibold, .truncate = .ellipsis });
    ui.spacer(c);
    const close = ui.div(c).row().w(24).h(24).center().rounded(12).bg(c.theme.surface_hover).control();
    ui.icon(c, "xmark", .{ .size = 11, .color = c.theme.muted });
    if (close.clicked()) s.clearSelection();
    close.end();
    head.end();

    // Facts or the engine's own JSON, on their own row: the title keeps its
    // room however long the selected name is.
    const tabs = ui.div(c).row().wFull().itemsCenter().gap(6);
    if (tab(c, "Facts", "info.circle", !s.pane_inspect)) {
        s.pane_inspect = false;
        s.redraw();
    }
    if (tab(c, "Inspect", "doc.text", s.pane_inspect)) {
        s.pane_inspect = true;
        if (s.inspect_buf == null) s.reloadInspect();
        s.redraw();
    }
    ui.spacer(c);
    if (s.pane_inspect) {
        if (smallAction(c, "Copy", "doc.on.doc")) {
            if (s.inspect_buf) |text| {
                nanoui.clipboard.writeText(text);
                s.notify("Inspect JSON copied", false);
            }
        }
        if (smallAction(c, "Refresh", "arrow.clockwise")) s.reloadInspect();
    }
    tabs.end();

    const rule = ui.div(c).wFull().h(1).bg(c.theme.border);
    rule.end();

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

fn tab(c: *ui.Context, label: []const u8, icon_name: []const u8, active: bool) bool {
    const pill = ui.div(c).row().h(24).px(9).itemsCenter().gap(5).rounded(12)
        .bg(if (active) c.theme.accent.alpha(0.16) else c.theme.surface_hover).control();
    const color = if (active) c.theme.accent else c.theme.muted;
    ui.icon(c, icon_name, .{ .size = 10, .color = color });
    ui.text(c, label, .{
        .size = 11,
        .weight = if (active) .medium else .regular,
        .color = color,
    });
    const clicked = pill.clicked();
    pill.end();
    return clicked;
}

fn smallAction(c: *ui.Context, label: []const u8, icon_name: []const u8) bool {
    const pill = ui.div(c).row().h(22).px(8).itemsCenter().gap(4).rounded(11).bg(c.theme.surface_hover).control();
    ui.icon(c, icon_name, .{ .size = 9, .color = c.theme.muted });
    ui.text(c, label, .{ .size = 10.5, .color = c.theme.muted });
    const clicked = pill.clicked();
    pill.end();
    return clicked;
}

/// The engine's own JSON for the selected object, fetched on demand and owned
/// by the state so a snapshot swap cannot pull it out from under the pane.
fn inspectView(s: *app.State, c: *ui.Context) void {
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
        ui.text(c, "Nothing loaded yet. Refresh fetches the engine's JSON.", .{ .size = 11.5, .color = c.theme.muted });
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
        // The status already says "Up 2 hours"; the useful, missing fact is
        // the wall-clock start. Before the inspect lands, show the uptime.
        if (s.detail_lifetime) |life| {
            if (life.started_at) |at| facts(c, "Started", format.started(c, at, format.now()));
        } else if (lifetime.upSeconds(ct.Status)) |secs| {
            facts(c, "Up", format.duration(c, secs));
        }
        const st = app.statFor(s, ct.Id);
        if (ct.isUp()) {
            // The table can drop MEM at the smallest size; the inspector is
            // then the only place the selected container's figure survives.
            if (st.sampled) {
                // CPU can be unknown while memory is real: say so for CPU
                // only, rather than hiding the whole sample.
                facts(c, "CPU", if (st.cpu_known) format.percent(c, st.cpu) else "—");
                const mem = if (st.limit > 0)
                    c.print("{s} of {s}", .{ format.bytes(c, st.mem), format.bytes(c, @as(u64, @intCast(st.limit))) })
                else
                    format.bytes(c, st.mem);
                facts(c, "Memory", mem);
            } else {
                facts(c, "CPU", "—");
                facts(c, "Memory", "—");
            }
            facts(c, "Network", c.print("↓ {s} · ↑ {s}", .{
                format.bytes(c, st.net_rx),
                format.bytes(c, st.net_tx),
            }));
            facts(c, "Disk", c.print("read {s} · wrote {s}", .{
                format.bytes(c, st.blk_read),
                format.bytes(c, st.blk_write),
            }));
            if (st.pids) |n| facts(c, "PIDs", c.print("{d}", .{n}));
        }
        if (s.detail_lifetime) |life| {
            facts(c, "Restarts", c.print("{d}", .{life.restarts}));
        }
        var bindings: [16]api.HostBinding = undefined;
        if (ct.publishedPorts(&bindings) > 0) {
            const row = ui.div(c).row().wFull().itemsCenter().gap(10);
            factLabel(c, "Ports");
            // The inspector has room for host→container, which is the
            // spelling that says what clicking the pill will reach.
            if (widgets.portPills(c, &ct, 4, s.layout.pane - 28 - LABEL_W - 10, true)) |binding| s.openPort(binding);
            row.end();
        }
        // Pause and the run command sit here rather than in the row: three
        // buttons fit the row's fixed slot. Pause is reversible enough not to
        // need a confirm; the command goes straight to the clipboard.
        const verbs = ui.div(c).row().wFull().gap(6).pt(2);
        if (ct.isPaused()) {
            if (verb(c, "Resume", "play.fill")) _ = s.actFmt("POST", "/containers/{s}/unpause", ct.Id, "Resumed");
        } else if (ct.isRunning()) {
            if (verb(c, "Pause", "pause.fill")) _ = s.actFmt("POST", "/containers/{s}/pause", ct.Id, "Paused");
        }
        if (ct.isRunning() and verb(c, "Terminal", "terminal")) terminal.open(s, &ct);
        if (s.detail_lifetime) |life| {
            if (life.run_cmd) |cmd| {
                if (verb(c, "Copy run command", "doc.on.doc")) {
                    nanoui.clipboard.writeText(cmd);
                    s.notify("Run command copied", false);
                }
            }
        }
        verbs.end();
        break;
    }

    const logs_head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, "Logs · last 500 lines", .{ .size = 11, .weight = .medium, .color = c.theme.muted });
    ui.spacer(c);
    if (smallAction(c, "Refresh", "arrow.clockwise")) logs.reload(s);
    logs_head.end();

    const log_tools = ui.div(c).row().wFull().itemsCenter().gap(6).pb(2);
    if (logToggle(c, "Follow", s.log_opts.follow)) {
        s.log_opts.follow = !s.log_opts.follow;
        s.redraw();
    }
    if (logToggle(c, "Times", s.log_opts.timestamps)) {
        s.log_opts.timestamps = !s.log_opts.timestamps;
        logs.reload(s);
    }
    if (smallAction(c, "Copy", "doc.on.doc")) logs.copy(s);
    ui.spacer(c);
    _ = ui.textField(c, &s.log_opts.search, .{
        .placeholder = "Search logs",
        .width = .{ .px = if (s.layout.pane < 400) 104 else 130 },
        .size = 11.5,
    });
    log_tools.end();

    ui.scroll(c, .{ .width = .fill, .height = .fill, .padding = .{ .left = 10, .right = 10, .top = 8, .bottom = 8 }, .radius = 8, .background = c.theme.background });
    defer ui.end(c);
    if (s.log_text.len == 0) {
        ui.text(c, "No output yet. A container that has said nothing says nothing.", .{ .size = 11.5, .color = c.theme.muted });
    } else {
        ui.text(c, logs.visible(s, c), .{
            .size = 11.5,
            .mono = true,
            .wrap = true,
            .leading = 1.4,
            .color = c.theme.text,
        });
    }
}

fn logToggle(c: *ui.Context, label: []const u8, on: bool) bool {
    const pill = ui.div(c).row().h(22).px(8).itemsCenter().rounded(11)
        .bg(if (on) c.theme.accent.alpha(0.16) else c.theme.surface_hover).control();
    ui.text(c, label, .{
        .size = 10.5,
        .weight = if (on) .medium else .regular,
        .color = if (on) c.theme.accent else c.theme.muted,
    });
    const clicked = pill.clicked();
    pill.end();
    return clicked;
}

fn imageDetail(s: *app.State, c: *ui.Context) void {
    for (s.images) |im| {
        if (s.selected == null or !std.mem.eql(u8, s.selected.?, im.Id)) continue;
        facts(c, "Repository", if (im.dangling()) "untagged" else im.tag());
        facts(c, "ID", shortId(im.Id));
        facts(c, "Size", format.bytes(c, im.Size));
        facts(c, "Created", format.ago(c, im.Created, format.now()));
        facts(c, "Status", if (im.dangling()) "dangling" else "ready");
        if (verb(c, "Run…", "play.fill")) {
            // A dangling image has no tag to run: its full Id still names it
            // unambiguously, where its "label" would run a container named
            // "untagged".
            image_tasks.prepareRun(s, if (im.dangling()) im.Id else im.label());
            s.clearSelection();
        }
        action(s, c, "Delete image", im.Id, if (im.dangling()) "untagged" else im.tag(), "Delete", "DELETE", "/images/{s}?force=1");
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
        } else {
            facts(c, "Size", "—");
            facts(c, "In use", "—");
        }
        action(s, c, "Remove volume", v.Name, v.Name, "Remove", "DELETE", "/volumes/{s}");
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
            action(s, c, "Remove network", if (n.Id.len > 0) n.Id else n.Name, n.Name, "Remove", "DELETE", "/networks/{s}");
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

fn verb(c: *ui.Context, label: []const u8, icon_name: []const u8) bool {
    const box = ui.div(c).row().h(24).px(10).itemsCenter().gap(5).rounded(12).bg(c.theme.surface_hover).control();
    ui.icon(c, icon_name, .{ .size = 9, .color = c.theme.text });
    ui.text(c, label, .{ .size = 11.5, .color = c.theme.text });
    const clicked = box.clicked();
    box.end();
    return clicked;
}

fn action(s: *app.State, c: *ui.Context, label: []const u8, id: []const u8, name: []const u8, verb_text: []const u8, method: []const u8, comptime path_fmt: []const u8) void {
    const box = ui.div(c).row().h(26).px(10).itemsCenter().gap(5).rounded(13).bg(c.theme.danger.alpha(0.14)).control();
    ui.icon(c, "trash", .{ .size = 10, .color = c.theme.danger });
    ui.text(c, label, .{ .size = 12, .weight = .medium, .color = c.theme.danger });
    if (box.clicked()) s.confirm(id, name, verb_text, method, path_fmt);
    box.end();
}

fn factLabel(c: *ui.Context, text: []const u8) void {
    const slot = ui.div(c).row().w(LABEL_W).itemsCenter();
    defer slot.end();
    ui.text(c, text, .{ .size = 11, .color = c.theme.muted });
}

fn facts(c: *ui.Context, name: []const u8, value: []const u8) void {
    const row = ui.div(c).row().wFull().gap(10).itemsCenter();
    defer row.end();
    factLabel(c, name);
    ui.text(c, value, .{ .size = 11.5, .truncate = .middle });
}
