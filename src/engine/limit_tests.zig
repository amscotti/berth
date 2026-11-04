//! Endpoint body limits sit above the structural entry caps: a list with the
//! maximum normal entries must fit on the wire. These tests drive the real
//! socket reader with one near-cap and one over-cap answer.
const std = @import("std");
const api = @import("api.zig");
const guards = @import("guards.zig");
const MockEngine = @import("test_server.zig").MockEngine;

fn response(gpa: std.mem.Allocator, body: []const u8) ![]u8 {
    return std.fmt.allocPrint(gpa, "HTTP/1.1 200 OK\r\nContent-Length: {d}\r\n\r\n{s}", .{ body.len, body });
}

fn jsonPadding(gpa: std.mem.Allocator, len: usize) ![]u8 {
    const body = try gpa.alloc(u8, len);
    @memset(body, ' ');
    body[0] = '[';
    body[1] = ']';
    return body;
}

fn dfPadding(gpa: std.mem.Allocator, len: usize) ![]u8 {
    const prefix = "{\"Containers\":[]}";
    const body = try gpa.alloc(u8, len);
    @memset(body, ' ');
    @memcpy(body[0..prefix.len], prefix);
    return body;
}

test "list body limits leave room for five thousand normal entries" {
    try std.testing.expectEqual(@as(usize, 32 << 20), guards.bodyLimit(.containers));
    try std.testing.expectEqual(@as(usize, 32 << 20), guards.bodyLimit(.df));
    try std.testing.expectEqual(@as(usize, 16 << 20), guards.bodyLimit(.images));
    try std.testing.expectEqual(@as(usize, 16 << 20), guards.bodyLimit(.volumes));
    try std.testing.expectEqual(@as(usize, 16 << 20), guards.bodyLimit(.networks));
    // ~1.7 KiB per container at the structural cap is what used to exceed 8 MiB.
    try std.testing.expect(guards.bodyLimit(.containers) > guards.max_containers * 1700);
}

test "a list body just below the endpoint limit is accepted" {
    const gpa = std.testing.allocator;
    const limit = guards.bodyLimit(.images);
    const body = try jsonPadding(gpa, limit - 64 * 1024);
    defer gpa.free(body);
    const reply = try response(gpa, body);
    defer gpa.free(reply);

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});
    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener, .response = reply };
    try mock.start();
    defer mock.finish() catch {};
    const images = try client.images();
    try std.testing.expectEqual(@as(usize, 0), images.len);
    try mock.finish();
}

test "a df body just below the endpoint limit is accepted" {
    const gpa = std.testing.allocator;
    const limit = guards.bodyLimit(.df);
    const body = try dfPadding(gpa, limit - 64 * 1024);
    defer gpa.free(body);
    const reply = try response(gpa, body);
    defer gpa.free(reply);

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});
    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener, .response = reply };
    try mock.start();
    defer mock.finish() catch {};
    const df = try client.df();
    try std.testing.expectEqual(@as(usize, 0), df.Containers.?.len);
    try mock.finish();
}

test "a list body over the endpoint limit is refused" {
    const gpa = std.testing.allocator;
    const limit = guards.bodyLimit(.images);
    const body = try jsonPadding(gpa, limit + 1);
    defer gpa.free(body);
    const reply = try response(gpa, body);
    defer gpa.free(reply);

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});
    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var client = api.Client.init(std.testing.io, std.testing.allocator, path);
    client.alloc = arena.allocator();

    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener, .response = reply };
    try mock.start();
    defer mock.finish() catch {};
    try std.testing.expectError(error.TooLarge, client.images());
    try mock.finish();
}
