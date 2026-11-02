//! Bulk selection for containers: the checkbox mode, the toolbar actions, and
//! the one confirmation that names how many rows are about to change. Actions
//! run on a worker so five hundred stop requests cannot freeze the window.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const api = @import("engine/api.zig");

pub const max = 512;
const id_len = 64;

pub const Route = enum {
    start,
    stop,
    delete,

    pub fn verb(self: Route) []const u8 {
        return switch (self) {
            .start => "Started",
            .stop => "Stopped",
            .delete => "Deleted",
        };
    }

    pub fn word(self: Route) []const u8 {
        return switch (self) {
            .start => "Start",
            .stop => "Stop",
            .delete => "Delete",
        };
    }

    fn method(self: Route) []const u8 {
        return switch (self) {
            .start, .stop => "POST",
            .delete => "DELETE",
        };
    }
};

pub const Pending = struct {
    count: usize,
    route: Route,
};

pub const State = struct {
    enabled: bool = false,
    ids: [max][id_len]u8 = undefined,
    lens: [max]u8 = undefined,
    count: usize = 0,
    pending: ?Pending = null,
    job: ?*Job = null,

    pub fn id(self: *const State, index: usize) []const u8 {
        if (index >= self.count) return "";
        return self.ids[index][0..self.lens[index]];
    }

    pub fn has(self: *const State, container_id: []const u8) bool {
        for (0..self.count) |i| {
            if (std.mem.eql(u8, self.id(i), container_id)) return true;
        }
        return false;
    }

    pub fn toggle(self: *State, container_id: []const u8) void {
        for (0..self.count) |i| {
            if (!std.mem.eql(u8, self.id(i), container_id)) continue;
            const last = self.count - 1;
            if (i != last) {
                self.ids[i] = self.ids[last];
                self.lens[i] = self.lens[last];
            }
            self.count = last;
            return;
        }
        if (self.count == max or container_id.len > id_len) return;
        const n = container_id.len;
        @memcpy(self.ids[self.count][0..n], container_id);
        self.lens[self.count] = @intCast(n);
        self.count += 1;
    }

    /// Leaves selection mode; a Cancel and an engine switch both use it.
    pub fn clear(self: *State) void {
        self.enabled = false;
        self.count = 0;
        self.pending = null;
    }

    /// Drop selections that no longer exist, so a refresh cannot leave the
    /// toolbar promising to stop a container the engine no longer has.
    pub fn retain(self: *State, containers: []const api.Container) void {
        var kept: usize = 0;
        for (0..self.count) |i| {
            const wanted = self.id(i);
            var found = false;
            for (containers) |ct| {
                if (std.mem.eql(u8, ct.Id, wanted)) {
                    found = true;
                    break;
                }
            }
            if (!found) continue;
            if (kept != i) {
                self.ids[kept] = self.ids[i];
                self.lens[kept] = self.lens[i];
            }
            kept += 1;
        }
        self.count = kept;
    }

    pub fn running(self: *const State) bool {
        return self.job != null;
    }
};

/// The toolbar line while a worker is stopping or deleting rows.
pub fn runningLabel(state: *const State) []const u8 {
    const job = state.job orelse return "";
    return switch (job.route) {
        .start => "Starting",
        .stop => "Stopping",
        .delete => "Deleting",
    };
}

const Job = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    route: Route,
    total: usize,
    done: usize = 0,
    socket: [512]u8 = undefined,
    socket_len: usize = 0,
    api_version: [16]u8 = undefined,
    api_len: usize = 0,
    ids: [max][id_len]u8 = undefined,
    lens: [max]u8 = undefined,
    count: usize,
    first_error: []const u8 = "",
    thread: ?std.Thread = null,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

pub fn startSelected(s: *app.State) void {
    startJob(s, .start);
}

pub fn stopSelected(s: *app.State) void {
    s.bulk.pending = .{ .count = s.bulk.count, .route = .stop };
    s.redraw();
}

pub fn deleteSelected(s: *app.State) void {
    s.bulk.pending = .{ .count = s.bulk.count, .route = .delete };
    s.redraw();
}

pub fn resolve(s: *app.State, ok: bool) void {
    const pending = s.bulk.pending orelse return;
    s.bulk.pending = null;
    if (!ok) {
        s.redraw();
        return;
    }
    startJob(s, pending.route);
}

fn startJob(s: *app.State, route: Route) void {
    if (s.bulk.running()) return;
    var client = s.client orelse {
        s.notify("no engine is selected", true);
        return;
    };
    if (s.bulk.count == 0) return;

    const job = s.gpa.create(Job) catch {
        s.notify("could not start the bulk action", true);
        return;
    };
    job.* = .{
        .gpa = s.gpa,
        .io = s.io,
        .route = route,
        .total = s.bulk.count,
        .count = s.bulk.count,
    };
    job.socket_len = @min(client.socket.len, job.socket.len);
    @memcpy(job.socket[0..job.socket_len], client.socket[0..job.socket_len]);
    job.api_len = @min(client.api.len, job.api_version.len);
    @memcpy(job.api_version[0..job.api_len], client.api[0..job.api_len]);
    @memcpy(job.ids[0..s.bulk.count], s.bulk.ids[0..s.bulk.count]);
    @memcpy(job.lens[0..s.bulk.count], s.bulk.lens[0..s.bulk.count]);

    job.thread = std.Thread.spawn(.{}, work, .{ s, job }) catch {
        s.gpa.destroy(job);
        s.notify("could not start the bulk action", true);
        return;
    };
    s.bulk.job = job;
    s.bulk.clear();
    s.redraw();
}

fn work(s: *app.State, job: *Job) void {
    const page = std.heap.page_allocator;
    var arena = std.heap.ArenaAllocator.init(page);
    defer arena.deinit();
    var client = api.Client.init(job.io, page, job.socket[0..job.socket_len]);
    client.api = job.api_version[0..job.api_len];
    client.alloc = arena.allocator();

    var path_buf: [128]u8 = undefined;
    var i: usize = 0;
    while (i < job.count) : (i += 1) {
        if (job.canceled.load(.acquire)) break;
        const id = job.ids[i][0..job.lens[i]];
        const path = switch (job.route) {
            .start => std.fmt.bufPrint(&path_buf, "/containers/{s}/start", .{id}) catch break,
            .stop => std.fmt.bufPrint(&path_buf, "/containers/{s}/stop", .{id}) catch break,
            .delete => std.fmt.bufPrint(&path_buf, "/containers/{s}?force=1", .{id}) catch break,
        };
        client.action(job.route.method(), path) catch {
            if (job.first_error.len == 0 and client.err_body.len > 0) {
                job.first_error = page.dupe(u8, client.err_body) catch "";
            }
            continue;
        };
        job.done += 1;
    }
    if (job.canceled.load(.acquire)) {
        if (job.first_error.len > 0) page.free(job.first_error);
        return; // shutdown: the job is deliberately abandoned
    }
    nanoui.App.after(0, s, finishBridge);
}

fn finishBridge(ctx: ?*anyopaque) callconv(.c) void {
    const s: *app.State = @ptrCast(@alignCast(ctx.?));
    finish(s);
}

fn finish(s: *app.State) void {
    const job = s.bulk.job orelse return;
    if (job.thread) |thread| thread.join();
    s.bulk.job = null;
    const route = job.route;
    const done = job.done;
    const total = job.total;
    const error_text = s.gpa.dupe(u8, job.first_error) catch "";
    const page = std.heap.page_allocator;
    if (job.first_error.len > 0) page.free(job.first_error);
    s.gpa.destroy(job);

    if (done == total) {
        s.notifyFmt("{s} {d} containers", .{ route.verb(), done }, false);
    } else {
        s.notifyFmt("{s} {d} of {d} containers", .{ route.verb(), done, total }, true);
        if (error_text.len > 0) {
            s.notifyFmt("{s}", .{firstLine(error_text)}, true);
        }
    }
    if (error_text.len > 0) s.gpa.free(error_text);
    s.refresh();
    s.redraw();
}

fn firstLine(text: []const u8) []const u8 {
    const end = std.mem.indexOfScalar(u8, text, '\n') orelse @min(text.len, 90);
    return text[0..@min(end, 90)];
}

pub fn cancel(s: *app.State) void {
    const job = s.bulk.job orelse return;
    job.canceled.store(true, .release);
    if (job.thread) |thread| thread.detach();
    s.bulk.job = null;
}

pub fn confirmDialog(s: *app.State, c: *ui.Context) void {
    const pending = s.bulk.pending orelse return;
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.danger);
    ui.text(c, pending.route.word(), .{ .size = 13, .weight = .semibold, .color = c.theme.danger });
    ui.text(c, c.print("{d} containers — this cannot be undone", .{pending.count}), .{
        .size = 12,
        .color = c.theme.muted,
    });
    const row = ui.div(c).row().wFull().justifyEnd().gap(8);
    if (bulkButton(c, "Cancel", false, false)) resolve(s, false);
    if (bulkButton(c, pending.route.word(), true, true)) resolve(s, true);
    row.end();
    box.end();
}

fn bulkButton(c: *ui.Context, label: []const u8, danger: bool, primary: bool) bool {
    const bg = if (!primary) c.theme.surface_hover else if (danger) c.theme.danger else c.theme.accent;
    const box = ui.div(c).row().w(92).h(28).center().rounded(14).bg(bg).control();
    ui.text(c, label, .{
        .size = 12,
        .weight = if (primary) .medium else .regular,
        .color = if (primary) c.theme.accent_text else c.theme.text,
    });
    const clicked = box.clicked();
    box.end();
    return clicked;
}

test "selection toggles by id and forgets removed containers" {
    var state: State = .{};
    state.enabled = true;
    state.toggle("abc");
    state.toggle("def");
    try std.testing.expect(state.has("abc"));
    state.toggle("abc");
    try std.testing.expect(!state.has("abc"));
    try std.testing.expectEqual(@as(usize, 1), state.count);

    const containers = [_]api.Container{
        .{ .Id = "def", .State = "running" },
        .{ .Id = "xyz", .State = "exited" },
    };
    state.toggle("gone");
    state.retain(containers[0..]);
    try std.testing.expectEqual(@as(usize, 1), state.count);
    try std.testing.expectEqualStrings("def", state.id(0));
    state.clear();
    try std.testing.expect(!state.enabled);
}
