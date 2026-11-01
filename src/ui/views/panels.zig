//! Panels: the overview, disk usage, runtimes and settings.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");
const prefs = @import("../../settings.zig");
const graph = @import("../graph.zig");
const history = @import("../history.zig");

pub fn overview(s: *app.State, c: *ui.Context) void {
    const rt = s.activeRuntime();
    widgets.pageHeader(c, "Overview", if (rt) |r| r.name else "no engine");

    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);

    const info = if (rt) |r| r.info else api.Info{};

    // What the containers are using, as it moves: the figures nanomon's
    // overview is built from, with berth's own colours.
    var cpu_sum: f32 = 0;
    var mem_sum: u64 = 0;
    for (s.containers, 0..) |ct, i| {
        if (!ct.isRunning() or i >= s.stats.len) continue;
        cpu_sum += @floatCast(s.stats[i].cpu);
        mem_sum += s.stats[i].mem;
    }
    const cores: f32 = @floatFromInt(@max(info.NCPU, 1));
    const cpu_share = cpu_sum / (cores * 100.0) * 100.0;

    const top = ui.div(c).row().wFull().gap(12).pb(12);
    graphCard(s, c, "CONTAINERS · CPU", format.percent(c, cpu_share), .cpu, c.theme.accent, 100);
    graphCard(s, c, "CONTAINERS · MEMORY", format.bytes(c, mem_sum), .mem, c.theme.success, @floatFromInt(@max(info.MemTotal, 1)));
    top.end();

    const bottom = ui.div(c).row().wFull().gap(12).pb(12);
    graphCard(s, c, "CONTAINERS RUNNING", c.print("{d}", .{app.runningCount(s)}), .running, .hex(0xff9f0a), @max(s.history.peak(.running) * 2, 4));
    graphCard(s, c, "DISK", format.bytes(c, @as(u64, @intCast(@max(0, if (s.df) |df| df.imageBytes() else 0)))), .disk, c.theme.muted, @max(s.history.peak(.disk) * 1.6, 1));
    bottom.end();

    hostCard(s, c, mem_sum);

    // The three-layer memory model starts here: on macOS the engine lives in a
    // VM, so container figures are "of the VM", never "of the machine". We show
    // the engine's own totals and label them honestly rather than inventing a
    // host number we cannot yet attribute.
    const engine_card = ui.div(c).column().wFull().gap(6).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    ui.text(c, "Engine", .{ .size = 15, .weight = .semibold });
    if (rt) |r| {
        ui.text(c, c.print("{s} {s} · API {s} · storage {s}", .{
            r.kind.label(), r.version.Version, r.version.ApiVersion, info.Driver,
        }), .{ .size = 12, .color = c.theme.muted });
        ui.text(c, c.print("root {s}", .{info.DockerRootDir}), .{
            .size = 12,
            .color = c.theme.muted,
            .truncate = .middle,
        });
        if (info.OperatingSystem.len > 0) {
            ui.text(c, c.print("{s} · {s}/{s} · cgroups {s}", .{
                info.OperatingSystem, info.OSType, info.Architecture, info.CgroupVersion,
            }), .{ .size = 12, .color = c.theme.muted });
        }
    } else {
        ui.text(c, "No engine selected. Open Runtimes to see what was found.", .{ .size = 12, .color = c.theme.muted });
    }
    engine_card.end(); // before the disk breakdown, or the bar becomes its child

    if (s.df) |df| diskBar(c, df, "Disk (engine)");
}

/// The machine, and how much of it the containers account for. This is the
/// question a container monitor owes an answer to and rarely gives: containers
/// use this much, the rest of the machine uses that much.
fn hostCard(s: *app.State, c: *ui.Context, containers_mem: u64) void {
    const total = s.host_sample.mem_total;
    const used = s.host_sample.mem_used;
    if (total == 0) return; // nothing readable here; say nothing rather than zero

    const card = ui.div(c).column().wFull().gap(8).p(14).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer card.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(12);
    ui.text(c, "HOST", .{ .size = 11, .weight = .medium, .color = c.theme.muted });
    ui.text(c, format.bytes(c, used), .{ .size = 18, .weight = .semibold, .tabular = true });
    ui.text(c, c.print("of {s} in use", .{format.bytes(c, total)}), .{ .size = 11.5, .color = c.theme.muted });
    ui.spacer(c);
    ui.text(c, c.print("CPU {s}", .{format.percent(c, s.host_sample.cpu_percent)}), .{
        .size = 11.5,
        .color = c.theme.muted,
        .tabular = true,
    });
    head.end();

    // Three segments now: what the containers hold, what the engine itself
    // holds, and the rest of the machine. On a Mac the engine's share is its
    // VM; here it is the daemon's own processes.
    const in_containers = @min(containers_mem, used);
    const engine = @min(s.host_sample.engine_rss, used - in_containers);
    const elsewhere = used - in_containers - engine;
    const bar = ui.div(c).row().wFull().h(8).rounded(4).bg(c.theme.track);
    const parts = [_]struct { bytes: u64, color: ui.Color }{
        .{ .bytes = in_containers, .color = c.theme.accent },
        .{ .bytes = engine, .color = .hex(0xff9f0a) },
    };
    // Widths come from the machine's total, so the bar is the whole machine
    // and the empty track is the memory nobody is using.
    const span: f32 = 880.0;
    for (parts) |part| {
        if (part.bytes == 0) continue;
        const frac = @as(f32, @floatFromInt(part.bytes)) / @as(f32, @floatFromInt(@max(total, 1)));
        const piece = ui.div(c).w(@max(@as(f32, 2), span * frac)).h(8).rounded(4).bg(part.color);
        defer piece.end();
    }
    bar.end();

    ui.text(c, c.print("containers {s} · engine {s} · elsewhere {s}", .{
        format.bytes(c, in_containers),
        format.bytes(c, engine),
        format.bytes(c, elsewhere),
    }), .{ .size = 11, .color = c.theme.muted });
}

/// A figure and its history: the number answers "now", the graph answers
/// "and lately?".
fn graphCard(s: *app.State, c: *ui.Context, label: []const u8, value: []const u8, which: history.Which, color: ui.Color, scale: ?f32) void {
    const card = ui.div(c).column().grow().gap(8).p(14).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer card.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    ui.text(c, label, .{ .size = 11, .weight = .medium, .color = c.theme.muted });
    ui.spacer(c);
    ui.text(c, value, .{ .size = 18, .weight = .semibold, .tabular = true });
    head.end();

    const series = [_]graph.Series{.{ .values = s.history.values(which), .color = color }};
    graph.cellGraph(c, &series, .{ .height = 76, .scale = scale });
}

fn diskBar(c: *ui.Context, df: api.Df, heading: []const u8) void {
    const imgs = df.imageBytes();
    const conts = if (df.ContainerUsage) |u| u.TotalSize else 0;
    const vols = if (df.VolumeUsage) |u| u.TotalSize else 0;
    const cache = if (df.BuildCacheUsage) |u| u.TotalSize else 0;
    const total: i64 = imgs + conts + vols + cache;
    if (total <= 0) return;

    const box = ui.div(c).column().wFull().gap(8).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer box.end();
    ui.text(c, heading, .{ .size = 15, .weight = .semibold });

    const f = @as(f32, @floatFromInt(total));
    const segs = [_]widgets.Segment{
        .{ .fraction = @as(f32, @floatFromInt(imgs)) / f, .color = c.theme.accent },
        .{ .fraction = @as(f32, @floatFromInt(conts)) / f, .color = c.theme.success },
        .{ .fraction = @as(f32, @floatFromInt(vols)) / f, .color = .hex(0xff9f0a) },
        .{ .fraction = @as(f32, @floatFromInt(cache)) / f, .color = c.theme.muted },
    };
    const bar = ui.div(c).row().wFull().h(8).rounded(4).bg(c.theme.track);
    for (segs) |seg| {
        if (seg.fraction <= 0) continue;
        const piece = ui.div(c).grow().h(8).rounded(4).bg(seg.color);
        defer piece.end();
    }
    bar.end(); // before the legend, or the legend is drawn inside the bar

    ui.text(c, c.print("images {s} · containers {s} · volumes {s} · build cache {s}", .{
        format.bytes(c, @as(u64, @intCast(@max(0, imgs)))),
        format.bytes(c, @as(u64, @intCast(@max(0, conts)))),
        format.bytes(c, @as(u64, @intCast(@max(0, vols)))),
        format.bytes(c, @as(u64, @intCast(@max(0, cache)))),
    }), .{ .size = 11.5, .color = c.theme.muted, .truncate = .ellipsis });
}

pub fn disk(s: *app.State, c: *ui.Context) void {
    widgets.pageHeader(c, "Disk", "as of the last refresh");
    const df = s.df orelse {
        widgets.emptyState(c, "internaldrive", "No disk figures yet", "They arrive with the first completed refresh");
        return;
    };
    const row = ui.div(c).row().wFull().gap(12).pb(12);
    const imgs = df.imageBytes();
    const conts = if (df.ContainerUsage) |u| u.TotalSize else 0;
    const vols = if (df.VolumeUsage) |u| u.TotalSize else 0;
    const cache = if (df.BuildCacheUsage) |u| u.TotalSize else 0;
    widgets.metric(c, "IMAGES", format.bytes(c, @as(u64, @intCast(@max(0, imgs)))), 160);
    widgets.metric(c, "CONTAINERS", format.bytes(c, @as(u64, @intCast(@max(0, conts)))), 160);
    widgets.metric(c, "VOLUMES", format.bytes(c, @as(u64, @intCast(@max(0, vols)))), 160);
    widgets.metric(c, "BUILD CACHE", format.bytes(c, @as(u64, @intCast(@max(0, cache)))), 160);
    widgets.metric(c, "RECLAIMABLE", format.bytesSigned(c, df.reclaimable()), 160);
    row.end(); // explicit: with `defer` the note below became a sixth item in
               // this row and was squeezed to 96pt at the window edge.

    // The four stores as one strip, so the view answers "how is it split"
    // and not only "how much". Widths are shares of the total, and an empty
    // store is skipped rather than drawn as a zero-width sliver.
    const total: u64 = @as(u64, @intCast(@max(0, imgs))) + @as(u64, @intCast(@max(0, conts))) +
        @as(u64, @intCast(@max(0, vols))) + @as(u64, @intCast(@max(0, cache)));
    if (total > 0) {
        const strip = ui.div(c).row().wFull().h(10).rounded(5).bg(c.theme.track);
        const parts = [_]struct { bytes: u64, color: ui.Color }{
            .{ .bytes = @intCast(@max(0, imgs)), .color = c.theme.accent },
            .{ .bytes = @intCast(@max(0, conts)), .color = c.theme.success },
            .{ .bytes = @intCast(@max(0, vols)), .color = .hex(0xff9f0a) },
            .{ .bytes = @intCast(@max(0, cache)), .color = c.theme.muted },
        };
        for (parts) |part| {
            if (part.bytes == 0) continue;
            const frac = @as(f32, @floatFromInt(part.bytes)) / @as(f32, @floatFromInt(total));
            const piece = ui.div(c).w(@max(@as(f32, 3), 880.0 * frac)).h(10).rounded(5).bg(part.color);
            defer piece.end();
        }
        strip.end();
    }

    const note = ui.div(c).column().wFull().gap(4).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer note.end();
    ui.text(c, "What these numbers are", .{ .size = 15, .weight = .semibold });
    ui.text(c, "Read from /system/df, which walks the image and volume stores. It is cached because it is expensive.", .{ .size = 12, .color = c.theme.muted });
    ui.text(c, "On macOS this is space inside the engine's VM. Pruning frees it in the guest; the disk image file itself only shrinks when it is compacted.", .{ .size = 12, .color = c.theme.muted });
}

pub fn runtimes(s: *app.State, c: *ui.Context) void {
    widgets.pageHeader(c, "Runtimes", "every engine found on this machine");
    const list = ui.div(c).column().wFull().gap(8);
    defer list.end();
    for (s.runtimes, 0..) |rt, i| {
        const active = i == s.active;
        const card = ui.div(c).row().wFull().itemsCenter().gap(12).p(14).rounded(10)
            .bg(if (active) c.theme.accent.alpha(0.08) else c.theme.surface)
            .border(if (active) c.theme.accent else c.theme.border);
        defer card.end();

        const dot = ui.div(c).w(8).h(8).rounded(4).bg(if (rt.reachable) c.theme.success else c.theme.danger);
        dot.end();

        const text_col = ui.div(c).column().grow().gap(2);
        ui.text(c, rt.name, .{ .size = 13, .weight = .medium });
        ui.text(c, rt.socket, .{ .size = 11, .color = c.theme.muted, .truncate = .middle });
        if (rt.reachable) {
            ui.text(c, c.print("v{s} · API {s}", .{ rt.version.Version, rt.version.ApiVersion }), .{ .size = 11, .color = c.theme.muted });
        } else {
            ui.text(c, rt.error_text, .{ .size = 11, .color = c.theme.danger });
        }
        text_col.end();

        if (rt.reachable and !active) {
            const sw = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(c.theme.accent.alpha(0.14));
            defer sw.end();
            ui.text(c, "Use", .{ .size = 12, .weight = .medium, .color = c.theme.accent });
            if (sw.clicked()) s.selectRuntime(i);
        } else if (active) {
            ui.text(c, "active", .{ .size = 11, .weight = .medium, .color = c.theme.accent });
        }
    }
}

pub fn settings(s: *app.State, c: *ui.Context) void {
    widgets.pageHeader(c, "Settings", "");
    const box = ui.div(c).column().w(560).gap(10).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer box.end();
    ui.text(c, "About", .{ .size = 15, .weight = .semibold });
    ui.text(c, "berth 0.1 · a native container monitor", .{ .size = 12, .color = c.theme.muted });
    ui.text(c, "Built with nanoui on Zig 0.17. Nothing polls: figures arrive from the engine and the window redraws when they change.", .{ .size = 12, .color = c.theme.muted });
    ui.divider(c);

    ui.text(c, "Appearance", .{ .size = 13, .weight = .medium });
    ui.text(c, "System follows this machine's desktop. The choice is remembered between launches.", .{ .size = 12, .color = c.theme.muted });
    const row = ui.div(c).row().gap(6).pt(4);
    for ([_]prefs.Appearance{ .system, .light, .dark }) |option| {
        const on = s.appearance == option;
        const button = ui.div(c).row().h(28).px(12).itemsCenter().rounded(14)
            .bg(if (on) c.theme.accent.alpha(0.16) else c.theme.surface_hover);
        defer button.end();
        ui.text(c, option.label(), .{
            .size = 12,
            .weight = if (on) .medium else .regular,
            .color = if (on) c.theme.accent else c.theme.text,
        });
        if (button.clicked()) s.setAppearance(option);
    }
    row.end(); // before the next section, or it becomes a child of this row

    ui.divider(c);
    ui.text(c, "Density", .{ .size = 13, .weight = .medium });
    ui.text(c, "Comfortable (44pt rows). Compact arrives with the rest of the settings store.", .{ .size = 12, .color = c.theme.muted });
}
