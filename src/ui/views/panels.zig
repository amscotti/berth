//! Panels: the overview, disk usage, runtimes and settings.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");

pub fn overview(s: *app.State, c: *ui.Context) void {
    const rt = s.activeRuntime();
    widgets.pageHeader(c, "Overview", if (rt) |r| r.name else "no engine");

    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);

    const cards = ui.div(c).row().wFull().gap(12).pb(12);

    const info = if (rt) |r| r.info else api.Info{};
    widgets.metric(c, "CONTAINERS", c.print("{d}/{d}", .{ info.ContainersRunning, info.Containers }), 150);
    widgets.metric(c, "IMAGES", c.print("{d}", .{info.Images}), 120);
    widgets.metric(c, "ENGINE CPUs", c.print("{d}", .{info.NCPU}), 130);
    widgets.metric(c, "ENGINE MEMORY", format.bytes(c, info.MemTotal), 150);
    cards.end();

    // The three-layer memory model starts here: on macOS the engine lives in a
    // VM, so container figures are "of the VM", never "of the machine". We show
    // the engine's own totals and label them honestly rather than inventing a
    // host number we cannot yet attribute.
    const engine_card = ui.div(c).column().wFull().gap(6).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer engine_card.end();
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

    if (s.df) |df| diskBar(c, df, "Disk (engine)");
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
    defer bar.end();
    for (segs) |seg| {
        if (seg.fraction <= 0) continue;
        const piece = ui.div(c).grow().h(8).rounded(4).bg(seg.color);
        defer piece.end();
    }

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
    defer row.end();
    const imgs = df.imageBytes();
    const conts = if (df.ContainerUsage) |u| u.TotalSize else 0;
    const vols = if (df.VolumeUsage) |u| u.TotalSize else 0;
    const cache = if (df.BuildCacheUsage) |u| u.TotalSize else 0;
    widgets.metric(c, "IMAGES", format.bytes(c, @as(u64, @intCast(@max(0, imgs)))), 160);
    widgets.metric(c, "CONTAINERS", format.bytes(c, @as(u64, @intCast(@max(0, conts)))), 160);
    widgets.metric(c, "VOLUMES", format.bytes(c, @as(u64, @intCast(@max(0, vols)))), 160);
    widgets.metric(c, "BUILD CACHE", format.bytes(c, @as(u64, @intCast(@max(0, cache)))), 160);
    widgets.metric(c, "RECLAIMABLE", format.bytesSigned(c, df.reclaimable()), 160);

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
    ui.text(c, "Density", .{ .size = 13, .weight = .medium });
    ui.text(c, "Comfortable (40pt rows). Compact arrives with the settings store.", .{ .size = 12, .color = c.theme.muted });
    _ = s;
}
