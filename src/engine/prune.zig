//! The engine's prune endpoints: remove what nothing is using and report the
//! bytes that came back. Docker and Podman share these routes; an engine that
//! does not support one returns an error the caller can report.
const std = @import("std");
const api = @import("api.zig");

pub const Kind = enum { containers, images, volumes, networks, build_cache };

pub fn pathFor(kind: Kind) []const u8 {
    return switch (kind) {
        .containers => "/containers/prune",
        // `dangling=false` is Docker Desktop's "unused images": tagged images
        // no container references, not only the intermediate layers.
        .images => "/images/prune?dangling=false",
        // `all=true` is a filter, not a query flag: without it Docker prunes
        // only anonymous volumes, while Docker Desktop's "unused" includes
        // named ones. The JSON is percent-encoded for the query string.
        .volumes => "/volumes/prune?filters=%7B%22all%22%3A%7B%22true%22%3Atrue%7D%7D",
        .networks => "/networks/prune",
        .build_cache => "/build/prune",
    };
}

pub fn run(client: *api.Client, kind: Kind) !u64 {
    const path = pathFor(kind);
    const body = try client.callOpts("POST", path, "", 1 << 20, false);
    defer client.gpa.free(body);
    const parsed = std.json.parseFromSliceLeaky(Response, client.alloc, body, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    }) catch return 0;
    return parsed.SpaceReclaimed;
}

/// Only `SpaceReclaimed` is used; the deleted-name arrays are engine chatter.
const Response = struct {
    SpaceReclaimed: u64 = 0,
};

test "every prune route carries the flags the UI promises" {
    try std.testing.expectEqualStrings("/containers/prune", pathFor(.containers));
    // Unused images means tagged-but-unreferenced, not only dangling layers.
    try std.testing.expectEqualStrings("/images/prune?dangling=false", pathFor(.images));
    // Named unused volumes are only included with Docker's `all` filter.
    try std.testing.expect(std.mem.indexOf(u8, pathFor(.volumes), "all%22%3A%7B%22true") != null);
    try std.testing.expectEqualStrings("/networks/prune", pathFor(.networks));
    try std.testing.expectEqualStrings("/build/prune", pathFor(.build_cache));
}

test "a prune answer is read for its reclaimed bytes" {
    const text = "{\"ImagesDeleted\":[{\"Deleted\":\"sha256:1\"}],\"SpaceReclaimed\":4096}";
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const parsed = try std.json.parseFromSliceLeaky(Response, arena.allocator(), text, .{
        .ignore_unknown_fields = true,
        .allocate = .alloc_always,
    });
    try std.testing.expectEqual(@as(u64, 4096), parsed.SpaceReclaimed);
}
