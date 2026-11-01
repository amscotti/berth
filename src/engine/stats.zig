//! Container statistics and logs: the two payloads with shapes of their own.
//! Split out of `api.zig` to keep every file inside the 600-line rule.
const std = @import("std");

/// One `/containers/{id}/stats` sample. `precpu_stats` is what makes CPU%
/// meaningful: with `one-shot=true` it arrives empty and the formula
/// degenerates into "average since the container started", which never matches
/// `docker stats`.
pub const Stats = struct {
    read: []const u8 = "",
    cpu_stats: Cpu = .{},
    precpu_stats: Cpu = .{},
    memory_stats: Mem = .{},
    pids_stats: ?struct { current: u32 = 0 } = null,
    networks: ?std.json.Value = null,

    pub const Cpu = struct {
        cpu_usage: Usage = .{},
        system_cpu_usage: u64 = 0,
        online_cpus: u32 = 0,

        pub const Usage = struct { total_usage: u64 = 0, percpu_usage: ?[]u64 = null };
    };

    pub const Mem = struct {
        usage: u64 = 0,
        limit: u64 = 0,
        stats: ?std.json.Value = null,
    };

    /// Percent of one core, the same figure `docker stats` prints: the delta in
    /// container CPU over the delta in system CPU, times the core count.
    pub fn cpuPercent(self: Stats) f64 {
        const total = self.cpu_stats.cpu_usage.total_usage;
        const pre = self.precpu_stats.cpu_usage.total_usage;
        const system = self.cpu_stats.system_cpu_usage;
        const pre_system = self.precpu_stats.system_cpu_usage;
        // No previous sample means no delta. Without this guard a `one-shot`
        // response silently reports the lifetime average, which looks like a
        // plausible number and is not the one `docker stats` prints.
        if (pre == 0 or pre_system == 0) return 0;
        if (total <= pre or system <= pre_system) return 0;
        var cores = self.cpu_stats.online_cpus;
        if (cores == 0) {
            if (self.cpu_stats.cpu_usage.percpu_usage) |list| cores = @intCast(list.len);
        }
        if (cores == 0) cores = 1;
        const cpu_delta = @as(f64, @floatFromInt(total - pre));
        const system_delta = @as(f64, @floatFromInt(system - pre_system));
        return cpu_delta / system_delta * @as(f64, @floatFromInt(cores)) * 100.0;
    }

    /// Working set: usage minus the kernel's reclaimable file cache. cgroup v2
    /// reports `inactive_file`, v1 `total_inactive_file`; the older `cache` key
    /// only exists before Docker 20.10.
    pub fn memUsed(self: Stats) u64 {
        const usage = self.memory_stats.usage;
        const stats = self.memory_stats.stats orelse return usage;
        if (stats != .object) return usage;
        const keys = [_][]const u8{ "inactive_file", "total_inactive_file", "cache" };
        for (keys) |k| {
            const v = stats.object.get(k) orelse continue;
            const cache: u64 = switch (v) {
                .integer => |i| @intCast(@max(@as(i64, 0), i)),
                .float => |f| @intFromFloat(@max(@as(f64, 0), f)),
                else => continue,
            };
            if (cache >= usage) return 0;
            return usage - cache;
        }
        return usage;
    }

    pub fn memLimit(self: Stats) u64 {
        return self.memory_stats.limit;
    }
};

test "cpu percent uses the previous sample, not the lifetime average" {
    const json =
        \\{"cpu_stats":{"cpu_usage":{"total_usage":3000000},"system_cpu_usage":40000000,"online_cpus":4},
        \\ "precpu_stats":{"cpu_usage":{"total_usage":1000000},"system_cpu_usage":20000000}}
    ;
    var parsed = try std.json.parseFromSlice(Stats, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    // (2e6 / 2e7) * 4 * 100 = 40%
    try std.testing.expectApproxEqAbs(@as(f64, 40.0), parsed.value.cpuPercent(), 0.001);
}

test "a one-shot sample has no cpu to speak of" {
    const json =
        \\{"cpu_stats":{"cpu_usage":{"total_usage":3000000},"system_cpu_usage":40000000,"online_cpus":4}}
    ;
    var parsed = try std.json.parseFromSlice(Stats, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqual(@as(f64, 0), parsed.value.cpuPercent());
}

test "memory is the working set, not the page cache" {
    const json =
        \\{"memory_stats":{"usage":104857600,"limit":1073741824,
        \\ "stats":{"inactive_file":52428800,"anon":41943040}}}
    ;
    var parsed = try std.json.parseFromSlice(Stats, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqual(@as(u64, 52428800), parsed.value.memUsed());
    try std.testing.expectEqual(@as(u64, 1073741824), parsed.value.memLimit());
}


/// Container logs. Non-TTY containers multiplex stdout and stderr with an
/// 8-byte header per frame — `[stream, 0, 0, 0, len_be32]` — while TTY
/// containers send raw bytes. Assuming one shape makes the other look like
/// binary confetti, so `demux` detects and strips only when the framing is
/// really there.
pub const Logs = struct {
    text: []u8,

    pub fn deinit(self: Logs, gpa: std.mem.Allocator) void {
        gpa.free(self.text);
    }
};

/// Strips Docker's stream headers. If the bytes do not start with a plausible
/// frame, they are returned unchanged — a TTY container's output is not framed.
pub fn demux(gpa: std.mem.Allocator, raw: []const u8, max: usize) ![]u8 {
    if (raw.len < 8) return gpa.dupe(u8, raw);
    const first = raw[0];
    if ((first != 1 and first != 2) or raw[1] != 0 or raw[2] != 0 or raw[3] != 0) {
        return gpa.dupe(u8, raw);
    }

    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    var i: usize = 0;
    while (i + 8 <= raw.len) {
        const stream = raw[i];
        const len = std.mem.readInt(u32, raw[i + 4 ..][0..4], .big);
        i += 8;
        if ((stream != 1 and stream != 2) or i + len > raw.len) {
            // Not a frame after all: give back everything, raw.
            out.deinit(gpa);
            return gpa.dupe(u8, raw);
        }
        if (out.items.len + len > max) return error.TooLarge;
        try out.appendSlice(gpa, raw[i .. i + len]);
        i += len;
    }
    if (out.items.len == 0) {
        out.deinit(gpa);
        return gpa.dupe(u8, raw);
    }
    return out.toOwnedSlice(gpa);
}

test "frames are stripped when present" {
    // two frames: "hi\n" on stdout, "oops\n" on stderr
    const raw = "\x01\x00\x00\x00\x00\x00\x00\x03hi\n" ++ "\x02\x00\x00\x00\x00\x00\x00\x05oops\n";
    const text = try demux(std.testing.allocator, raw, 1 << 20);
    defer std.testing.allocator.free(text);
    try std.testing.expectEqualStrings("hi\noops\n", text);
}

test "a tty stream is left alone" {
    const raw = "plain output\nwithout frames\n";
    const text = try demux(std.testing.allocator, raw, 1 << 20);
    defer std.testing.allocator.free(text);
    try std.testing.expectEqualStrings(raw, text);
}

test "a truncated frame falls back to raw rather than losing text" {
    const raw = "\x01\x00\x00\x00\x00\x00\x00\x40short";
    const text = try demux(std.testing.allocator, raw, 1 << 20);
    defer std.testing.allocator.free(text);
    try std.testing.expectEqualStrings(raw, text);
}

