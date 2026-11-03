//! The Events view: one row per thing the engine did while berth watched.
//! Newest first, searchable on the fields the row shows, fed by the refresh
//! worker's interval poll rather than a streaming connection.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const engine_events = @import("../../engine/events.zig");
const format = @import("../format.zig");
const widgets = @import("../widgets.zig");

const ROW = 34.0;

const full_cols = [_]widgets.Col{
    .{ .name = "TIME", .width = 90 },
    .{ .name = "TYPE", .width = 90 },
    .{ .name = "ACTION", .width = 110 },
    .{ .name = "ACTOR", .width = 220 },
    .{ .name = "DETAIL", .width = 260 },
};

const medium_cols = [_]widgets.Col{
    .{ .name = "TIME", .width = 80 },
    .{ .name = "TYPE", .width = 80 },
    .{ .name = "ACTION", .width = 100 },
    .{ .name = "ACTOR", .width = 180 },
    .{ .name = "DETAIL", .width = 120 },
};

const compact_cols = [_]widgets.Col{
    .{ .name = "TIME", .width = 80 },
    .{ .name = "TYPE", .width = 70 },
    .{ .name = "ACTION", .width = 100 },
    .{ .name = "ACTOR", .width = 160 },
};

const Cols = struct {
    header: []const widgets.Col,
    time: f32,
    kind: f32,
    action: f32,
    actor: f32,
    detail: f32 = 0,
};

fn columns(width: f32) Cols {
    if (width >= 834) return .{ .header = &full_cols, .time = 90, .kind = 90, .action = 110, .actor = 220, .detail = 260 };
    if (width >= 630) return .{ .header = &medium_cols, .time = 80, .kind = 80, .action = 100, .actor = 180, .detail = 120 };
    return .{ .header = &compact_cols, .time = 80, .kind = 70, .action = 100, .actor = 160 };
}

pub fn view(s: *app.State, c: *ui.Context) void {
    if (s.last_update == 0 and s.error_text.len == 0) {
        _ = widgets.pageHeaderSearch(c, "Events", "Reading engine…", null);
        widgets.loadingRows(c, 6);
        return;
    }

    const q = c.alloc().dupe(u8, s.search.events.text()) catch "";
    var shown: usize = 0;
    for (s.events.items) |event| {
        if (event.matches(q)) shown += 1;
    }
    const subtitle = if (q.len > 0 and shown < s.events.items.len)
        c.print("{d} of {d} shown", .{ shown, s.events.items.len })
    else
        widgets.count(c, s.events.items.len, "event", "events");
    if (widgets.pageHeaderSearch(c, "Events", subtitle, .{
        .buffer = &s.search.events,
        .placeholder = if (s.layout.content < 760) "Search" else "Search events",
        .width = if (s.layout.content < 760) 132 else 190,
    }).cleared) s.redraw();

    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);
    if (s.events.items.len == 0) {
        if (q.len > 0) {
            if (widgets.emptyStateAction(c, "list.bullet", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every event", "Clear search")) {
                s.search.events.clear();
                s.redraw();
            }
        } else {
            _ = widgets.emptyStateAction(c, "list.bullet", "No events yet", "Exits, restarts, OOMs and cleanups land here", null);
        }
        return;
    }
    if (shown == 0) {
        if (widgets.emptyStateAction(c, "list.bullet", c.print("No results for \u{201c}{s}\u{201d}", .{q}), "Clear the search to see every event", "Clear search")) {
            s.search.events.clear();
            s.redraw();
        }
        return;
    }

    const cols = columns(s.layout.content);
    widgets.tableHeader(c, cols.header, null);

    const order = c.alloc().alloc(usize, shown) catch return;
    var n: usize = 0;
    for (s.events.items, 0..) |event, i| {
        if (!event.matches(q)) continue;
        order[n] = i;
        n += 1;
    }

    var rows = ui.list(c, .{ .count = shown, .row_height = ROW });
    while (rows.next()) |row_index| {
        const event = s.events.items[order[row_index]];
        var row = ui.div(c).row().wFull().h(ROW).px(12).itemsCenter().gap(10).rounded(4)
            .bg(if (row_index % 2 == 1) c.theme.surface_hover.alpha(0.35) else c.theme.surface);
        defer row.end();

        widgets.cellMuted(c, format.started(c, event.seconds(), format.now()), cols.time, false);
        widgets.cellMuted(c, event.kind, cols.kind, false);
        widgets.pillCell(c, event.action, cols.action);
        widgets.cell(c, actor(event), cols.actor, false);
        if (cols.detail > 0) widgets.cellMuted(c, detail(c, event), cols.detail, false);
    }
    rows.end();
}

fn actor(event: engine_events.Event) []const u8 {
    if (event.name.len > 0) return event.name;
    if (event.id.len > 19 and std.mem.startsWith(u8, event.id, "sha256:")) return event.id[7..19];
    return event.id;
}

fn detail(c: *ui.Context, event: engine_events.Event) []const u8 {
    if (std.mem.eql(u8, event.action, "die") and event.exit_code.len > 0) {
        return c.print("exit {s}", .{event.exit_code});
    }
    if (event.image.len > 0) return event.image;
    if (event.id.len > 0) return event.id;
    return "—";
}
