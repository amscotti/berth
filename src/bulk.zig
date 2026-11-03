//! Bulk selection for containers: the checkbox mode, the toolbar actions, and
//! the one confirmation that names how many rows are about to change. Actions
//! run on a worker so five hundred stop requests cannot freeze the window.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const api = @import("engine/api.zig");
const widgets = @import("ui/widgets.zig");

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
    /// The project a pending start/stop-all belongs to, copied so a refresh
    /// that rotates the label arena cannot make the dialog point at freed
    /// text before the user confirms.
    project: [128]u8 = undefined,
    project_len: usize = 0,

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
        self.project_len = 0;
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
    /// The Compose project this batch belongs to, for the completion toast.
    project: [128]u8 = undefined,
    project_len: usize = 0,
    thread: ?std.Thread = null,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

pub fn startSelected(s: *app.State) void {
    var ids: [max][]const u8 = undefined;
    const n = selection(s, &ids);
    if (startBatch(s, .start, ids[0..n], "")) s.bulk.clear();
}

pub fn stopSelected(s: *app.State) void {
    s.bulk.project_len = 0;
    s.bulk.pending = .{ .count = s.bulk.count, .route = .stop };
    s.redraw();
}

pub fn deleteSelected(s: *app.State) void {
    s.bulk.project_len = 0;
    s.bulk.pending = .{ .count = s.bulk.count, .route = .delete };
    s.redraw();
}

/// "Start all" on a Compose project: every stopped container in it, through
/// the same worker and toast as a checkbox batch.
pub fn startProject(s: *app.State, project: []const u8) void {
    var ids: [max][]const u8 = undefined;
    const n = projectIds(s.containers, project, .start, &ids);
    _ = startBatch(s, .start, ids[0..n], project);
}

/// "Stop all" goes through the confirmation: it changes several containers at
/// once, and the dialog names the project so it is clear what is being stopped.
pub fn stopProject(s: *app.State, project: []const u8) void {
    if (project.len == 0 or project.len > s.bulk.project.len) return;
    var ids: [max][]const u8 = undefined;
    const n = projectIds(s.containers, project, .stop, &ids);
    if (n == 0) return;
    s.bulk.project_len = project.len;
    @memcpy(s.bulk.project[0..s.bulk.project_len], project);
    s.bulk.pending = .{ .count = n, .route = .stop };
    s.redraw();
}

pub fn resolve(s: *app.State, ok: bool) void {
    const pending = s.bulk.pending orelse return;
    s.bulk.pending = null;
    if (!ok) {
        s.bulk.project_len = 0;
        s.redraw();
        return;
    }
    if (s.bulk.project_len > 0) {
        const project = s.bulk.project[0..s.bulk.project_len];
        var ids: [max][]const u8 = undefined;
        const n = projectIds(s.containers, project, pending.route, &ids);
        s.bulk.project_len = 0;
        if (n == 0) {
            s.notifyFmt("nothing to {s} in {s}", .{ pending.route.word(), project }, true);
            return;
        }
        _ = startBatch(s, pending.route, ids[0..n], project);
        return;
    }
    startJob(s, pending.route);
}

/// The confirmation re-reads the project each frame it is open, so its count
/// and the action `resolve` will take cannot disagree.
pub fn countProject(s: *app.State, project: []const u8, route: Route) usize {
    var ids: [max][]const u8 = undefined;
    return projectIds(s.containers, project, route, &ids);
}

fn selection(s: *app.State, out: *[max][]const u8) usize {
    const n = @min(s.bulk.count, max);
    for (0..n) |i| out[i] = s.bulk.id(i);
    return n;
}

/// The project's containers that the route would actually change; a stopped
/// container cannot be stopped again, and one that is up cannot be started.
/// Paused counts as up, exactly as the engine reports it.
fn projectIds(containers: []const api.Container, project: []const u8, route: Route, out: *[max][]const u8) usize {
    var n: usize = 0;
    for (containers) |ct| {
        if (ct.Id.len == 0 or ct.Id.len > id_len) continue;
        const p = ct.project() orelse continue;
        if (!std.mem.eql(u8, p, project)) continue;
        const up = ct.isUp();
        if (route == .start and up) continue;
        if (route == .stop and !up) continue;
        out[n] = ct.Id;
        n += 1;
        if (n == max) break;
    }
    return n;
}

fn startJob(s: *app.State, route: Route) void {
    var ids: [max][]const u8 = undefined;
    const n = selection(s, &ids);
    if (startBatch(s, route, ids[0..n], "")) s.bulk.clear();
}

/// Returns whether the worker started; a failed start leaves the caller's
/// selection alone so the user can retry.
fn startBatch(s: *app.State, route: Route, ids: []const []const u8, project: []const u8) bool {
    // Any queued confirmation is about a set this batch is replacing.
    s.bulk.pending = null;
    s.bulk.project_len = 0;
    if (s.bulk.running()) return false;
    if (ids.len == 0 or ids.len > max) return false;
    var client = s.client orelse {
        s.notify("no engine is selected", true);
        return false;
    };

    const job = s.gpa.create(Job) catch {
        s.notify("could not start the bulk action", true);
        return false;
    };
    job.* = .{
        .gpa = s.gpa,
        .io = s.io,
        .route = route,
        .total = ids.len,
        .count = ids.len,
    };
    job.socket_len = @min(client.socket.len, job.socket.len);
    @memcpy(job.socket[0..job.socket_len], client.socket[0..job.socket_len]);
    job.api_len = @min(client.api.len, job.api_version.len);
    @memcpy(job.api_version[0..job.api_len], client.api[0..job.api_len]);
    job.project_len = @min(project.len, job.project.len);
    @memcpy(job.project[0..job.project_len], project[0..job.project_len]);
    for (ids, 0..) |id, i| {
        job.lens[i] = @intCast(@min(id.len, id_len));
        @memcpy(job.ids[i][0..job.lens[i]], id[0..job.lens[i]]);
    }

    job.thread = std.Thread.spawn(.{}, work, .{ s, job }) catch {
        s.gpa.destroy(job);
        s.notify("could not start the bulk action", true);
        return false;
    };
    s.bulk.job = job;
    s.redraw();
    return true;
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
    var project_buf: [128]u8 = undefined;
    const project_len = job.project_len;
    @memcpy(project_buf[0..project_len], job.project[0..project_len]);
    const project = project_buf[0..project_len];
    const error_text = s.gpa.dupe(u8, job.first_error) catch "";
    const page = std.heap.page_allocator;
    if (job.first_error.len > 0) page.free(job.first_error);
    s.gpa.destroy(job);

    if (done == total) {
        if (project.len > 0) {
            s.notifyFmt("{s} {d} in {s}", .{ route.verb(), done, project }, false);
        } else {
            s.notifyFmt("{s} {d} container{s}", .{ route.verb(), done, plural(done) }, false);
        }
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

fn plural(n: usize) []const u8 {
    return if (n == 1) "" else "s";
}

pub fn cancel(s: *app.State) void {
    const job = s.bulk.job orelse return;
    job.canceled.store(true, .release);
    if (job.thread) |thread| thread.detach();
    s.bulk.job = null;
}

pub fn confirmDialog(s: *app.State, c: *ui.Context) void {
    const pending = s.bulk.pending orelse return;
    const project = s.bulk.project[0..s.bulk.project_len];
    const count = if (project.len > 0) countProject(s, project, pending.route) else pending.count;
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.danger);
    ui.text(c, pending.route.word(), .{ .size = 13, .weight = .semibold, .color = c.theme.danger });
    const line = if (count == 0)
        c.print("nothing left to {s} in {s}", .{ pending.route.word(), project })
    else if (project.len > 0)
        c.print("{d} container{s} in {s} will be stopped", .{ count, plural(count), project })
    else if (pending.route == .delete)
        c.print("{d} container{s} — this cannot be undone", .{ count, plural(count) })
    else
        c.print("{d} container{s} will be stopped", .{ count, plural(count) });
    ui.text(c, line, .{
        .size = 12,
        .color = c.theme.muted,
    });
    const row = ui.div(c).row().wFull().justifyEnd().gap(8);
    if (widgets.dialogButton(c, "Cancel", false, false)) resolve(s, false);
    if (widgets.dialogButton(c, pending.route.word(), true, true)) resolve(s, true);
    row.end();
    box.end();
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

test "a project batch takes only the containers the route would change" {
    var running = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer running.deinit();
    var stopped = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer stopped.deinit();
    var other = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"other\"}", .{});
    defer other.deinit();
    const items = [_]api.Container{
        .{ .Id = "up", .State = "running", .Labels = running.value },
        .{ .Id = "down", .State = "exited", .Labels = stopped.value },
        .{ .Id = "elsewhere", .State = "running", .Labels = other.value },
    };
    var out: [max][]const u8 = undefined;
    // Start takes the stopped container; Stop takes the running one. A
    // different project is never touched.
    try std.testing.expectEqual(@as(usize, 1), projectIds(items[0..], "shop", .start, &out));
    try std.testing.expectEqualStrings("down", out[0]);
    try std.testing.expectEqual(@as(usize, 1), projectIds(items[0..], "shop", .stop, &out));
    try std.testing.expectEqualStrings("up", out[0]);
    try std.testing.expectEqual(@as(usize, 0), projectIds(items[0..], "missing", .stop, &out));
}
