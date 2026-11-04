//! Socket-path canonicalization: symlinks can chain (Podman's docker.sock →
//! machine alias → named runtime socket) and parent directories can link too
//! (`/var` ↔ `/private/var`), so one readLink is not enough.
const std = @import("std");

pub fn canonicalSocketPath(io: std.Io, gpa: std.mem.Allocator, path: []const u8) ![]u8 {
    // `unix://` with no path is empty, and a typed path may be relative; the
    // absolute filesystem APIs assert, so normalize both before calling them.
    if (path.len == 0) return gpa.dupe(u8, "");
    const absolute = if (std.fs.path.isAbsolute(path))
        try gpa.dupe(u8, path)
    else blk: {
        const cwd = try std.process.currentPathAlloc(io, gpa);
        defer gpa.free(cwd);
        break :blk try std.fs.path.resolveAlloc(gpa, &.{ cwd, path });
    };
    defer gpa.free(absolute);

    var real_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    if (std.Io.Dir.realPathFileAbsolute(io, absolute, &real_buf)) |n| {
        return gpa.dupe(u8, real_buf[0..n]);
    } else |_| {}

    var current = try gpa.dupe(u8, absolute);
    errdefer gpa.free(current);
    var hops: usize = 0;
    while (hops < 8) : (hops += 1) {
        var link_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
        const n = std.Io.Dir.readLinkAbsolute(io, current, &link_buf) catch break;
        const dir = std.fs.path.dirname(current) orelse break;
        const next = try std.fs.path.resolveAlloc(gpa, &.{ dir, link_buf[0..n] });
        gpa.free(current);
        current = next;
    }
    return current;
}

pub fn sameSocket(io: std.Io, gpa: std.mem.Allocator, a: []const u8, b: []const u8) bool {
    if (std.mem.eql(u8, a, b)) return true;
    const ca = canonicalSocketPath(io, gpa, a) catch return false;
    defer gpa.free(ca);
    const cb = canonicalSocketPath(io, gpa, b) catch return false;
    defer gpa.free(cb);
    return std.mem.eql(u8, ca, cb);
}

test "empty and relative socket paths do not hit absolute asserts" {
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    const empty = try canonicalSocketPath(io, gpa, "");
    defer gpa.free(empty);
    try std.testing.expectEqualStrings("", empty);

    const relative = try canonicalSocketPath(io, gpa, "relative.sock");
    defer gpa.free(relative);
    try std.testing.expect(std.fs.path.isAbsolute(relative));
    try std.testing.expect(std.mem.endsWith(u8, relative, "/relative.sock"));

    const cwd = try std.process.currentPathAlloc(io, gpa);
    defer gpa.free(cwd);
    const expected = try std.fmt.allocPrint(gpa, "{s}/relative.sock", .{cwd});
    defer gpa.free(expected);
    try std.testing.expect(sameSocket(io, gpa, "relative.sock", expected));
}
