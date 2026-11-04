//! The menu-bar mini-monitor: engine state, running count and the containers'
//! CPU share, updated with every committed refresh — and with every failed
//! one, which is exactly when it gets looked at. The menu's Open item brings
//! the window back; on macOS closing the window leaves the app in the menu
//! bar.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const format = @import("ui/format.zig");

pub fn create(s: *app.State) void {
    const template = [_]nanoui.MenuItem{
        .{ .label = "Open berth", .id = "show", .click = nanoui.MenuClick.of(s, openItem) },
        nanoui.MenuItem.separator,
        .{ .label = "Quit berth", .role = .quit },
    };
    if (s.app.newTray(.{
        .symbol = "cube",
        .tooltip = "berth",
        .menu = &template,
    })) |tray| {
        s.tray = tray;
        update(s);
    } else |_| {}
}

fn openItem(s: *app.State, _: *nanoui.menu.Item) void {
    show(s);
}

pub fn show(s: *app.State) void {
    if (s.window) |window| {
        window.show();
        window.focus();
        return;
    }
    if (s.recreate) |reopen| reopen(s);
    if (s.window) |window| window.focus();
}

pub fn update(s: *app.State) void {
    const tray = s.tray orelse return;
    const runtime = s.activeRuntime();
    // `reachable` is one discovery scan stale; the last refresh's error is
    // not. Either one means the title must not promise containers that are
    // no longer there.
    const live = runtime != null and runtime.?.reachable and s.error_text.len == 0;
    // Reading lasts until the first snapshot commits; "engine down" would
    // report a failure that has not happened while the client is connecting.
    const reading = s.last_update == 0 and (s.discovery_job != null or s.error_text.len == 0);
    var title_buf: [96]u8 = undefined;
    const title = if (reading)
        "berth · reading engine…"
    else if (!live)
        "berth · engine down"
    else blk: {
        var mem: u64 = 0;
        for (s.stats) |sample| {
            if (sample.sampled) mem += sample.mem;
        }
        var bytes_buf: [32]u8 = undefined;
        // A partial sum is not current memory: say unknown until every up
        // container answered, matching the overview card.
        const mem_text = if (s.stats_complete) format.bytesBuf(&bytes_buf, mem) else "—";
        const cpu = s.history.latestCpu();
        break :blk if (cpu) |v|
            std.fmt.bufPrint(&title_buf, "{d} up · {d:.1}% · {s}", .{
                app.runningCount(s),
                v,
                mem_text,
            }) catch "berth"
        else
            std.fmt.bufPrint(&title_buf, "{d} up · — · {s}", .{
                app.runningCount(s),
                mem_text,
            }) catch "berth";
    };
    tray.setTitle(title);
}
