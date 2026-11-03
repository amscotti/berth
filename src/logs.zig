//! The container log pane's options and its on-demand reads: follow, local
//! timestamps, in-pane search and copy. Split from `app.zig` so the state
//! machine stays inside the line rule.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const format = @import("ui/format.zig");
const lifetime = @import("engine/lifetime.zig");

pub const State = struct {
    follow: bool = true,
    timestamps: bool = false,
    search: ui.TextBuffer = .{},

    pub fn deinit(self: *State) void {
        self.search.deinit();
    }
};

pub fn clear(s: *app.State) void {
    if (s.log_buf) |buf| s.gpa.free(buf);
    s.log_buf = null;
    s.log_text = "";
}

pub fn load(s: *app.State, id: []const u8) void {
    clear(s);
    var client = s.client orelse return;
    const text = client.logs(id, 500, s.log_opts.timestamps) catch {
        s.log_text = "could not read this container's logs";
        s.redraw();
        return;
    };
    const owned: ?[]u8 = if (s.log_opts.timestamps)
        localize(s.gpa, text)
    else
        s.gpa.dupe(u8, text) catch null;
    if (owned) |owned_text| {
        s.log_buf = owned_text;
        s.log_text = owned_text;
    } else {
        s.log_text = "could not read this container's logs";
    }
    s.redraw();
}

pub fn reload(s: *app.State) void {
    if (s.detail_kind != .container) return;
    const id = s.selected orelse return;
    load(s, id);
}

/// A tail fetched by the refresh worker, committed only if the pane is still
/// showing the same container. This is what keeps follow from blocking the UI
/// thread every five seconds.
pub fn commitSnapshot(s: *app.State, text: []const u8, id: []const u8, timestamps: bool) void {
    if (text.len == 0 or id.len == 0) return;
    if (s.detail_kind != .container) return;
    const selected = s.selected orelse return;
    if (!std.mem.eql(u8, selected, id)) return;
    clear(s);
    const owned: ?[]u8 = if (timestamps)
        localize(s.gpa, text)
    else
        s.gpa.dupe(u8, text) catch null;
    if (owned) |owned_text| {
        s.log_buf = owned_text;
        s.log_text = owned_text;
    } else {
        s.log_text = "could not read this container's logs";
    }
}

pub fn copy(s: *app.State) void {
    if (s.log_text.len == 0) return;
    nanoui.clipboard.writeText(s.log_text);
    s.notify("Logs copied", false);
}

/// The lines the pane shows this frame: the whole tail, or the lines matching
/// the search. Allocated in the frame arena because it is painted after this
/// function returns.
pub fn visible(s: *app.State, c: *ui.Context) []const u8 {
    const query = s.log_opts.search.text();
    if (query.len == 0) return s.log_text;

    var out: std.ArrayList(u8) = .empty;
    var lines = std.mem.splitScalar(u8, s.log_text, '\n');
    var any = false;
    while (lines.next()) |line| {
        if (!contains(line, query)) continue;
        if (any) out.append(c.alloc(), '\n') catch break;
        out.appendSlice(c.alloc(), line) catch break;
        any = true;
    }
    if (!any) return "no lines match";
    return out.items;
}

fn contains(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > haystack.len) return false;
    var i: usize = 0;
    outer: while (i + needle.len <= haystack.len) : (i += 1) {
        for (needle, 0..) |n, j| {
            if (std.ascii.toLower(haystack[i + j]) != std.ascii.toLower(n)) continue :outer;
        }
        return true;
    }
    return false;
}

/// Docker writes RFC 3339 UTC; a person reads the clock on their desk.
fn localize(gpa: std.mem.Allocator, text: []const u8) []u8 {
    var out: std.ArrayList(u8) = .empty;
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |line| {
        const space = std.mem.indexOfScalar(u8, line, ' ') orelse line.len;
        const stamp = line[0..space];
        if (lifetime.rfc3339(stamp)) |unix| {
            var buf: [16]u8 = undefined;
            out.appendSlice(gpa, format.clockBuf(&buf, unix)) catch return gpa.dupe(u8, text) catch "";
        } else {
            out.appendSlice(gpa, stamp) catch return gpa.dupe(u8, text) catch "";
        }
        if (space < line.len) out.appendSlice(gpa, line[space..]) catch break;
        out.append(gpa, '\n') catch break;
    }
    if (out.items.len > 0 and out.items[out.items.len - 1] == '\n') _ = out.pop();
    return out.toOwnedSlice(gpa) catch gpa.dupe(u8, text) catch "";
}

test "log search matches whole lines case-insensitively" {
    try std.testing.expect(contains("alpha one", "ALPHA"));
    try std.testing.expect(contains("beta two", "two"));
    try std.testing.expect(!contains("beta two", "alpha"));
    try std.testing.expect(contains("anything", ""));
}
