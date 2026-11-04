//! Panels: the overview, disk usage, runtimes and settings.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const stats_pool = @import("../../stats_pool.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");
const graph = @import("../graph.zig");
const history = @import("../history.zig");
const theme = @import("../theme.zig");

pub fn overview(s: *app.State, c: *ui.Context) void {
    const rt = s.activeRuntime();
    // Reading lasts until the first snapshot commits, not just until adoption:
    // there is a client but no data for the moment in between.
    const first = widgets.firstRead(s.last_update, s.error_text);
    const reading = first == .reading;
    const ready = first == .ready;
    const subtitle = if (reading and rt == null)
        "Reading engine…"
    else if (rt) |r|
        r.name
    else
        "no engine";
    widgets.pageHeader(c, "Overview", subtitle);

    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);
    if (s.df_error_text.len > 0) widgets.errorBanner(c, s.df_error_text);

    const info = if (rt) |r| r.info else api.Info{};

    // What the containers are using, as it moves: the figures nanomon's
    // overview is built from, with berth's own colours.
    const mem_sum = stats_pool.memoryUsed(s.containers, s.stats);
    const cores: f32 = @floatFromInt(@max(info.NCPU, 1));
    const cpu_share = stats_pool.aggregateCpuPercent(s.containers, s.stats, cores);
    const connected = if (rt) |r| r.reachable else false;
    const unknown = "—";
    const mem_value = if (ready and connected and s.stats_complete) format.bytes(c, mem_sum) else unknown;
    const mem_note = if (reading)
        "reading engine…"
    else if (!ready)
        // A failed first pass has no sample to call partial.
        "no figures yet"
    else if (!connected)
        "no engine answering"
    else if (s.stats_complete)
        c.print("of {s}", .{format.bytes(c, info.MemTotal)})
    else
        "partial sample";
    const cpu_value = if (ready and connected and s.stats_complete and cpu_share != null)
        format.percent(c, cpu_share.?)
    else
        unknown;
    const cpu_note = if (!ready)
        (if (reading) "reading engine…" else "no figures yet")
    else if (!connected)
        "no engine answering"
    else if (!s.stats_complete)
        "partial sample"
    else if (cpu_share == null)
        "no CPU sample"
    else
        c.print("of {d:.0} cores", .{cores});
    const running_value = if (ready and connected) c.print("{d}", .{app.runningCount(s)}) else unknown;
    const running_note = if (!ready)
        (if (reading) "reading engine…" else "no figures yet")
    else if (connected)
        c.print("of {d} containers", .{s.containers.len})
    else
        "no engine answering";
    const disk_note = if (reading)
        "reading engine…"
    else if (!ready)
        "no figures yet"
    else if (s.df_error)
        "could not read disk figures"
    else if (!connected)
        "no engine answering"
    else if (s.df) |df|
        c.print("reclaimable {s}", .{format.bytesSigned(c, df.reclaimable())})
    else
        "no figures yet";
    // Two equal columns: `grow` hands out leftover space after each card's
    // intrinsic content, which made the CPU card narrower than Memory.
    const col_w = (s.layout.content - 12) / 2;

    const top = ui.div(c).row().wFull().gap(12).pb(12);
    graphCard(
        s,
        c,
        "cpu",
        "Containers · CPU",
        cpu_value,
        cpu_note,
        .cpu,
        theme.accentFill(c.theme),
        theme.accentText(c.theme),
        100,
        col_w,
    );
    graphCard(
        s,
        c,
        "memorychip",
        "Containers · Memory",
        mem_value,
        mem_note,
        .mem,
        c.theme.success,
        theme.successText(c.theme),
        @floatFromInt(@max(info.MemTotal, 1)),
        col_w,
    );
    top.end();

    const disk_bytes: u64 = if (s.df) |df| @intCast(@max(0, df.totalBytes())) else 0;
    const bottom = ui.div(c).row().wFull().gap(12).pb(12);
    graphCard(
        s,
        c,
        "square.grid.3x3",
        "Containers Running",
        running_value,
        running_note,
        .running,
        .hex(0xd9a441),
        theme.runningText(c.theme),
        // The whole fleet is the scale: one running container is one unit,
        // not half the card, and the height does not move as a peak ages out.
        graph.countScale(s.containers.len),
        col_w,
    );
    graphCard(
        s,
        c,
        "internaldrive",
        "Disk",
        if (ready and connected and s.df != null) format.bytes(c, disk_bytes) else unknown,
        disk_note,
        .disk,
        c.theme.muted,
        c.theme.muted,
        // A rounded ceiling rather than `peak * 1.6`: a flat store keeps one
        // height while old samples fall off the ring.
        graph.ceiling(s.history.peak(.disk)),
        col_w,
    );
    bottom.end();

    hostCard(s, c, if (connected and s.stats_complete) @as(?u64, mem_sum) else null);

    // The three-layer memory model starts here: on macOS the engine lives in a
    // VM, so container figures are "of the VM", never "of the machine". We show
    // the engine's own totals and label them honestly rather than inventing a
    // host number we cannot yet attribute.
    const engine_card = ui.div(c).column().wFull().gap(6).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    const engine_head = ui.div(c).row().wFull().itemsCenter().gap(8);
    widgets.chip(c, "cube", c.theme.accent, 20);
    ui.text(c, "Engine", .{ .size = 15, .weight = .semibold });
    engine_head.end();
    if (rt) |r| {
        if (r.reachable) {
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
            ui.text(c, "Nothing is answering on the sockets berth knows. Open Runtimes to see what was tried.", .{ .size = 12, .color = c.theme.muted });
        }
    } else if (reading) {
        ui.text(c, "Reading engine… sockets are being probed in the background.", .{ .size = 12, .color = c.theme.muted });
    } else {
        ui.text(c, "No engine selected. Open Runtimes to see what was found.", .{ .size = 12, .color = c.theme.muted });
    }
    engine_card.end(); // before the disk breakdown, or the bar becomes its child

    if (s.df) |df| diskBar(c, df, "Disk (engine)");
}

/// The machine, and how much of it the containers account for. This is the
/// question a container monitor owes an answer to and rarely gives: containers
/// use this much, the rest of the machine uses that much. `containers_mem` is
/// null when any up container was unsampled, because the partial sum would
/// otherwise land in "elsewhere" and look exact.
fn hostCard(s: *app.State, c: *ui.Context, containers_mem: ?u64) void {
    const total = s.host_sample.mem_total;
    const used = s.host_sample.mem_used;
    if (total == 0) return; // nothing readable here; say nothing rather than zero

    const card = ui.div(c).column().wFull().gap(8).p(14).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer card.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(12);
    widgets.chip(c, "desktopcomputer", c.theme.muted, 20);
    ui.text(c, "Host", .{ .size = 12.5, .weight = .semibold, .color = c.theme.muted });
    ui.text(c, format.bytes(c, used), .{ .size = 18, .weight = .semibold, .tabular = true });
    ui.text(c, c.print("of {s} in use", .{format.bytes(c, total)}), .{ .size = 11.5, .color = c.theme.muted });
    ui.spacer(c);
    // The first sample has no previous tick count, so its zero is not "idle";
    // it is "no delta yet". Sampling once per refresh means it is unknown for
    // one pass only.
    ui.text(c, c.print("CPU {s}", .{
        if (s.host_sample.cpu_known) format.percent(c, s.host_sample.cpu_percent) else "—",
    }), .{
        .size = 11.5,
        .color = c.theme.muted,
        .tabular = true,
    });
    head.end();

    const containers_known = containers_mem orelse {
        ui.text(c, "containers — · per-container stats incomplete", .{
            .size = 11,
            .color = c.theme.muted,
        });
        return;
    };

    // Three segments: what the containers hold (a working set measured inside
    // the engine's VM on macOS), the engine's own host processes, and the rest
    // of the machine. Guest RAM is never counted into the engine segment — it
    // carries the containers, which are already the first segment.
    const in_containers = @min(containers_known, used);
    const engine = @min(s.host_sample.engine_rss, used - in_containers);
    const elsewhere = used - in_containers - engine;
    const bar = ui.div(c).row().wFull().h(8).rounded(4).bg(c.theme.track).clip();
    const parts = [_]struct { bytes: u64, color: ui.Color }{
        .{ .bytes = in_containers, .color = theme.accentFill(c.theme) },
        .{ .bytes = engine, .color = theme.warning_fill },
        .{ .bytes = elsewhere, .color = c.theme.muted },
    };
    // Each share grows into the track, so the bar is the whole machine at any
    // window width. The unused share is an explicit, invisible leaf: one grow
    // child alone would take the whole track regardless of its weight.
    var used_frac: f32 = 0;
    for (parts) |part| {
        if (part.bytes == 0) continue;
        const frac = @as(f32, @floatFromInt(part.bytes)) / @as(f32, @floatFromInt(@max(total, 1)));
        used_frac += frac;
        _ = c.leaf(.{ .grow = frac, .height = .{ .px = 8 }, .radius = 4, .background = part.color }, .none, null);
    }
    if (used_frac < 1) {
        _ = c.leaf(.{ .grow = 1 - used_frac, .height = .{ .px = 8 }, .background = c.theme.track }, .none, null);
    }
    bar.end();

    ui.text(c, c.print("containers {s} · engine helpers {s} · elsewhere {s}", .{
        format.bytes(c, in_containers),
        format.bytes(c, engine),
        format.bytes(c, elsewhere),
    }), .{ .size = 11, .color = c.theme.muted });
}

/// A figure and its history: the number answers "now", the graph answers
/// "and lately?".
fn graphCard(
    s: *app.State,
    c: *ui.Context,
    icon: []const u8,
    label: []const u8,
    value: []const u8,
    note: []const u8,
    which: history.Which,
    color: ui.Color,
    text_color: ui.Color,
    scale: ?f32,
    width: f32,
) void {
    const card = ui.div(c).column().w(width).gap(9).p(14).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer card.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    widgets.chip(c, icon, text_color, 20);
    ui.text(c, label, .{ .size = 12.5, .weight = .semibold, .color = text_color });
    ui.spacer(c);
    ui.text(c, "recent", .{ .size = 9.5, .color = c.theme.muted.alpha(0.8) });
    head.end();

    const figure = ui.div(c).row().wFull().itemsEnd().gap(8);
    ui.text(c, value, .{ .size = 26, .weight = .bold, .tabular = true });
    ui.spacer(c);
    ui.text(c, note, .{ .size = 11, .color = c.theme.muted, .truncate = .ellipsis });
    figure.end();

    const series = [_]graph.Series{.{ .values = s.history.values(which), .color = color }};
    graph.cellGraph(c, &series, .{ .height = 70, .scale = scale });
}

fn diskBar(c: *ui.Context, df: api.Df, heading: []const u8) void {
    const store = df.stores();
    const imgs = store.images;
    const conts = store.containers;
    const vols = store.volumes;
    const cache = store.build_cache;
    // The same clamped shares the Disk page draws decide whether there is a
    // bar at all; a negative engine sentinel must not hide a store that the
    // other view would render.
    const shares = df.shares();
    if (shares.total() <= 0) return;

    const box = ui.div(c).column().wFull().gap(8).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer box.end();
    ui.text(c, heading, .{ .size = 15, .weight = .semibold });

    const segs = [_]widgets.Segment{
        .{ .fraction = shares.images, .color = theme.accentFill(c.theme) },
        .{ .fraction = shares.containers, .color = c.theme.success },
        .{ .fraction = shares.volumes, .color = theme.store_volume },
        .{ .fraction = shares.build_cache, .color = c.theme.muted },
    };
    const bar = ui.div(c).row().wFull().h(8).rounded(4).bg(c.theme.track).clip();
    // Weighted leaves, not one `grow` flag per segment: equal grow flags made
    // every non-empty store the same width. The unused share is explicit for
    // the same reason as the host bar — a lone weighted child takes the track.
    var used: f32 = 0;
    for (segs) |seg| {
        if (seg.fraction <= 0) continue;
        used += seg.fraction;
        _ = c.leaf(.{ .grow = seg.fraction, .height = .{ .px = 8 }, .radius = 4, .background = seg.color }, .none, null);
    }
    if (used < 1) {
        _ = c.leaf(.{ .grow = 1 - used, .height = .{ .px = 8 }, .background = c.theme.track }, .none, null);
    }
    bar.end(); // before the legend, or the legend is drawn inside the bar

    // The swatches above the words, so the bar is readable without guessing
    // which colour is which store.
    const legend = ui.div(c).row().wFull().gap(16);
    legendItem(c, "images", format.bytes(c, @as(u64, @intCast(@max(0, imgs)))), theme.accentFill(c.theme));
    legendItem(c, "containers", format.bytes(c, @as(u64, @intCast(@max(0, conts)))), c.theme.success);
    legendItem(c, "volumes", format.bytes(c, @as(u64, @intCast(@max(0, vols)))), theme.store_volume);
    legendItem(c, "build cache", format.bytes(c, @as(u64, @intCast(@max(0, cache)))), c.theme.muted);
    legend.end();
}

fn legendItem(c: *ui.Context, label: []const u8, value: []const u8, color: ui.Color) void {
    const row = ui.div(c).row().itemsCenter().gap(5);
    defer row.end();
    const dot = ui.div(c).w(6).h(6).rounded(3).bg(color);
    dot.end();
    ui.text(c, if (value.len == 0) label else c.print("{s} {s}", .{ label, value }), .{ .size = 11.5, .color = c.theme.muted });
}

pub fn disk(s: *app.State, c: *ui.Context) void {
    const first = widgets.firstRead(s.last_update, s.error_text);
    if (first == .reading) {
        widgets.pageHeader(c, "Disk", "Reading engine…");
        widgets.loadingRows(c, 3);
        return;
    }
    if (first == .failed) {
        widgets.pageHeader(c, "Disk", "engine not answering");
        widgets.errorBanner(c, s.error_text);
        widgets.emptyState(c, "internaldrive", "Engine not answering", widgets.engine_help);
        return;
    }
    const disk_subtitle = if (s.df_error and s.df != null) "last disk read failed" else "as of the last refresh";
    widgets.pageHeader(c, "Disk", disk_subtitle);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);
    if (s.df_error_text.len > 0) widgets.errorBanner(c, s.df_error_text);
    const df = s.df orelse {
        if (s.error_text.len > 0) {
            widgets.emptyState(c, "internaldrive", "Engine not answering", widgets.engine_help);
        } else if (s.df_error) {
            widgets.emptyState(c, "internaldrive", "Could not read disk figures", "The engine answered, but /system/df did not");
        } else {
            widgets.emptyState(c, "internaldrive", "No disk figures yet", "They arrive with the first completed refresh");
        }
        return;
    };
    const row = ui.div(c).row().wFull().gap(12).pb(12);
    const store = df.stores();
    const imgs = store.images;
    const conts = store.containers;
    const vols = store.volumes;
    const cache = store.build_cache;
    // Five figures share the row; fixed 160s need 848pt and clip at the 960
    // minimum. Let them take an equal share of what the page actually has.
    const metric_w = @max(120, (s.layout.content - 4 * 12) / 5);
    widgets.metric(c, "IMAGES", format.bytes(c, @as(u64, @intCast(@max(0, imgs)))), metric_w);
    widgets.metric(c, "CONTAINERS", format.bytes(c, @as(u64, @intCast(@max(0, conts)))), metric_w);
    widgets.metric(c, "VOLUMES", format.bytes(c, @as(u64, @intCast(@max(0, vols)))), metric_w);
    widgets.metric(c, "BUILD CACHE", format.bytes(c, @as(u64, @intCast(@max(0, cache)))), metric_w);
    widgets.metric(c, "RECLAIMABLE", format.bytesSigned(c, df.reclaimable()), metric_w);
    row.end(); // explicit: with `defer` the note below became a sixth item in
    // this row and was squeezed to 96pt at the window edge.

    // The four stores as one strip, so the view answers "how is it split"
    // and not only "how much". Widths are shares of the total, and an empty
    // store is skipped rather than drawn as a zero-width sliver.
    const shares = df.shares();
    if (shares.total() > 0) {
        const strip = ui.div(c).row().wFull().h(10).rounded(5).bg(c.theme.track).clip();
        const parts = [_]struct { fraction: f32, color: ui.Color }{
            .{ .fraction = shares.images, .color = theme.accentFill(c.theme) },
            .{ .fraction = shares.containers, .color = c.theme.success },
            .{ .fraction = shares.volumes, .color = theme.store_volume },
            .{ .fraction = shares.build_cache, .color = c.theme.muted },
        };
        for (parts) |part| {
            if (part.fraction <= 0) continue;
            _ = c.leaf(.{ .grow = part.fraction, .height = .{ .px = 10 }, .radius = 5, .background = part.color }, .none, null);
        }
        strip.end();

        const legend = ui.div(c).row().wFull().gap(18).pt(8).pb(12);
        legendItem(c, "images", "", theme.accentFill(c.theme));
        legendItem(c, "containers", "", c.theme.success);
        legendItem(c, "volumes", "", theme.store_volume);
        legendItem(c, "build cache", "", c.theme.muted);
        legend.end();
    }

    const note = ui.div(c).column().wFull().gap(4).p(16).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer note.end();
    ui.text(c, "What these numbers are", .{ .size = 15, .weight = .semibold });
    ui.text(c, "Read from /system/df, which walks the image and volume stores. It is cached because it is expensive.", .{ .size = 12, .color = c.theme.muted });
    ui.text(c, "On macOS this is space inside the engine's VM. Pruning frees it in the guest; the disk image file itself only shrinks when it is compacted.", .{ .size = 12, .color = c.theme.muted });
    const actions = ui.div(c).row().wFull().justifyEnd().pt(4);
    const clean = ui.div(c).row().h(26).px(10).itemsCenter().rounded(13).bg(c.theme.accent);
    ui.text(c, "Clean up unused…", .{ .size = 12, .weight = .medium, .color = c.theme.accent_text });
    if (clean.clicked()) s.openPrune();
    clean.end();
    actions.end();
}
