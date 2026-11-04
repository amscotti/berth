//! Image creation endpoints: pull a reference, and create then start a
//! container from the JSON the Run dialog builds.
const std = @import("std");
const api = @import("api.zig");

pub const Created = struct {
    Id: []const u8 = "",
};

/// The pull query, without an empty `tag=` when a digest already pins the
/// image — engines vary in how they read that combination.
pub fn pullPath(buf: []u8, from: []const u8, tag: []const u8) ![]const u8 {
    if (tag.len > 0) return std.fmt.bufPrint(buf, "/images/create?fromImage={s}&tag={s}", .{ from, tag });
    return std.fmt.bufPrint(buf, "/images/create?fromImage={s}", .{from});
}

/// `POST /images/create` answers with a stream of progress objects. This call
/// waits for the whole stream; the job around it is what keeps the UI alive.
pub fn pull(client: *api.Client, from: []const u8, tag: []const u8) ![]const u8 {
    var buf: [320]u8 = undefined;
    const p = try pullPath(&buf, from, tag);
    return client.callOptsTimeout("POST", p, "", 4 << 20, false, 900, false);
}

pub fn create(client: *api.Client, name: []const u8, body: []const u8) !Created {
    // The name is validated to engine-safe characters before it reaches here.
    // With none given, the parameter is omitted entirely: `?name=` empty is
    // an "invalid container name" rejection, not an anonymous container.
    const p = if (name.len > 0)
        try std.fmt.allocPrint(client.gpa, "/containers/create?name={s}", .{name})
    else
        try client.gpa.dupe(u8, "/containers/create");
    defer client.gpa.free(p);
    const response = try client.callOptsTimeout("POST", p, body, 1 << 20, false, 30, false);
    defer client.gpa.free(response);
    // The parsed Id must outlive `response`, which is freed above: without
    // alloc_always it would point into the freed buffer.
    return std.json.parseFromSliceLeaky(Created, client.alloc, response, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    });
}

pub fn start(client: *api.Client, id: []const u8) !void {
    const p = try std.fmt.allocPrint(client.gpa, "/containers/{s}/start", .{id});
    defer client.gpa.free(p);
    // This is a lifecycle call: an already-running container answers 304.
    const body = try client.callOptsTimeout("POST", p, "", 1 << 20, false, 30, true);
    client.gpa.free(body);
}

test "pull paths address the image endpoints" {
    var buf: [160]u8 = undefined;
    const tagged = try pullPath(&buf, "nginx", "alpine");
    try std.testing.expectEqualStrings("/images/create?fromImage=nginx&tag=alpine", tagged);

    const digest = try pullPath(&buf, "nginx@sha256:5ff9aFAKE", "");
    try std.testing.expectEqualStrings("/images/create?fromImage=nginx@sha256:5ff9aFAKE", digest);
    try std.testing.expect(std.mem.indexOf(u8, digest, "tag=") == null);
}
