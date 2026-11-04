//! The machine itself, from /proc. Pure parsers so they can be tested against
//! fixed text, and a sampler that keeps the previous CPU ticks to turn
//! counters into a percentage.
const std = @import("std");

pub const Sample = struct {
    mem_total: u64 = 0,
    mem_used: u64 = 0,
    cpu_percent: f32 = 0,
    /// False until there are two tick readings to subtract.
    cpu_known: bool = false,
    /// What the engine's own processes hold. On Linux the containers are
    /// siblings of the daemon, not children of it, so this is genuinely
    /// "engine overhead" and not "everything the engine runs".
    engine_rss: u64 = 0,
};

/// `MemTotal: 16267132 kB` and friends, in bytes.
pub fn parseMemInfo(text: []const u8) Sample {
    var total: u64 = 0;
    var available: u64 = 0;
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |line| {
        const colon = std.mem.indexOfScalar(u8, line, ':') orelse continue;
        const key = std.mem.trim(u8, line[0..colon], " ");
        const rest = std.mem.trim(u8, line[colon + 1 ..], " ");
        const space = std.mem.indexOfScalar(u8, rest, ' ') orelse rest.len;
        const value = std.fmt.parseInt(u64, rest[0..space], 10) catch continue;
        if (std.mem.eql(u8, key, "MemTotal")) total = value * 1024;
        if (std.mem.eql(u8, key, "MemAvailable")) available = value * 1024;
    }
    return .{ .mem_total = total, .mem_used = if (total > available) total - available else 0 };
}

/// `cpu  4705 356 584 3699 23 0 0 0 0 0` — busy over total.
/// The processes that are the engine rather than the containers. Containerd
/// shims are deliberately absent: those are per-container overhead.
pub const ENGINE_PROCESSES = [_][]const u8{ "dockerd", "containerd", "docker-proxy", "podman" };

pub const Ticks = struct { busy: u64 = 0, total: u64 = 0 };

pub fn parseStat(text: []const u8) ?Ticks {
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, "cpu ")) continue;
        var it = std.mem.tokenizeAny(u8, line[3..], " ");
        var fields: [10]u64 = @splat(0);
        var n: usize = 0;
        while (it.next()) |tok| {
            if (n == fields.len) break;
            fields[n] = std.fmt.parseInt(u64, tok, 10) catch 0;
            n += 1;
        }
        if (n < 4) return null;
        var total: u64 = 0;
        for (fields[0..n]) |f| total += f;
        const idle = fields[3] + (if (n > 4) fields[4] else 0);
        return .{ .busy = total - idle, .total = total };
    }
    return null;
}

/// Turns two counter readings into a percentage.
pub fn percentBetween(prev: Ticks, now: Ticks) f32 {
    if (now.total <= prev.total or now.busy < prev.busy) return 0;
    const d_total = now.total - prev.total;
    const d_busy = now.busy - prev.busy;
    if (d_total == 0) return 0;
    return @as(f32, @floatFromInt(d_busy)) / @as(f32, @floatFromInt(d_total)) * 100.0;
}

/// Reads a `/proc` file. Not `readFileAlloc`: procfs reports `st_size` as 0,
/// so a size-based read returns nothing at all — an empty string, no error.
fn readProc(io: std.Io, gpa: std.mem.Allocator, path: []const u8) ![]u8 {
    const file = try std.Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    // A plain descriptor read, not the buffered Io reader: procfs files report
    // `st_size` 0, and both `readFileAlloc` and the buffered reader hand back an
    // empty string without an error. `read` until EOF is the only shape that
    // works on them.
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    var chunk: [8192]u8 = undefined;
    while (true) {
        const n = try std.posix.read(file.handle, &chunk);
        if (n == 0) break;
        if (out.items.len + n > 64 * 1024) return error.TooLarge;
        try out.appendSlice(gpa, chunk[0..n]);
    }
    return out.toOwnedSlice(gpa);
}

/// `VmRSS:     153272 kB` from a process's status file, in bytes.
pub fn parseVmRss(text: []const u8) u64 {
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, "VmRSS:")) continue;
        const rest = std.mem.trim(u8, line["VmRSS:".len..], " \t");
        var it = std.mem.tokenizeAny(u8, rest, " \t");
        const number = it.next() orelse return 0;
        const kb = std.fmt.parseInt(u64, number, 10) catch return 0;
        return kb * 1024;
    }
    return 0;
}

/// Sums the resident memory of processes whose `comm` matches one of `names`.
/// A /proc walk: there is no cheaper portable answer, and it runs once per
/// refresh over a few hundred entries.
pub fn engineRss(io: std.Io, gpa: std.mem.Allocator, names: []const []const u8) u64 {
    const proc = std.Io.Dir.openDirAbsolute(io, "/proc", .{ .iterate = true }) catch return 0;
    defer proc.close(io);
    var it = proc.iterate();
    var sum: u64 = 0;
    while (it.next(io) catch null) |entry| {
        const name = entry.name;
        if (name.len == 0 or name[0] < '0' or name[0] > '9') continue;
        var path: [64]u8 = undefined;

        const comm_path = std.fmt.bufPrint(&path, "/proc/{s}/comm", .{name}) catch continue;
        const comm = readProc(io, gpa, comm_path) catch continue;
        const matched = blk: {
            defer gpa.free(comm);
            const trimmed = std.mem.trim(u8, comm, " \n\r\t");
            for (names) |n| {
                if (std.mem.eql(u8, trimmed, n)) break :blk true;
            }
            break :blk false;
        };
        if (!matched) continue;

        const status_path = std.fmt.bufPrint(&path, "/proc/{s}/status", .{name}) catch continue;
        const status = readProc(io, gpa, status_path) catch continue;
        defer gpa.free(status);
        sum += parseVmRss(status);
    }
    return sum;
}

pub const Sampler = struct {
    prev: ?Ticks = null,

    pub fn sample(self: *Sampler, io: std.Io, gpa: std.mem.Allocator) Sample {
        var out: Sample = .{};
        if (readProc(io, gpa, "/proc/meminfo")) |text| {
            defer gpa.free(text);
            const mem = parseMemInfo(text);
            out.mem_total = mem.mem_total;
            out.mem_used = mem.mem_used;
        } else |_| {}
        if (readProc(io, gpa, "/proc/stat")) |text| {
            defer gpa.free(text);
            if (parseStat(text)) |ticks| {
                if (self.prev) |prev| {
                    out.cpu_percent = percentBetween(prev, ticks);
                    out.cpu_known = true;
                }
                self.prev = ticks;
            }
        } else |_| {}
        return out;
    }
};

test "vmrss reads a process's resident memory" {
    const text = "Name:\tdockerd\nVmPeak:\t2000000 kB\nVmRSS:\t153272 kB\nThreads:\t42\n";
    try std.testing.expectEqual(@as(u64, 153272 * 1024), parseVmRss(text));
    try std.testing.expectEqual(@as(u64, 0), parseVmRss("VmRSS: notanumber kB"));
}

test "meminfo reads total and available" {
    const text =
        \\MemTotal:       16267132 kB
        \\MemFree:         1234567 kB
        \\MemAvailable:    9876543 kB
    ;
    const s = parseMemInfo(text);
    try std.testing.expectEqual(@as(u64, 16267132 * 1024), s.mem_total);
    try std.testing.expectEqual(@as(u64, (16267132 - 9876543) * 1024), s.mem_used);
}

test "stat sums the cpu line" {
    const text =
        \\cpu  100 0 100 700 100 0 0 0 0 0
        \\cpu0 50 0 50 350 50 0 0 0 0 0
    ;
    const t = parseStat(text).?;
    try std.testing.expectEqual(@as(u64, 1000), t.total);
    try std.testing.expectEqual(@as(u64, 200), t.busy);
}

test "percent between two readings" {
    const a: Ticks = .{ .busy = 100, .total = 1000 };
    const b: Ticks = .{ .busy = 200, .total = 2000 };
    try std.testing.expectApproxEqAbs(@as(f32, 10.0), percentBetween(a, b), 0.001);
}

test "a counter that went backwards reports nothing rather than a wrong number" {
    const a: Ticks = .{ .busy = 200, .total = 2000 };
    const b: Ticks = .{ .busy = 100, .total = 1000 };
    try std.testing.expectEqual(@as(f32, 0), percentBetween(a, b));
}
