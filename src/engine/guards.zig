//! Cheap pre-parse limits for engine list bodies. A JSON parser materialises
//! every element, so an answer with millions of `{}` costs far more memory
//! than the bytes on the wire; counting elements first keeps that
//! amplification away from the snapshot.
const std = @import("std");

/// Far beyond what a person can inspect and far below the point where project
/// and network scans become unusable. The same deliberate availability limit
/// applies to each list store: a bigger answer is refused whole rather than
/// parsed into hundreds of megabytes.
pub const max_containers: usize = 5_000;
pub const max_images: usize = 5_000;
pub const max_volumes: usize = 5_000;
pub const max_networks: usize = 5_000;
pub const max_build_cache: usize = 5_000;

/// Deep enough for any real engine document, shallow enough that the
/// recursive parser cannot be turned into a stack amplifier.
pub const max_depth: usize = 64;

pub const Shape = enum { containers, images, volumes, networks, df };

pub const Kind = enum { too_deep, too_many };

pub const Violation = struct {
    kind: Kind,
    cap: usize = 0,
    noun: []const u8 = "",
};

const FieldLimit = struct {
    name: []const u8,
    cap: usize,
    noun: []const u8,
};

const volume_fields = [_]FieldLimit{
    .{ .name = "Volumes", .cap = max_volumes, .noun = "volumes" },
};
const df_fields = [_]FieldLimit{
    .{ .name = "Containers", .cap = max_containers, .noun = "containers" },
    .{ .name = "Images", .cap = max_images, .noun = "images" },
    .{ .name = "Volumes", .cap = max_volumes, .noun = "volumes" },
    .{ .name = "BuildCache", .cap = max_build_cache, .noun = "build-cache entries" },
};

/// The cheap checks that must pass before a list body is handed to the JSON
/// parser. A depth violation is reported first: it applies even to a single
/// object, where an element count would see nothing.
pub fn violation(raw: []const u8, shape: Shape) ?Violation {
    if (nestingExceeds(raw, max_depth)) return .{ .kind = .too_deep };
    return switch (shape) {
        .containers => rootArrayViolation(raw, max_containers, "containers"),
        .images => rootArrayViolation(raw, max_images, "images"),
        .networks => rootArrayViolation(raw, max_networks, "networks"),
        .volumes => rootFieldsViolation(raw, &volume_fields),
        .df => rootFieldsViolation(raw, &df_fields),
    };
}

fn rootArrayViolation(raw: []const u8, cap: usize, noun: []const u8) ?Violation {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    var scanner = std.json.Scanner.initCompleteInput(arena.allocator(), raw);
    defer scanner.deinit();

    const first = scanner.next() catch return null;
    if (first != .array_begin) return null;
    var count: usize = 0;
    while (true) {
        const next = scanner.peekNextTokenType() catch return null;
        if (next == .array_end) {
            _ = scanner.next() catch return null;
            break;
        }
        scanner.skipValue() catch return null;
        count += 1;
        if (count > cap) return .{ .kind = .too_many, .cap = cap, .noun = noun };
    }
    return null;
}

/// Walks the ROOT object only and compares decoded keys. A nested decoy named
/// `Containers` is skipped with its value; an escaped root key such as
/// `"\u0043ontainers"` decodes before the comparison.
fn rootFieldsViolation(raw: []const u8, fields: []const FieldLimit) ?Violation {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    var scanner = std.json.Scanner.initCompleteInput(arena.allocator(), raw);
    defer scanner.deinit();

    const first = scanner.next() catch return null;
    if (first != .object_begin) return null;
    while (true) {
        const next = scanner.peekNextTokenType() catch return null;
        if (next == .object_end) {
            _ = scanner.next() catch return null;
            break;
        }
        const key_token = scanner.nextAllocMax(arena.allocator(), .alloc_always, raw.len) catch return null;
        const key: []const u8 = switch (key_token) {
            .allocated_string => |s| s,
            .string => |s| s,
            else => return null,
        };
        var matched: ?FieldLimit = null;
        for (fields) |limit| {
            if (std.mem.eql(u8, key, limit.name)) {
                matched = limit;
                break;
            }
        }
        if (matched) |limit| {
            const value_kind = scanner.peekNextTokenType() catch return null;
            if (value_kind == .array_begin) {
                _ = scanner.next() catch return null;
                var count: usize = 0;
                while (true) {
                    const item_kind = scanner.peekNextTokenType() catch return null;
                    if (item_kind == .array_end) {
                        _ = scanner.next() catch return null;
                        break;
                    }
                    scanner.skipValue() catch return null;
                    count += 1;
                    if (count > limit.cap) {
                        return .{ .kind = .too_many, .cap = limit.cap, .noun = limit.noun };
                    }
                }
            } else {
                scanner.skipValue() catch return null;
            }
        } else {
            scanner.skipValue() catch return null;
        }
    }
    return null;
}

fn fieldViolation(raw: []const u8, field: []const u8, cap: usize, noun: []const u8) ?Violation {
    const fields = [_]FieldLimit{.{ .name = field, .cap = cap, .noun = noun }};
    return rootFieldsViolation(raw, &fields);
}

/// An iterative pre-flight, because the parser itself is recursive. Braces
/// inside strings are content, not nesting, so a JSON blob full of `[[[` in a
/// label cannot trip the guard.
pub fn nestingExceeds(raw: []const u8, limit: usize) bool {
    var depth: usize = 0;
    var in_string = false;
    var escaped = false;
    for (raw) |ch| {
        if (in_string) {
            if (escaped) {
                escaped = false;
            } else if (ch == '\\') {
                escaped = true;
            } else if (ch == '"') {
                in_string = false;
            }
            continue;
        }
        switch (ch) {
            '"' => in_string = true,
            '{', '[' => {
                depth += 1;
                if (depth > limit) return true;
            },
            '}', ']' => {
                if (depth > 0) depth -= 1;
            },
            else => {},
        }
    }
    return false;
}

fn appendObjects(gpa: std.mem.Allocator, out: *std.ArrayList(u8), count: usize) !void {
    try out.append(gpa, '[');
    for (0..count) |i| {
        if (i > 0) try out.append(gpa, ',');
        try out.appendSlice(gpa, "{}");
    }
    try out.append(gpa, ']');
}

test "root arrays count elements structurally" {
    try std.testing.expect(rootArrayViolation("[{},{}]", 1, "containers") != null);
    try std.testing.expect(rootArrayViolation("[{},{}]", 2, "containers") == null);
    try std.testing.expect(rootArrayViolation("[{\"a\":{\"b\":{}}},{}]", 2, "containers") == null);
    try std.testing.expect(rootArrayViolation("{}", 1, "containers") == null);
    try std.testing.expect(rootArrayViolation("[]", 0, "containers") == null);
}

test "a synthetic body above the container cap is detected before parsing" {
    const gpa = std.testing.allocator;
    var body: std.ArrayList(u8) = .empty;
    defer body.deinit(gpa);
    try appendObjects(gpa, &body, max_containers + 1);

    try std.testing.expect(violation(body.items, .containers) != null);
    try std.testing.expectEqual(Kind.too_many, violation(body.items, .containers).?.kind);
}

test "root fields are decoded and nested decoys do not satisfy the cap" {
    const gpa = std.testing.allocator;
    var decoy_first: std.ArrayList(u8) = .empty;
    defer decoy_first.deinit(gpa);
    try decoy_first.appendSlice(gpa, "{\"X\":{\"Containers\":[]},\"Containers\":");
    try appendObjects(gpa, &decoy_first, max_containers + 1);
    try decoy_first.append(gpa, '}');
    try std.testing.expectEqual(Kind.too_many, violation(decoy_first.items, .df).?.kind);

    var escaped: std.ArrayList(u8) = .empty;
    defer escaped.deinit(gpa);
    try escaped.appendSlice(gpa, "{\"\\u0043ontainers\":");
    try appendObjects(gpa, &escaped, max_containers + 1);
    try escaped.append(gpa, '}');
    try std.testing.expectEqual(Kind.too_many, violation(escaped.items, .df).?.kind);

    // The decoy alone is not a root field, so it is not counted.
    try std.testing.expect(violation("{\"X\":{\"Containers\":[{},{}]}}", .df) == null);
}

test "field arrays are found through whitespace but not inside strings" {
    try std.testing.expect(fieldViolation("{\n \"Volumes\" : [ {}, {} ]\n}", "Volumes", 1, "volumes") != null);
    try std.testing.expect(fieldViolation("{\"note\":\"\\\"Volumes\\\":[{},{}]\"}", "Volumes", 1, "volumes") == null);
    try std.testing.expect(fieldViolation("{\"Volumes\":[]}", "Volumes", 1, "volumes") == null);
}

test "every list shape rejects an answer above its cap" {
    const gpa = std.testing.allocator;
    const shapes = [_]struct { shape: Shape, cap: usize, field: []const u8 }{
        .{ .shape = .containers, .cap = max_containers, .field = "" },
        .{ .shape = .images, .cap = max_images, .field = "" },
        .{ .shape = .networks, .cap = max_networks, .field = "" },
        .{ .shape = .volumes, .cap = max_volumes, .field = "Volumes" },
        .{ .shape = .df, .cap = max_containers, .field = "Containers" },
        .{ .shape = .df, .cap = max_images, .field = "Images" },
        .{ .shape = .df, .cap = max_volumes, .field = "Volumes" },
        .{ .shape = .df, .cap = max_build_cache, .field = "BuildCache" },
    };
    for (shapes) |item| {
        var body: std.ArrayList(u8) = .empty;
        defer body.deinit(gpa);
        if (item.shape == .volumes or item.shape == .df) {
            try body.append(gpa, '{');
            if (item.field.len == 0) unreachable;
            try body.append(gpa, '"');
            try body.appendSlice(gpa, item.field);
            try body.appendSlice(gpa, "\":");
        }
        try appendObjects(gpa, &body, item.cap + 1);
        if (item.shape == .volumes or item.shape == .df) try body.append(gpa, '}');
        try std.testing.expectEqual(Kind.too_many, violation(body.items, item.shape).?.kind);
    }
}

test "a deeply nested single object is refused before parsing" {
    const gpa = std.testing.allocator;
    var deep: std.ArrayList(u8) = .empty;
    defer deep.deinit(gpa);
    try deep.appendSlice(gpa, "{\"Id\":\"a\",\"Labels\":{\"x\":");
    for (0..max_depth + 8) |_| try deep.append(gpa, '[');
    try deep.append(gpa, '0');
    for (0..max_depth + 8) |_| try deep.append(gpa, ']');
    try deep.appendSlice(gpa, "}}");

    const wrappers = [_]struct { shape: Shape, prefix: []const u8, suffix: []const u8 }{
        .{ .shape = .containers, .prefix = "[", .suffix = "]" },
        .{ .shape = .images, .prefix = "[", .suffix = "]" },
        .{ .shape = .networks, .prefix = "[", .suffix = "]" },
        .{ .shape = .volumes, .prefix = "{\"Volumes\":[", .suffix = "]}" },
        .{ .shape = .df, .prefix = "{\"Containers\":[", .suffix = "]}" },
    };
    for (wrappers) |item| {
        var body: std.ArrayList(u8) = .empty;
        defer body.deinit(gpa);
        try body.appendSlice(gpa, item.prefix);
        try body.appendSlice(gpa, deep.items);
        try body.appendSlice(gpa, item.suffix);
        try std.testing.expectEqual(Kind.too_deep, violation(body.items, item.shape).?.kind);
    }
}
