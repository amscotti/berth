//! HTTP/1.1 over a Unix domain socket: the protocol every container runtime
//! speaks. Docker, Podman, Colima and Lima all expose the same engine API on a
//! socket file rather than a port, which is why the framework's own `http`
//! (URL strings, `http`/`https` only, one whole response) cannot be used here.
//!
//! The stdlib opens the socket (`std.Io.net.UnixAddress`); everything in this
//! file is the framing on top of it — request bytes out, status line and
//! headers in, `Content-Length` or chunked bodies decoded.
//!
//! Requests ask for `Connection: close` and read to end of file, so a response
//! is complete when the peer hangs up. Streaming endpoints (`/events`,
//! `/stats?stream=true`, `/logs&follow=1`) are not read this way; they need
//! `readable` handling, which the caller drives.
const std = @import("std");
const Io = std.Io;

pub const Error = error{
    BadStatus,
    BadHeaders,
    BadChunk,
    TooLarge,
    ShortBody,
    UnsupportedTransfer,
};

/// A response whose body the caller owns.
pub const Response = struct {
    status: u16,
    body: []u8,

    pub fn deinit(self: Response, gpa: std.mem.Allocator) void {
        gpa.free(self.body);
    }

    pub fn ok(self: Response) bool {
        return self.status >= 200 and self.status < 300;
    }
};

/// One connection to an engine socket. Not thread safe; one per request, or
/// one per streaming endpoint held by the reader.
pub const Conn = struct {
    io: Io,
    stream: Io.net.Stream,

    pub fn open(io: Io, path: []const u8) !Conn {
        const addr = try Io.net.UnixAddress.init(path);
        return .{ .io = io, .stream = try addr.connect(io) };
    }

    pub fn close(self: *Conn) void {
        self.stream.socket.close(self.io);
    }

    /// The descriptor, for a streaming caller that watches it with fdwatch.
    pub fn handle(self: *const Conn) std.posix.fd_t {
        return self.stream.socket.handle;
    }

    /// Writes the request and reads the whole answer. `max` bounds the body, so
    /// a runaway `/system/df` cannot exhaust memory.
    pub fn request(
        self: *Conn,
        gpa: std.mem.Allocator,
        method: []const u8,
        path: []const u8,
        body: []const u8,
        max: usize,
    ) !Response {
        try self.write(method, path, body, &.{});
        const raw = try self.readAll(gpa, max);
        defer gpa.free(raw);
        return decode(gpa, raw, max);
    }

    pub fn write(self: *Conn, method: []const u8, path: []const u8, body: []const u8, headers: []const [2][]const u8) !void {
        var buf: [4096]u8 = undefined;
        var w: Io.Writer = .fixed(&buf);
        const ww = &w;
        try ww.print("{s} {s} HTTP/1.1\r\nHost: docker\r\nUser-Agent: berth\r\nAccept: application/json\r\nConnection: close\r\n", .{ method, path });
        for (headers) |h| try ww.print("{s}: {s}\r\n", .{ h[0], h[1] });
        if (body.len > 0) try ww.print("Content-Length: {d}\r\n", .{body.len});
        try ww.writeAll("\r\n");
        if (body.len > 0) try ww.writeAll(body);
        const file = Io.File{ .handle = self.stream.socket.handle, .flags = .{ .nonblocking = false } };
        try file.writeStreamingAll(self.io, ww.buffered());
    }

    /// Reads until the peer closes. With `Connection: close` that is exactly the
    /// response; growth doubles and is bounded by `max`.
    fn readAll(self: *Conn, gpa: std.mem.Allocator, max: usize) ![]u8 {
        var cap: usize = 16 * 1024;
        if (cap > max) cap = max;
        var buf = try gpa.alloc(u8, cap);
        errdefer gpa.free(buf);
        var len: usize = 0;
        while (true) {
            if (len == buf.len) {
                if (len >= max) return error.TooLarge;
                const next = @min(buf.len * 2, max);
                buf = try gpa.realloc(buf, next);
            }
            const n = try std.posix.read(self.stream.socket.handle, buf[len..]);
            if (n == 0) break;
            len += n;
        }
        return try gpa.realloc(buf, len);
    }
};

pub const Head = struct {
    status: u16,
    chunked: bool = false,
    content_length: ?usize = null,
};

/// Splits a response into status, headers and body, and decodes the body.
pub fn decode(gpa: std.mem.Allocator, raw: []const u8, max: usize) !Response {
    const split = std.mem.indexOf(u8, raw, "\r\n\r\n") orelse {
        // Some engines answer with a bare status line and no body at all.
        if (std.mem.startsWith(u8, raw, "HTTP/1.")) {
            const head = try parseHead(raw);
            return .{ .status = head.status, .body = try gpa.dupe(u8, "") };
        }
        return error.BadHeaders;
    };
    const head = try parseHead(raw[0..split]);
    const rest = raw[split + 4 ..];

    if (head.chunked) return .{ .status = head.status, .body = try dechunk(gpa, rest, max) };
    if (head.content_length) |n| {
        if (rest.len < n) return error.ShortBody;
        return .{ .status = head.status, .body = try gpa.dupe(u8, rest[0..n]) };
    }
    return .{ .status = head.status, .body = try gpa.dupe(u8, rest) };
}

/// `HTTP/1.1 200 OK` and the header lines after it.
pub fn parseHead(text: []const u8) !Head {
    var lines = std.mem.splitSequence(u8, text, "\r\n");
    const first = lines.next() orelse return error.BadStatus;
    if (!std.mem.startsWith(u8, first, "HTTP/1.")) return error.BadStatus;
    const sp = std.mem.indexOfScalar(u8, first, ' ') orelse return error.BadStatus;
    const rest = first[sp + 1 ..];
    const cut = std.mem.indexOfScalar(u8, rest, ' ') orelse rest.len;
    const code = std.fmt.parseInt(u16, rest[0..cut], 10) catch return error.BadStatus;

    var head: Head = .{ .status = code };
    while (lines.next()) |line| {
        if (line.len == 0) break;
        const colon = std.mem.indexOfScalar(u8, line, ':') orelse continue;
        const name = std.mem.trim(u8, line[0..colon], " ");
        const value = std.mem.trim(u8, line[colon + 1 ..], " ");
        if (std.ascii.eqlIgnoreCase(name, "Transfer-Encoding")) {
            // Docker and Podman send exactly `chunked`; anything else is a
            // transfer coding we do not implement, so say so rather than guess.
            if (std.ascii.eqlIgnoreCase(value, "chunked")) {
                head.chunked = true;
            } else if (!std.ascii.eqlIgnoreCase(value, "identity")) {
                return error.UnsupportedTransfer;
            }
        } else if (std.ascii.eqlIgnoreCase(name, "Content-Length")) {
            head.content_length = std.fmt.parseInt(usize, value, 10) catch null;
        }
    }
    return head;
}

/// `sizeCRLF bytes CRLF` repeated, ended by a zero size.
pub fn dechunk(gpa: std.mem.Allocator, body: []const u8, max: usize) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    var i: usize = 0;
    while (true) {
        const eol = std.mem.indexOfPos(u8, body, i, "\r\n") orelse return error.BadChunk;
        var size_text = body[i..eol];
        // A chunk size may carry extensions after a ';'.
        if (std.mem.indexOfScalar(u8, size_text, ';')) |semi| size_text = size_text[0..semi];
        const size = std.fmt.parseInt(usize, std.mem.trim(u8, size_text, " "), 16) catch return error.BadChunk;
        i = eol + 2;
        if (size == 0) break;
        if (i + size > body.len) return error.BadChunk;
        if (out.items.len + size > max) return error.TooLarge;
        try out.appendSlice(gpa, body[i .. i + size]);
        i += size;
        if (i + 2 > body.len or !std.mem.eql(u8, body[i .. i + 2], "\r\n")) return error.BadChunk;
        i += 2;
    }
    return out.toOwnedSlice(gpa);
}

test "status line and headers" {
    const head = try parseHead("HTTP/1.1 304 Not Modified\r\nContent-Length: 0");
    try std.testing.expectEqual(@as(u16, 304), head.status);
    try std.testing.expectEqual(@as(?usize, 0), head.content_length);
    try std.testing.expect(!head.chunked);
}

test "chunked transfer encoding" {
    const raw = "4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
    const body = try dechunk(std.testing.allocator, raw, 1 << 20);
    defer std.testing.allocator.free(body);
    try std.testing.expectEqualStrings("Wikipedia", body);
}

test "chunked with extensions and a split frame" {
    const raw = "5;ext=1\r\nhello\r\nA\r\n0123456789\r\n0\r\n\r\n";
    const body = try dechunk(std.testing.allocator, raw, 1 << 20);
    defer std.testing.allocator.free(body);
    try std.testing.expectEqualStrings("hello0123456789", body);
}

test "a whole response, chunked" {
    const raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n{\"a\":1}\r\n0\r\n\r\n";
    const res = try decode(std.testing.allocator, raw, 1 << 20);
    defer res.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(u16, 200), res.status);
    try std.testing.expectEqualStrings("{\"a\":1}", res.body);
}

test "a whole response, content length" {
    const raw = "HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\n\r\nnope";
    const res = try decode(std.testing.allocator, raw, 1 << 20);
    defer res.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(u16, 404), res.status);
    try std.testing.expectEqualStrings("nope", res.body);
}

test "a short body is an error, not a silent truncation" {
    const raw = "HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nnope";
    try std.testing.expectError(error.ShortBody, decode(std.testing.allocator, raw, 1 << 20));
}

test "a body over the limit is refused" {
    const raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\n0123456789abcdef\r\n0\r\n\r\n";
    try std.testing.expectError(error.TooLarge, decode(std.testing.allocator, raw, 8));
}
