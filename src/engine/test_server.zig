//! A tiny Unix-socket engine for tests. It accepts connections until stopped,
//! records each request path, and answers with one canned response. The poll
//! wait is short so a failed test can always join the thread; it is not part of
//! the production build.
const std = @import("std");
const Io = std.Io;

pub const MockEngine = struct {
    io: Io,
    listener: *Io.net.Server,
    response: []const u8 = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}",
    /// One response per request, in order; `response` answers any beyond the
    /// end. Tests that need a failure in the middle of a batch use this.
    responses: []const []const u8 = &.{},
    /// Set the cancellation flag while serving request number N, before the
    /// response goes out, so the client sees it immediately after consuming it.
    cancel_after: usize = 0,
    canceled: ?*std.atomic.Value(bool) = null,
    stop: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    paths: [16][192]u8 = undefined,
    lens: [16]usize = @splat(0),
    count: usize = 0,
    /// The request log is not atomic; this is what a test polls while the
    /// worker is still running.
    served: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    failure: ?anyerror = null,
    thread: ?std.Thread = null,

    pub fn start(self: *MockEngine) !void {
        self.thread = try std.Thread.spawn(.{}, serve, .{self});
    }

    /// Waits for `expected` requests without sleeping. The bound is a spin
    /// count, so a broken worker fails the test instead of hanging it.
    pub fn waitFor(self: *MockEngine, expected: usize) !void {
        var spins: usize = 0;
        while (self.served.load(.acquire) < expected) {
            if (spins >= 10_000_000) return error.MockTimeout;
            spins += 1;
            std.Thread.yield() catch {};
        }
    }

    pub fn finish(self: *MockEngine) !void {
        self.stop.store(true, .release);
        if (self.thread) |thread| {
            thread.join();
            self.thread = null;
        }
        if (self.failure) |e| {
            self.failure = null;
            return e;
        }
    }

    pub fn pathAt(self: *const MockEngine, i: usize) []const u8 {
        return self.paths[i][0..self.lens[i]];
    }

    /// The container id in a `/containers/{id}/...` request path, or "".
    pub fn containerId(self: *const MockEngine, i: usize) []const u8 {
        const path = self.pathAt(i);
        const marker = "/containers/";
        const marker_start = std.mem.indexOf(u8, path, marker) orelse return "";
        const rest = path[marker_start + marker.len ..];
        const end = std.mem.indexOfScalar(u8, rest, '/') orelse rest.len;
        return rest[0..end];
    }

    fn serve(self: *MockEngine) void {
        const fd = self.listener.socket.handle;
        while (!self.stop.load(.acquire)) {
            var fds = [_]std.posix.pollfd{.{
                .fd = fd,
                .events = std.posix.POLL.IN,
                .revents = 0,
            }};
            const ready = std.posix.poll(&fds, 20) catch |e| {
                self.failure = e;
                return;
            };
            if (ready == 0) continue;
            const stream = self.listener.accept(self.io) catch |e| {
                if (self.stop.load(.acquire)) return;
                self.failure = e;
                return;
            };
            self.handle(stream) catch |e| {
                self.failure = e;
                return;
            };
        }
    }

    fn handle(self: *MockEngine, stream: Io.net.Stream) !void {
        defer stream.close(self.io);
        var buf: [2048]u8 = undefined;
        var len: usize = 0;
        while (std.mem.indexOf(u8, buf[0..len], "\r\n\r\n") == null) {
            if (len == buf.len) break;
            const n = std.posix.read(stream.socket.handle, buf[len..]) catch |e| return e;
            if (n == 0) return;
            len += n;
        }
        const line_end = std.mem.indexOf(u8, buf[0..len], "\r\n") orelse len;
        var parts = std.mem.splitScalar(u8, buf[0..line_end], ' ');
        _ = parts.next();
        const path = parts.next() orelse "";
        const index = self.count;
        if (index < self.paths.len) {
            const n = @min(path.len, self.paths[0].len);
            @memcpy(self.paths[index][0..n], path[0..n]);
            self.lens[index] = n;
            self.count += 1;
        }
        self.served.store(self.count, .release);
        if (self.cancel_after != 0 and self.count == self.cancel_after) {
            if (self.canceled) |flag| flag.store(true, .release);
        }
        const body = if (index < self.responses.len) self.responses[index] else self.response;
        const file = Io.File{ .handle = stream.socket.handle, .flags = .{ .nonblocking = false } };
        try file.writeStreamingAll(self.io, body);
    }
};
