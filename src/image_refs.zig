//! Pure parsing for the pull/run card: reference shapes, engine-safe
//! names, and the port/volume/environment lists people type as commas.
const std = @import("std");

/// `nginx:alpine` into `nginx` + `alpine`; a registry port is not a tag.
pub fn splitReference(reference: []const u8) struct { repo: []const u8, tag: []const u8 } {
    // A digest pins one image and must stay whole: `nginx@sha256:…` split at
    // the digest's colon would ask the engine for a tag it does not have.
    if (std.mem.indexOfScalar(u8, reference, '@') != null) return .{ .repo = reference, .tag = "" };
    const colon = std.mem.lastIndexOfScalar(u8, reference, ':') orelse return .{ .repo = reference, .tag = "" };
    const slash = std.mem.lastIndexOfScalar(u8, reference, '/');
    if (slash != null and slash.? > colon) return .{ .repo = reference, .tag = "" };
    return .{ .repo = reference[0..colon], .tag = reference[colon + 1 ..] };
}

/// An engine reads an untagged reference as "every tag of that repository" —
/// dozens of images nobody asked for. A bare name means latest, as in the CLI;
/// a digest already pins exactly one image and keeps its empty tag.
pub fn effectiveTag(reference: []const u8) []const u8 {
    if (std.mem.indexOfScalar(u8, reference, '@') != null) return "";
    const split = splitReference(reference);
    return if (split.tag.len > 0) split.tag else "latest";
}

/// Reference characters an engine's query string can carry safely. Anything
/// else (a space, `&`, `#`, `%`) would be spliced into the request line raw.
pub fn validReference(reference: []const u8) bool {
    if (reference.len == 0) return false;
    for (reference) |ch| {
        const ok = std.ascii.isAlphanumeric(ch) or ch == '.' or ch == '/' or
            ch == '_' or ch == '-' or ch == ':' or ch == '@';
        if (!ok) return false;
    }
    return true;
}

/// Docker's own container-name rule: one word of letters, digits, dot, dash
/// and underscore, starting with a letter or digit.
pub fn validName(name: []const u8) bool {
    if (name.len == 0 or name.len > 63) return false;
    for (name) |ch| {
        const ok = std.ascii.isAlphanumeric(ch) or ch == '_' or ch == '.' or ch == '-';
        if (!ok) return false;
    }
    return std.ascii.isAlphanumeric(name[0]);
}

/// One `-p` entry: `host:container`, `ip:host:container`, either side
/// optionally carrying a `/udp` style suffix on the container port.
pub const PortBinding = struct {
    host_ip: []const u8 = "",
    host_port: []const u8 = "",
    container: []const u8 = "",
};

/// The container port follows the *last* colon; `127.0.0.1:8080:80` binds to
/// one address, it is not host "127.0.0.1" against container "8080:80".
/// Bracketed IPv6 hosts are not offered and are rejected rather than mangled.
pub fn parsePort(part: []const u8) ?PortBinding {
    const last = std.mem.lastIndexOfScalar(u8, part, ':') orelse return null;
    const container = part[last + 1 ..];
    if (container.len == 0) return null;
    var host = part[0..last];
    var ip: []const u8 = "";
    if (std.mem.indexOfScalar(u8, host, ':')) |first| {
        ip = host[0..first];
        host = host[first + 1 ..];
        if (std.mem.indexOfScalar(u8, host, ':') != null) return null;
        if (!isIpv4(ip)) return null;
    }
    if (host.len == 0) return null;
    return .{ .host_ip = ip, .host_port = host, .container = container };
}

fn isIpv4(s: []const u8) bool {
    if (s.len == 0 or s.len > 15) return false;
    var groups: usize = 0;
    var it = std.mem.splitScalar(u8, s, '.');
    while (it.next()) |g| {
        groups += 1;
        if (groups > 4) return false;
        _ = std.fmt.parseInt(u16, g, 10) catch return false;
    }
    return groups == 4;
}

pub fn firstInvalid(text: []const u8, separator: u8) ?[]const u8 {
    var parts = std.mem.splitScalar(u8, text, ',');
    while (parts.next()) |raw| {
        const part = std.mem.trim(u8, raw, " ");
        if (part.len == 0) continue;
        if (std.mem.indexOfScalar(u8, part, separator) == null) return part;
    }
    return null;
}

/// The first port entry that is neither `host:container` nor
/// `ip:host:container`, for a toast that names the offender.
pub fn firstInvalidPort(text: []const u8) ?[]const u8 {
    var parts = std.mem.splitScalar(u8, text, ',');
    while (parts.next()) |raw| {
        const part = std.mem.trim(u8, raw, " ");
        if (part.len == 0) continue;
        if (parsePort(part) == null) return part;
    }
    return null;
}

/// The first line of an engine message, capped: a toast is not a log viewer.
pub fn firstLine(text: []const u8) []const u8 {
    const end = std.mem.indexOfScalar(u8, text, '\n') orelse @min(text.len, 90);
    return text[0..@min(end, 90)];
}

test "image references split repo and tag, respecting registry ports" {
    const simple = splitReference("nginx:alpine");
    try std.testing.expectEqualStrings("nginx", simple.repo);
    try std.testing.expectEqualStrings("alpine", simple.tag);

    const registry = splitReference("localhost:5000/web");
    try std.testing.expectEqualStrings("localhost:5000/web", registry.repo);
    try std.testing.expectEqualStrings("", registry.tag);

    const digestless = splitReference("alpine");
    try std.testing.expectEqualStrings("alpine", digestless.repo);
    try std.testing.expectEqualStrings("", digestless.tag);

    // A digest must survive whole: its colon is not a tag separator.
    const digest = splitReference("nginx@sha256:5ff9aFAKE");
    try std.testing.expectEqualStrings("nginx@sha256:5ff9aFAKE", digest.repo);
    try std.testing.expectEqualStrings("", digest.tag);
}

test "a bare reference pulls latest, never every tag" {
    try std.testing.expectEqualStrings("latest", effectiveTag("nginx"));
    try std.testing.expectEqualStrings("alpine", effectiveTag("nginx:alpine"));
    try std.testing.expectEqualStrings("", effectiveTag("nginx@sha256:5ff9aFAKE"));
}

test "references and names carry only characters engines accept" {
    try std.testing.expect(validReference("nginx:alpine"));
    try std.testing.expect(validReference("localhost:5000/web:latest"));
    try std.testing.expect(!validReference("my web"));
    try std.testing.expect(!validReference("a&b"));
    try std.testing.expect(!validReference(""));

    try std.testing.expect(validName("web-1"));
    try std.testing.expect(validName("a.b_c"));
    try std.testing.expect(!validName("my web"));
    try std.testing.expect(!validName("1web two"));
    try std.testing.expect(!validName("-nope"));
    var sixty_four: [64]u8 = undefined; // 63 is Docker's own limit
    @memset(&sixty_four, 'x');
    try std.testing.expect(!validName(&sixty_four));
}

test "port entries parse from the last colon" {
    const plain = parsePort("18082:80").?;
    try std.testing.expectEqualStrings("", plain.host_ip);
    try std.testing.expectEqualStrings("18082", plain.host_port);
    try std.testing.expectEqualStrings("80", plain.container);

    const udp = parsePort("18082:80/udp").?;
    try std.testing.expectEqualStrings("80/udp", udp.container);

    const bound = parsePort("127.0.0.1:8080:80").?;
    try std.testing.expectEqualStrings("127.0.0.1", bound.host_ip);
    try std.testing.expectEqualStrings("8080", bound.host_port);
    try std.testing.expectEqualStrings("80", bound.container);

    // IPv6 hosts and colon-less entries are refused, not guessed.
    try std.testing.expect(parsePort("[::1]:8080:80") == null);
    try std.testing.expect(parsePort("8080") == null);
    try std.testing.expect(parsePort("8080:") == null);

    try std.testing.expectEqualStrings("80", firstInvalidPort("18082:80, 80").?);
    try std.testing.expect(firstInvalidPort("18082:80, 127.0.0.1:8081:81") == null);
}

test "option lists report the first malformed entry" {
    try std.testing.expectEqualStrings("80", firstInvalid("8080:80, 80", ':').?);
    try std.testing.expect(firstInvalid("8080:80", ':') == null);
    try std.testing.expectEqualStrings("MODE", firstInvalid("A=1, MODE", '=').?);
}
