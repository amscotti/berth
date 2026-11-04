//! The refresh worker's log-follow gate: selecting a row copies its id, but
//! only a pass that captured follow=true may spend a request on the tail.
const std = @import("std");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const refresh = @import("refresh.zig");
const Snapshot = @import("snapshot.zig").Snapshot;
const MockEngine = @import("engine/test_server.zig").MockEngine;

test "follow off skips the per-refresh log request" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});
    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var response_buf: [160]u8 = undefined;
    const response = try std.fmt.bufPrint(&response_buf, "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nfollow logs", .{});
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener, .response = response };
    try mock.start();
    defer mock.finish() catch {};

    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
    };
    defer state.arena.deinit();

    var completion: refresh.Completion = .{ .gpa = std.testing.allocator };
    var job: refresh.Job = .{
        .state = &state,
        .io = std.testing.io,
        .gpa = std.testing.allocator,
        .socket = try std.testing.allocator.dupe(u8, path),
        .generation = 0,
        .snapshot = Snapshot.init(std.testing.allocator),
        .completion = &completion,
    };
    defer std.testing.allocator.free(job.socket);
    defer job.snapshot.deinit();
    completion.job = &job;
    @memcpy(job.follow_id[0..3], "abc");
    job.follow_len = 3;

    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = job.snapshot.arena.allocator();

    // The selected row supplied an id, but Follow is off: no request.
    refresh.loadLogs(&job, &client);
    try std.testing.expectEqual(@as(usize, 0), mock.count);

    job.follow = true;
    refresh.loadLogs(&job, &client);
    try mock.waitFor(1);
    try std.testing.expectEqualStrings("follow logs", job.snapshot.logs);
    try std.testing.expectEqualStrings("abc", job.snapshot.logs_id);
    try std.testing.expect(!job.snapshot.logs_timestamps);
    try std.testing.expect(std.mem.indexOf(u8, mock.pathAt(0), "/containers/abc/logs") != null);
}
