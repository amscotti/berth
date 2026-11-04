//! Dialog and completion tests for action jobs: create form identity, the
//! cleanup sheet refusal, and a Podman prune completion. Kept apart from the
//! worker tests so both files stay inside the line limit.
const std = @import("std");
const app = @import("app.zig");
const actions = @import("actions.zig");
const api = @import("engine/api.zig");
const create_mod = @import("create.zig");

const Job = actions.Job;

fn testState() app.State {
    return .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
    };
}

fn copy(target: []u8, len: *usize, text: []const u8) bool {
    if (text.len > target.len) return false;
    @memcpy(target[0..text.len], text);
    len.* = text.len;
    return true;
}

test "submit captures the form generation before the worker starts" {
    const MockEngine = @import("engine/test_server.zig").MockEngine;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});
    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.actions.dispatch = false;
    state.client = api.Client.init(state.io, state.gpa, path);

    create_mod.open(&state, .volume);
    state.create_dialog.name.set("data");
    const generation = state.create_dialog.generation;
    create_mod.submit(&state);
    const job = state.actions.job orelse return error.ActionJobMissing;
    try std.testing.expectEqual(generation, job.form_generation);

    // Close and reopen the form while the POST is in flight: the completion
    // belongs to the old generation and must not close the new form.
    create_mod.close(&state);
    create_mod.open(&state, .volume);
    state.create_dialog.name.set("data");
    try std.testing.expect(state.create_dialog.generation != generation);

    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener };
    var worker: ?std.Thread = job.thread;
    defer if (worker) |thread| thread.join();
    try mock.start();
    defer mock.finish() catch {};
    try mock.waitFor(1);
    worker.?.join();
    worker = null;
    job.thread = null;

    state.client = null;
    actions.finish(job);
    try std.testing.expect(state.create_dialog.open == .volume);
    try std.testing.expectEqualStrings("Volume data created", state.toast.?.text);
}

test "the cleanup sheet refuses while an action is running" {
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    const job = try state.gpa.create(Job);
    job.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .prune };
    state.actions.job = job;
    defer {
        state.actions.job = null;
        state.gpa.destroy(job);
    }

    state.openPrune();
    try std.testing.expect(!state.prune_open);
    try std.testing.expect(state.toast.?.bad);
    try std.testing.expectEqualStrings("cleanup is running", state.toast.?.text);

    // A different action still blocks the sheet, with the generic wording.
    job.kind = .action;
    state.openPrune();
    try std.testing.expect(!state.prune_open);
    try std.testing.expectEqualStrings("wait for the current engine operation to finish", state.toast.?.text);
}

test "a podman prune completion does not report a hidden build-cache skip" {
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    const job = try state.gpa.create(Job);
    job.* = .{
        .state = &state,
        .gpa = state.gpa,
        .io = state.io,
        .kind = .prune,
        .engine = .podman,
        .reclaimed = 100,
    };
    state.actions.job = job;
    actions.finish(job);

    try std.testing.expect(!state.toast.?.bad);
    try std.testing.expectEqualStrings("Freed 100 B", state.toast.?.text);
}

test "a create closes the dialog on success and keeps it on failure" {
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    state.create_dialog.open = .volume;
    state.create_dialog.generation = 1;
    state.create_dialog.name.set("data");
    const created = try state.gpa.create(Job);
    created.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .create, .ok = true };
    created.create_kind = .volume;
    created.form_generation = 1;
    try std.testing.expect(copy(&created.name, &created.name_len, "data"));
    state.actions.job = created;
    actions.finish(created);
    try std.testing.expect(state.create_dialog.open == null);
    try std.testing.expectEqualStrings("Volume data created", state.toast.?.text);

    state.create_dialog.open = .network;
    state.create_dialog.generation = 2;
    state.create_dialog.name.set("backend");
    const failed = try state.gpa.create(Job);
    failed.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .create };
    failed.create_kind = .network;
    failed.form_generation = 2;
    try std.testing.expect(copy(&failed.name, &failed.name_len, "backend"));
    failed.error_text = try std.heap.page_allocator.dupe(u8, "name already in use");
    state.actions.job = failed;
    actions.finish(failed);
    try std.testing.expect(state.create_dialog.open == .network);
    try std.testing.expectEqualStrings("backend", state.create_dialog.name.text());
    try std.testing.expect(state.toast.?.bad);
    try std.testing.expect(std.mem.indexOf(u8, state.toast.?.text, "name already in use") != null);

    // The same kind and name in a newer form is still a different form.
    state.create_dialog.open = .volume;
    state.create_dialog.generation = 3;
    state.create_dialog.name.set("data");
    const stale = try state.gpa.create(Job);
    stale.* = .{ .state = &state, .gpa = state.gpa, .io = state.io, .kind = .create, .ok = true };
    stale.create_kind = .volume;
    stale.form_generation = 2;
    try std.testing.expect(copy(&stale.name, &stale.name_len, "data"));
    state.actions.job = stale;
    actions.finish(stale);
    try std.testing.expect(state.create_dialog.open == .volume);
    try std.testing.expectEqualStrings("data", state.create_dialog.name.text());
}

test "a refused action keeps its confirmation" {
    var state = testState();
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    state.pending = .{
        .id = try state.gpa.dupe(u8, "abc"),
        .name = try state.gpa.dupe(u8, "web"),
        .method = "POST",
        .path = try state.gpa.dupe(u8, "/containers/abc/stop"),
        .verb = "Stop",
    };
    var fake: Job = undefined;
    state.actions.job = &fake;
    actions.resolvePending(&state, true);
    // The busy operation refused the request; the user can retry.
    try std.testing.expect(state.pending != null);
    state.actions.job = null;
    state.clearPending();
}
