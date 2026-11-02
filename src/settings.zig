//! Saved preferences. A small JSON file in the app's data directory, because
//! the framework's key-value store is NSUserDefaults on Apple only and keeps
//! nothing across launches on Linux.
const std = @import("std");
const nanoui = @import("nanoui");

/// What the user asked for; `.system` follows the desktop.
pub const Appearance = enum {
    system,
    light,
    dark,

    pub fn label(self: Appearance) []const u8 {
        return switch (self) {
            .system => "System",
            .light => "Light",
            .dark => "Dark",
        };
    }
};

pub const Data = struct {
    appearance: Appearance = .system,
};

fn filePath(gpa: std.mem.Allocator, io: std.Io) ![]u8 {
    const dir = try nanoui.path(gpa, .data, "berth");
    defer gpa.free(dir);
    std.Io.Dir.cwd().createDirPath(io, dir) catch {};
    return std.fmt.allocPrint(gpa, "{s}/settings.json", .{dir});
}

pub fn load(gpa: std.mem.Allocator, io: std.Io) Data {
    const path = filePath(gpa, io) catch return .{};
    defer gpa.free(path);

    const file = std.Io.Dir.cwd().openFile(io, path, .{}) catch return .{};
    defer file.close(io);

    var buf: [4096]u8 = undefined;
    var reader = file.reader(io, &buf);
    const bytes = reader.interface.allocRemaining(gpa, .limited(64 * 1024)) catch return .{};
    defer gpa.free(bytes);

    const parsed = std.json.parseFromSlice(Data, gpa, bytes, .{
        .ignore_unknown_fields = true,
    }) catch return .{};
    defer parsed.deinit();
    return parsed.value;
}

pub fn save(gpa: std.mem.Allocator, io: std.Io, data: Data) !void {
    const path = filePath(gpa, io) catch return error.NoPath;
    defer gpa.free(path);

    var body: std.Io.Writer.Allocating = .init(gpa);
    defer body.deinit();
    try std.json.Stringify.value(data, .{}, &body.writer);

    const file = try std.Io.Dir.cwd().createFile(io, path, .{});
    defer file.close(io);
    try file.writeStreamingAll(io, body.written());
}

test "appearance labels are stable" {
    try std.testing.expectEqualStrings("System", Appearance.system.label());
    try std.testing.expectEqualStrings("Light", Appearance.light.label());
    try std.testing.expectEqualStrings("Dark", Appearance.dark.label());
}

test "saved settings parse and default to system" {
    const dark = try std.json.parseFromSlice(Data, std.testing.allocator, "{\"appearance\":\"dark\"}", .{ .ignore_unknown_fields = true });
    defer dark.deinit();
    try std.testing.expectEqual(Appearance.dark, dark.value.appearance);

    const empty = try std.json.parseFromSlice(Data, std.testing.allocator, "{}", .{ .ignore_unknown_fields = true });
    defer empty.deinit();
    try std.testing.expectEqual(Appearance.system, empty.value.appearance);
}
