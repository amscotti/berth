//! The pull and run forms' local rules, kept pure so the card can say what is
//! wrong beside the field before a request is sent. Engine failures still go
//! through the rail toast, because those describe a finished action.
const std = @import("std");
const refs = @import("image_refs.zig");

pub const Field = enum { reference, image, name, ports, volumes, env };

pub const Kind = enum {
    empty_reference,
    bad_reference,
    long_reference,
    empty_image,
    bad_image,
    long_image,
    bad_name,
    bad_port,
    bad_volume,
    bad_env,
    long_options,
};

pub const Failure = struct {
    kind: Kind,
    field: Field,
    /// The offending entry, when the sentence names one.
    detail: []const u8 = "",
};

pub fn pull(reference: []const u8) ?Failure {
    if (reference.len == 0) return .{ .kind = .empty_reference, .field = .reference };
    if (!refs.validReference(reference)) {
        return .{ .kind = .bad_reference, .field = .reference, .detail = refs.firstLine(reference) };
    }
    const split = refs.splitReference(reference);
    if (split.repo.len > 256 or split.tag.len > 128) {
        return .{ .kind = .long_reference, .field = .reference };
    }
    return null;
}

pub fn run(image: []const u8, name: []const u8, ports: []const u8, volumes: []const u8, env: []const u8) ?Failure {
    if (image.len == 0) return .{ .kind = .empty_image, .field = .image };
    if (!refs.validReference(image)) {
        return .{ .kind = .bad_image, .field = .image, .detail = refs.firstLine(image) };
    }
    if (image.len > 256) return .{ .kind = .long_image, .field = .image };
    if (name.len > 0 and !refs.validName(name)) return .{ .kind = .bad_name, .field = .name };
    if (refs.firstInvalidPort(ports)) |bad| {
        return .{ .kind = .bad_port, .field = .ports, .detail = bad };
    }
    if (refs.firstInvalid(volumes, ':')) |bad| {
        return .{ .kind = .bad_volume, .field = .volumes, .detail = bad };
    }
    if (refs.firstInvalid(env, '=')) |bad| {
        return .{ .kind = .bad_env, .field = .env, .detail = bad };
    }
    // Point at the field that is actually too long, not whichever row the
    // combined check happened to mention last.
    if (ports.len > 512) return .{ .kind = .long_options, .field = .ports };
    if (volumes.len > 512) return .{ .kind = .long_options, .field = .volumes };
    if (env.len > 512) return .{ .kind = .long_options, .field = .env };
    return null;
}

/// One sentence for the card, formatted into the caller's buffer so tests can
/// read exactly what the user sees without a window.
pub fn describe(failure: Failure, buf: []u8) []const u8 {
    return switch (failure.kind) {
        .empty_reference => "Enter an image reference.",
        .bad_reference => std.fmt.bufPrint(buf, "Reference {s} has characters an engine will reject.", .{failure.detail}) catch "That reference has characters an engine will reject.",
        .long_reference => "That reference is too long.",
        .empty_image => "Enter an image to run.",
        .bad_image => std.fmt.bufPrint(buf, "Image {s} has characters an engine will reject.", .{failure.detail}) catch "That image has characters an engine will reject.",
        .long_image => "That image reference is too long.",
        .bad_name => "Container names are letters, digits, dot, dash and underscore.",
        .bad_port => std.fmt.bufPrint(buf, "Port {s} needs host:container or ip:host:container.", .{failure.detail}) catch "A port needs host:container.",
        .bad_volume => std.fmt.bufPrint(buf, "Volume {s} needs source:container.", .{failure.detail}) catch "A volume needs source:container.",
        .bad_env => std.fmt.bufPrint(buf, "Environment {s} needs KEY=value.", .{failure.detail}) catch "An environment entry needs KEY=value.",
        .long_options => "That options list is too long.",
    };
}

test "pull reports the field and sentence for each bad reference" {
    var buf: [160]u8 = undefined;
    const empty = pull("").?;
    try std.testing.expectEqual(Field.reference, empty.field);
    try std.testing.expectEqualStrings("Enter an image reference.", describe(empty, &buf));

    const bad = pull("my image").?;
    try std.testing.expectEqual(Kind.bad_reference, bad.kind);
    try std.testing.expectEqualStrings("Reference my image has characters an engine will reject.", describe(bad, &buf));

    try std.testing.expect(pull("nginx:alpine") == null);
    try std.testing.expect(pull("localhost:5000/web") == null);
}

test "run names the first field that needs attention" {
    var buf: [192]u8 = undefined;
    try std.testing.expectEqual(Field.image, run("", "", "", "", "").?.field);
    try std.testing.expectEqual(Field.image, run("my image", "", "", "", "").?.field);
    try std.testing.expectEqual(Field.name, run("nginx", "two words", "", "", "").?.field);
    try std.testing.expectEqual(Field.ports, run("nginx", "", "8080", "", "").?.field);
    try std.testing.expectEqual(Field.volumes, run("nginx", "", "", "data", "").?.field);
    try std.testing.expectEqual(Field.env, run("nginx", "", "", "", "MODE").?.field);

    const port = run("nginx", "", "8080", "", "").?;
    try std.testing.expectEqualStrings("Port 8080 needs host:container or ip:host:container.", describe(port, &buf));
    const volume = run("nginx", "", "", "data", "").?;
    try std.testing.expectEqualStrings("Volume data needs source:container.", describe(volume, &buf));
    const env = run("nginx", "", "", "", "MODE").?;
    try std.testing.expectEqualStrings("Environment MODE needs KEY=value.", describe(env, &buf));

    try std.testing.expect(run("nginx:alpine", "web", "18080:80", "data:/data", "A=1") == null);
}

test "a long options list points at the field that overflowed" {
    var ports: [520]u8 = undefined;
    @memset(&ports, '1');
    @memcpy(ports[516..519], ":80");
    try std.testing.expectEqual(Field.ports, run("nginx", "", ports[0..], "", "").?.field);

    var volumes: [520]u8 = undefined;
    @memset(&volumes, 'v');
    @memcpy(volumes[518..520], ":d");
    try std.testing.expectEqual(Field.volumes, run("nginx", "", "", volumes[0..], "").?.field);

    var env: [520]u8 = undefined;
    @memset(&env, 'E');
    @memcpy(env[518..520], "=1");
    try std.testing.expectEqual(Field.env, run("nginx", "", "", "", env[0..]).?.field);
}
