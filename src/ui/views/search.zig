//! Text search for the list views: one predicate per table, matching the
//! fields the row actually shows. Kept here so `lists.zig` stays inside the
//! 600-line rule and so the matching rules are testable without a window.
const std = @import("std");
const api = @import("../../engine/api.zig");

/// A case-insensitive substring test. The query is typed, so it is short; a
/// sliding compare over the haystack is enough and allocates nothing.
pub fn contains(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > haystack.len) return false;
    var i: usize = 0;
    outer: while (i + needle.len <= haystack.len) : (i += 1) {
        for (needle, 0..) |n, j| {
            if (std.ascii.toLower(haystack[i + j]) != std.ascii.toLower(n)) continue :outer;
        }
        return true;
    }
    return false;
}

/// The fields each table shows, so what matches is what the user can see.
pub fn keepContainer(q: []const u8, ct: *const api.Container) bool {
    if (q.len == 0) return true;
    if (contains(ct.name(), q)) return true;
    if (contains(ct.Image, q)) return true;
    if (contains(ct.State, q)) return true;
    if (ct.health()) |h| {
        if (contains(h, q)) return true;
    }
    if (ct.project()) |p| return contains(p, q);
    return false;
}

pub fn keepImage(q: []const u8, im: api.Image) bool {
    if (q.len == 0) return true;
    if (contains(im.label(), q)) return true;
    return contains(im.Id, q);
}

pub fn keepVolume(q: []const u8, v: api.Volume) bool {
    if (q.len == 0) return true;
    return contains(v.Name, q) or contains(v.Driver, q) or contains(v.Mountpoint, q);
}

pub fn keepNetwork(q: []const u8, n: api.Network) bool {
    if (q.len == 0) return true;
    // The API reports an absent driver as the string "null"; the table shows
    // an em dash for it, so that is what a search should match.
    const driver = if (std.mem.eql(u8, n.Driver, "null")) "—" else n.Driver;
    return contains(n.Name, q) or contains(driver, q) or contains(n.Scope, q);
}

test "search is a case-insensitive substring over visible fields" {
    try std.testing.expect(contains("berth-demo-web", "demo"));
    try std.testing.expect(contains("berth-demo-web", "DEMO"));
    try std.testing.expect(contains("nginx:alpine", ":alp"));
    try std.testing.expect(contains("abc", "abc"));
    try std.testing.expect(!contains("nginx", "alpine"));
    try std.testing.expect(!contains("", "x"));
    try std.testing.expect(contains("anything", ""));
}

test "each table's predicate matches what its rows show" {
    const ct: api.Container = .{ .Names = &.{"/Web"}, .Image = "nginx", .State = "running", .Status = "Up 2 hours (healthy)" };
    try std.testing.expect(keepContainer("web", &ct));
    try std.testing.expect(keepContainer("NGINX", &ct));
    try std.testing.expect(!keepContainer("redis", &ct));
    // Each field is matched whole: a query spanning two fields matches nothing.
    try std.testing.expect(!keepContainer("web nginx", &ct));
    try std.testing.expect(keepContainer("healthy", &ct));

    const im: api.Image = .{ .Id = "sha256:0123456789abcdef", .RepoTags = &.{"alpine:3.22"} };
    try std.testing.expect(keepImage("3.22", im));
    try std.testing.expect(keepImage("0123456789", im)); // a pasted id prefix
    const dangling: api.Image = .{ .Id = "sha256:fff" };
    try std.testing.expect(keepImage("untagged", dangling));

    const vol: api.Volume = .{ .Name = "shop-data", .Driver = "local", .Mountpoint = "/var/lib/docker/volumes/shop-data" };
    try std.testing.expect(keepVolume("shop", vol));
    try std.testing.expect(keepVolume("/volumes", vol));

    const net: api.Network = .{ .Name = "bridge", .Driver = "bridge", .Scope = "local" };
    try std.testing.expect(keepNetwork("loc", net));
    try std.testing.expect(!keepNetwork("overlay", net));

    const none: api.Network = .{ .Name = "none", .Driver = "null", .Scope = "local" };
    try std.testing.expect(keepNetwork("\u{2014}", none)); // the dash the table shows
    try std.testing.expect(!keepNetwork("null", none));
}
