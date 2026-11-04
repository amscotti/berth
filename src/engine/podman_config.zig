//! Podman machine config lookup. The socket path changed layout in Podman 4,
//! and the machine name is not always encoded in it, so the config directory
//! is the only reliable source for a non-default machine.
const std = @import("std");
const lifetime = @import("lifetime.zig");

/// The machine name for a Podman 4 socket, or null when the layout is the
/// default machine or the config cannot tell us.
pub fn machineName(io: std.Io, home: []const u8, xdg_config: []const u8, socket: []const u8, out: []u8) ?[]const u8 {
    const marker = "/podman/machine/";
    const at = std.mem.indexOf(u8, socket, marker) orelse return null;
    const rest = socket[at + marker.len ..];
    const slash = std.mem.indexOfScalar(u8, rest, '/') orelse return null;
    const segment = rest[0..slash];
    if (segment.len == 0 or !std.mem.eql(u8, rest[slash + 1 ..], "podman.sock")) return null;

    var root_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const root = if (xdg_config.len > 0)
        std.fmt.bufPrint(&root_buf, "{s}/containers/podman/machine", .{xdg_config}) catch return null
    else if (home.len > 0)
        std.fmt.bufPrint(&root_buf, "{s}/.config/containers/podman/machine", .{home}) catch return null
    else
        return null;

    // Podman 4.5+ puts the socket under the VM-type directory and every machine
    // config beside it. Pick the most recently used config, not the default.
    var dir_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const vm_dir = std.fmt.bufPrint(&dir_buf, "{s}/{s}", .{ root, segment }) catch return null;
    if (std.Io.Dir.openDirAbsolute(io, vm_dir, .{ .iterate = true })) |dir| {
        defer dir.close(io);
        return activeConfigName(io, dir, out);
    } else |_| {}

    // Podman 4.0–4.4 put the machine name in the socket path and the config
    // under a VM-type directory. A matching <name>.json means this is a named
    // machine, not the default.
    if (std.Io.Dir.openDirAbsolute(io, root, .{ .iterate = true })) |dir| {
        defer dir.close(io);
        var it = dir.iterate();
        while (it.next(io) catch null) |entry| {
            if (entry.kind != .directory) continue;
            var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
            const path = std.fmt.bufPrint(&path_buf, "{s}/{s}/{s}.json", .{ root, entry.name, segment }) catch continue;
            std.Io.Dir.accessAbsolute(io, path, .{}) catch continue;
            if (segment.len > out.len) return null;
            @memcpy(out[0..segment.len], segment);
            return out[0..segment.len];
        }
    } else |_| {}
    return null;
}

/// The newest config in a VM-type directory. `LastUp` is Podman's record of
/// the last start; when it is missing or unreadable, the first config still
/// beats silently starting the default machine.
fn activeConfigName(io: std.Io, dir: std.Io.Dir, out: []u8) ?[]const u8 {
    const Config = struct {
        Name: []const u8 = "",
        LastUp: ?[]const u8 = null,
    };
    const gpa = std.heap.page_allocator;
    var best_len: ?usize = null;
    var best_up: i64 = -1;
    var it = dir.iterate();
    while (it.next(io) catch null) |entry| {
        if (!std.mem.endsWith(u8, entry.name, ".json")) continue;
        if (entry.kind != .file and entry.kind != .sym_link) continue;
        const stem = entry.name[0 .. entry.name.len - ".json".len];
        if (stem.len == 0) continue;

        var name_buf: [256]u8 = undefined;
        var name: []const u8 = stem;
        var up: i64 = 0;
        if (dir.readFileAlloc(io, entry.name, gpa, .limited(64 * 1024))) |text| {
            defer gpa.free(text);
            if (std.json.parseFromSlice(Config, gpa, text, .{ .ignore_unknown_fields = true })) |parsed| {
                defer parsed.deinit();
                if (parsed.value.Name.len > 0 and parsed.value.Name.len <= name_buf.len) {
                    @memcpy(name_buf[0..parsed.value.Name.len], parsed.value.Name);
                    name = name_buf[0..parsed.value.Name.len];
                }
                if (parsed.value.LastUp) |stamp| up = lifetime.rfc3339(stamp) orelse 0;
            } else |_| {}
        } else |_| {}

        if (best_len == null or up > best_up) {
            if (name.len > out.len) continue;
            @memcpy(out[0..name.len], name);
            best_len = name.len;
            best_up = up;
        }
    }
    return if (best_len) |n| out[0..n] else null;
}
