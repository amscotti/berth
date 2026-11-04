//! Bulk selection for containers: the checkbox mode, the toolbar actions, and
//! the one confirmation that names how many rows are about to change. Actions
//! run on a worker so five hundred stop requests cannot freeze the window.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const api = @import("engine/api.zig");
const widgets = @import("ui/widgets.zig");
const search = @import("ui/views/search.zig");
const sheets = @import("sheets.zig");
const bulk_batch = @import("bulk_batch.zig");
const bulk_action = @import("bulk_action.zig");

pub const max = bulk_batch.max;
const id_len = bulk_batch.id_len;

pub const Route = bulk_action.Route;
const Job = bulk_action.Job;
const plural = bulk_action.plural;

pub const Pending = struct {
    route: Route,
};

pub const State = struct {
    enabled: bool = false,
    ids: [max][id_len]u8 = undefined,
    lens: [max]u8 = undefined,
    count: usize = 0,
    pending: ?Pending = null,
    /// Frozen when a confirmation opens; later ticks or a refresh cannot change it.
    pending_batch: bulk_batch.Batch = .{},
    job: ?*Job = null,
    /// Set by Cancel while the worker stops between items; the bar reads it.
    canceling: bool = false,
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
        // An open confirmation names a frozen set: a tick behind the inline
        // dialog would make its count and the Confirm button disagree.
        if (self.pending != null) return;
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

    /// Selects every row the current filter and search leave on screen, in
    /// the engine's order, up to the same 512-row cap as manual ticks. The
    /// table can show at most that many, so the button cannot promise rows
    /// the list never painted.
    pub fn selectShown(self: *State, containers: []const api.Container, filter: app.Filter, query: []const u8) void {
        // A confirmation names a frozen set; letting the bar change it behind
        // the dialog would make the count and the worker disagree.
        if (self.pending != null) return;
        self.count = 0;
        for (containers) |*ct| {
            if (!filter.keeps(ct)) continue;
            if (!search.keepContainer(query, ct)) continue;
            if (ct.Id.len == 0 or ct.Id.len > id_len) continue;
            if (self.count == max) break;
            const n = ct.Id.len;
            @memcpy(self.ids[self.count][0..n], ct.Id);
            self.lens[self.count] = @intCast(n);
            self.count += 1;
        }
    }

    /// "None" clears the ticks without leaving checkbox mode.
    pub fn selectNone(self: *State) void {
        if (self.pending != null) return;
        self.count = 0;
    }

    /// Turns checkbox mode on or off. Leaving it drops the batch, as Cancel
    /// does; entering it starts empty so ids from an earlier batch cannot come
    /// back without being ticked again.
    pub fn setEnabled(self: *State, on: bool) void {
        self.enabled = on;
        self.count = 0;
        self.pending = null;
        self.pending_batch.count = 0;
        self.project_len = 0;
    }

    /// The Select chip. It must not go through `clear`, which would turn the
    /// mode back off on the same click.
    pub fn toggleMode(self: *State) void {
        self.setEnabled(!self.enabled);
    }

    /// Leaves selection mode; a Cancel and an engine switch both use it.
    pub fn clear(self: *State) void {
        self.setEnabled(false);
    }

    /// Drop selections that no longer exist, so a refresh cannot leave the
    /// toolbar promising to stop a container the engine no longer has. The
    /// open confirmation shrinks with them, so its count cannot promise a row
    /// that resolve will then have to skip.
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
        self.pending_batch.retain(containers);
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

pub fn startSelected(s: *app.State) void {
    if (runSelected(s, .start)) s.bulk.clear();
}

pub fn stopSelected(s: *app.State) void {
    var ids: [max][]const u8 = undefined;
    const n = selectedIds(s.containers, &s.bulk, .stop, &ids);
    if (n == 0) {
        noChange(s, .stop);
        return;
    }
    sheets.dismiss(s);
    s.bulk.project_len = 0;
    s.bulk.pending_batch.snapshot(ids[0..n]);
    s.bulk.pending = .{ .route = .stop };
    s.redraw();
}

pub fn deleteSelected(s: *app.State) void {
    var ids: [max][]const u8 = undefined;
    const n = selectedIds(s.containers, &s.bulk, .delete, &ids);
    if (n == 0) {
        noChange(s, .delete);
        return;
    }
    sheets.dismiss(s);
    s.bulk.project_len = 0;
    s.bulk.pending_batch.snapshot(ids[0..n]);
    s.bulk.pending = .{ .route = .delete };
    s.redraw();
}

/// "Start all" on a Compose project: every stopped container in it, through
/// the same worker and toast as a checkbox batch.
pub fn startProject(s: *app.State, project: []const u8) void {
    var ids: [max][]const u8 = undefined;
    const n = projectIds(s.containers, project, .start, &ids);
    _ = bulk_action.start(s, .start, ids[0..n], project);
}

/// "Stop all" goes through the confirmation: it changes several containers at
/// once, and the dialog names the project so it is clear what is being stopped.
pub fn stopProject(s: *app.State, project: []const u8) void {
    if (project.len == 0 or project.len > s.bulk.project.len) return;
    var ids: [max][]const u8 = undefined;
    const n = projectIds(s.containers, project, .stop, &ids);
    if (n == 0) return;
    sheets.dismiss(s);
    s.bulk.project_len = project.len;
    @memcpy(s.bulk.project[0..s.bulk.project_len], project);
    s.bulk.pending = .{ .route = .stop };
    s.redraw();
}

pub fn resolve(s: *app.State, ok: bool) void {
    const pending = s.bulk.pending orelse return;
    if (!ok) {
        s.bulk.pending = null;
        s.bulk.pending_batch.count = 0;
        s.bulk.project_len = 0;
        s.redraw();
        return;
    }
    if (s.bulk.project_len > 0) {
        const project = s.bulk.project[0..s.bulk.project_len];
        var ids: [max][]const u8 = undefined;
        const n = projectIds(s.containers, project, pending.route, &ids);
        if (n == 0) {
            // Nothing left to change is not a failure; do not flash red.
            s.notifyFmt("nothing to {s} in {s}", .{ pending.route.word(), project }, false);
            s.bulk.pending = null;
            s.bulk.pending_batch.count = 0;
            s.bulk.project_len = 0;
            s.redraw();
            return;
        }
        // `start` clears the pending question only once its worker exists.
        _ = bulk_action.start(s, pending.route, ids[0..n], project);
        return;
    }

    var ids: [max][]const u8 = undefined;
    const n = s.bulk.pending_batch.live(s.containers, &ids);
    if (n == 0) {
        noChange(s, pending.route);
        s.bulk.pending = null;
        s.bulk.pending_batch.count = 0;
        s.redraw();
        return;
    }
    if (bulk_action.start(s, pending.route, ids[0..n], "")) s.bulk.clear();
}

/// The confirmation re-reads the project each frame it is open, so its count
/// and the action `resolve` will take cannot disagree.
pub fn countProject(s: *app.State, project: []const u8, route: Route) usize {
    var ids: [max][]const u8 = undefined;
    return projectIds(s.containers, project, route, &ids);
}

/// The selected rows the route would actually change, in selection order.
/// Rows the engine no longer lists are skipped, like `retain` would have done.
fn selectedIds(containers: []const api.Container, state: *const State, route: Route, out: *[max][]const u8) usize {
    var n: usize = 0;
    for (0..state.count) |i| {
        const id = state.id(i);
        const ct = findContainer(containers, id) orelse continue;
        if (!route.changes(ct.isUp())) continue;
        out[n] = id;
        n += 1;
    }
    return n;
}

fn findContainer(containers: []const api.Container, id: []const u8) ?*const api.Container {
    for (containers) |*ct| {
        if (std.mem.eql(u8, ct.Id, id)) return ct;
    }
    return null;
}

/// A batch with nothing to change is not a failure, so it does not flash
/// "Started 0 containers" in red. The reason stays neutral: paused and
/// vanished rows are not "already running" or "already stopped".
fn noChange(s: *app.State, route: Route) void {
    s.notifyFmt("nothing to {s}: no selected container needs it", .{@tagName(route)}, false);
}

/// Sends the selection through the worker, filtered to what the route changes.
fn runSelected(s: *app.State, route: Route) bool {
    var ids: [max][]const u8 = undefined;
    const n = selectedIds(s.containers, &s.bulk, route, &ids);
    if (n == 0) {
        noChange(s, route);
        return false;
    }
    return bulk_action.start(s, route, ids[0..n], "");
}

/// The project's containers that the route would actually change. Paused
/// counts as up, exactly as the engine reports it.
fn projectIds(containers: []const api.Container, project: []const u8, route: Route, out: *[max][]const u8) usize {
    var n: usize = 0;
    for (containers) |ct| {
        if (ct.Id.len == 0 or ct.Id.len > id_len) continue;
        const p = ct.project() orelse continue;
        if (!std.mem.eql(u8, p, project)) continue;
        if (!route.changes(ct.isUp())) continue;
        out[n] = ct.Id;
        n += 1;
        if (n == max) break;
    }
    return n;
}

/// User Cancel: stop between items and let the worker's finish report.
pub fn cancel(s: *app.State) void {
    bulk_action.cancel(s);
}

/// Teardown: abandon the worker rather than wait for a socket.
pub fn discard(s: *app.State) void {
    bulk_action.discard(s);
}

pub fn confirmDialog(s: *app.State, c: *ui.Context) void {
    const pending = s.bulk.pending orelse return;
    // The shell clears page focus when the sheet opens; this is the second
    // guard against a background field taking the sheet's keys.
    const keys = c.focus == 0;
    if (keys and c.shortcut(.{ .key = ui.Shortcut.escape })) {
        resolve(s, false);
        return;
    }
    const enter = keys and c.input.key == .enter;
    const project = s.bulk.project[0..s.bulk.project_len];
    const count = if (project.len > 0) countProject(s, project, pending.route) else s.bulk.pending_batch.count;
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.danger);
    const title_at: u32 = @intCast(c.nodes.items.len);
    ui.text(c, pending.route.word(), .{ .size = 13, .weight = .semibold, .color = c.theme.danger });
    ui.access.describe(c, title_at, .{ .role = .static_text, .label = pending.route.word(), .header = true });
    const line = if (count == 0 and project.len > 0)
        c.print("nothing left to {s} in {s}", .{ @tagName(pending.route), project })
    else if (count == 0)
        c.print("nothing to {s}", .{@tagName(pending.route)})
    else if (project.len > 0)
        c.print("{d} container{s} in {s} will be stopped", .{ count, plural(count), project })
    else if (pending.route == .delete)
        c.print("{d} container{s} — this cannot be undone", .{ count, plural(count) })
    else
        c.print("{d} container{s} will be stopped", .{ count, plural(count) });
    ui.text(c, line, .{
        .size = 12,
        .color = c.theme.muted,
        .wrap = true,
    });
    const row = ui.div(c).row().wFull().justifyEnd().gap(8);
    if (widgets.dialogButton(c, "Cancel", false, false)) resolve(s, false);
    // Nothing left to run: only Cancel, so the dialog cannot offer an action
    // that resolve would immediately turn into a no-op.
    if (count > 0 and (enter or widgets.dialogButton(c, pending.route.word(), true, true))) resolve(s, true);
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
    state.pending_batch.snapshot(&[_][]const u8{ "def", "gone" });
    state.toggle("gone");
    state.retain(containers[0..]);
    try std.testing.expectEqual(@as(usize, 1), state.count);
    try std.testing.expectEqualStrings("def", state.id(0));
    // The open confirmation loses the vanished id in the same pass.
    try std.testing.expectEqual(@as(usize, 1), state.pending_batch.count);
    try std.testing.expectEqualStrings("def", state.pending_batch.id(0));
    state.clear();
    try std.testing.expect(!state.enabled);
}

test "the Select chip turns the mode on and off" {
    var state: State = .{};
    state.toggle("abc");
    state.toggleMode();
    try std.testing.expect(state.enabled);
    // Entering the mode starts empty: the old selection must not come back.
    try std.testing.expectEqual(@as(usize, 0), state.count);

    state.toggle("abc");
    state.toggleMode();
    try std.testing.expect(!state.enabled);
    try std.testing.expectEqual(@as(usize, 0), state.count);

    state.toggleMode();
    try std.testing.expect(state.enabled);
    try std.testing.expectEqual(@as(usize, 0), state.count);
}

test "an open confirmation freezes the selection" {
    var state: State = .{ .enabled = true };
    state.toggle("a");
    state.pending = .{ .route = .stop };
    state.toggle("b");
    state.toggle("a");
    try std.testing.expectEqual(@as(usize, 1), state.count);
    try std.testing.expect(state.has("a"));
    state.pending = null;
    state.toggle("b");
    try std.testing.expectEqual(@as(usize, 2), state.count);
}

test "a checkbox batch takes only the rows the route would change" {
    const items = [_]api.Container{
        .{ .Id = "up", .State = "running" },
        .{ .Id = "down", .State = "exited" },
        .{ .Id = "held", .State = "paused" },
    };
    var state: State = .{ .enabled = true };
    state.toggle("up");
    state.toggle("down");
    state.toggle("held");
    state.toggle("vanished");

    var out: [max][]const u8 = undefined;
    try std.testing.expectEqual(@as(usize, 1), selectedIds(items[0..], &state, .start, &out));
    try std.testing.expectEqualStrings("down", out[0]);

    // Paused counts as up, exactly as it does for a project batch.
    try std.testing.expectEqual(@as(usize, 2), selectedIds(items[0..], &state, .stop, &out));
    try std.testing.expectEqualStrings("up", out[0]);
    try std.testing.expectEqualStrings("held", out[1]);

    // Delete keeps everything the engine still lists; a vanished row is not
    // sent, since there is nothing left to remove.
    try std.testing.expectEqual(@as(usize, 3), selectedIds(items[0..], &state, .delete, &out));
}

test "a confirmed batch is the set resolve sends" {
    const before = [_]api.Container{ .{ .Id = "a", .State = "running" }, .{ .Id = "b", .State = "exited" } };
    var state: State = .{ .enabled = true };
    state.toggle("a");
    state.toggle("b");
    var ids: [max][]const u8 = undefined;
    const n = selectedIds(before[0..], &state, .stop, &ids);
    try std.testing.expectEqual(@as(usize, 1), n);
    state.pending_batch.snapshot(ids[0..n]);
    var sent: [max][]const u8 = undefined;
    try std.testing.expectEqual(@as(usize, 1), state.pending_batch.live(before[0..], &sent));
    try std.testing.expectEqualStrings("a", sent[0]);

    // The tick moves to `b` and `b` starts: neither may replace the batch the
    // dialog named. `a` merely changing state stays in it.
    state.toggle("a");
    const after = [_]api.Container{ .{ .Id = "a", .State = "exited" }, .{ .Id = "b", .State = "running" } };
    try std.testing.expectEqual(@as(usize, 1), selectedIds(after[0..], &state, .stop, &ids));
    try std.testing.expectEqualStrings("b", ids[0]);

    try std.testing.expectEqual(@as(usize, 1), state.pending_batch.live(after[0..], &sent));
    try std.testing.expectEqualStrings("a", sent[0]);
}

test "select all shown respects the filter, the search and the cap" {
    const items = [_]api.Container{
        .{ .Id = "web-up", .Names = &.{"/web-up"}, .State = "running" },
        .{ .Id = "web-down", .Names = &.{"/web-down"}, .State = "exited" },
        .{ .Id = "db-up", .Names = &.{"/db-up"}, .State = "running" },
    };
    var state: State = .{ .enabled = true };
    state.selectShown(items[0..], .running, "web");
    try std.testing.expectEqual(@as(usize, 1), state.count);
    try std.testing.expectEqualStrings("web-up", state.id(0));

    state.selectShown(items[0..], .all, "");
    try std.testing.expectEqual(@as(usize, 3), state.count);
    state.selectNone();
    try std.testing.expectEqual(@as(usize, 0), state.count);

    // The list itself stops at 512; a "select all" over a bigger snapshot
    // must not grow a batch the worker cannot carry.
    var many: [max + 1]api.Container = undefined;
    var ids: [max + 1][8]u8 = undefined;
    for (&many, &ids, 0..) |*ct, *id, i| {
        const text = std.fmt.bufPrint(id, "c{d}", .{i}) catch unreachable;
        ct.* = .{ .Id = text, .State = "running" };
    }
    state.selectShown(many[0..], .all, "");
    try std.testing.expectEqual(@as(usize, max), state.count);
    try std.testing.expectEqualStrings("c0", state.id(0));
    try std.testing.expectEqualStrings("c511", state.id(max - 1));
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

test "a refused bulk action keeps its confirmation" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    var items = [_]api.Container{.{ .Id = "abc", .State = "running" }};
    state.containers = items[0..];
    state.bulk.enabled = true;
    state.bulk.pending = .{ .route = .stop };
    state.bulk.pending_batch.snapshot(&[_][]const u8{"abc"});

    // No client is a refusal before the worker exists. The dialog and the
    // frozen batch must survive so the user can retry, not vanish.
    resolve(&state, true);
    try std.testing.expect(state.bulk.pending != null);
    try std.testing.expectEqual(@as(usize, 1), state.bulk.pending_batch.count);
}

test "a refused project bulk action keeps its question" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    var labels = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, "{\"com.docker.compose.project\":\"shop\"}", .{});
    defer labels.deinit();
    var items = [_]api.Container{.{ .Id = "abc", .State = "running", .Labels = labels.value }};
    state.containers = items[0..];
    state.bulk.pending = .{ .route = .stop };
    state.bulk.project_len = 4;
    @memcpy(state.bulk.project[0..4], "shop");

    resolve(&state, true);
    try std.testing.expect(state.bulk.pending != null);
    try std.testing.expectEqual(@as(usize, 4), state.bulk.project_len);
}

test "an emptied project confirmation is neutral, not an error" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    state.bulk.pending = .{ .route = .stop };
    state.bulk.project_len = 4;
    @memcpy(state.bulk.project[0..4], "shop");
    resolve(&state, true);
    try std.testing.expect(state.bulk.pending == null);
    try std.testing.expect(state.toast != null);
    try std.testing.expect(!state.toast.?.bad);
}
