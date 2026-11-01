//! The Mac, through sysctl only.
//!
//! **Written but not yet compiled**: this machine runs Linux and Zig needs the
//! macOS SDK to build for Darwin, so the first build of this file will happen
//! on the Mac. It is kept deliberately small — `sysctlbyname` with integer
//! results, no Mach structs — so there is as little as possible to get wrong.
//!
//! `kern.cp_time` is an array of CPU ticks (user, nice, sys, idle, intr); the
//! arithmetic is the same as Linux's `/proc/stat` from a different source.
const std = @import("std");

pub const Sample = struct {
    mem_total: u64 = 0,
    mem_used: u64 = 0,
    cpu_percent: f32 = 0,
    engine_rss: u64 = 0,
};

extern "c" fn sysctlbyname(
    name: [*:0]const u8,
    oldp: ?*anyopaque,
    oldlenp: ?*usize,
    newp: ?*anyopaque,
    newlen: usize,
) c_int;

/// A sysctl that answers with one integer, of either 32 or 64 bits — macOS is
/// inconsistent about which, so the returned length decides.
fn intByName(name: [*:0]const u8) ?u64 {
    var buf: [8]u8 = @splat(0);
    var len: usize = buf.len;
    if (sysctlbyname(name, &buf, &len, null, 0) != 0) return null;
    return switch (len) {
        4 => std.mem.readInt(u32, buf[0..4], .little),
        8 => std.mem.readInt(u64, buf[0..8], .little),
        else => null,
    };
}

pub const Ticks = struct { busy: u64 = 0, total: u64 = 0 };

/// The `kern.cp_time` counters folded into busy and total. The fourth is idle,
/// so everything else is work.
pub fn parseCpTime(values: []const u64) Ticks {
    if (values.len < 4) return .{};
    var total: u64 = 0;
    for (values) |v| total += v;
    return .{ .busy = total - values[3], .total = total };
}

pub fn percentBetween(prev: Ticks, now: Ticks) f32 {
    if (now.total <= prev.total or now.busy < prev.busy) return 0;
    const d_total = now.total - prev.total;
    const d_busy = now.busy - prev.busy;
    if (d_total == 0) return 0;
    return @as(f32, @floatFromInt(d_busy)) / @as(f32, @floatFromInt(d_total)) * 100.0;
}

pub const Sampler = struct {
    prev: ?Ticks = null,

    pub fn sample(self: *Sampler, io: std.Io, gpa: std.mem.Allocator) Sample {
        _ = io;
        _ = gpa;
        var out: Sample = .{};

        out.mem_total = intByName("hw.memsize") orelse 0;

        // "In use" is active + wired + compressed: the pages the system cannot
        // hand out again without swapping. Free pages alone would count every
        // cached file as used, which is how memory monitors end up alarming.
        const page_size = intByName("hw.pagesize") orelse 4096;
        const active = intByName("vm.page_active_count") orelse 0;
        const wired = intByName("vm.page_wire_count") orelse 0;
        const compressed = intByName("vm.compressor_page_count") orelse 0;
        out.mem_used = (active + wired + compressed) * page_size;
        if (out.mem_used > out.mem_total) out.mem_used = out.mem_total;

        var cp_time: [5]u64 = @splat(0);
        var len: usize = @sizeOf(@TypeOf(cp_time));
        if (sysctlbyname("kern.cp_time", &cp_time, &len, null, 0) == 0) {
            const ticks = parseCpTime(&cp_time);
            if (self.prev) |prev| out.cpu_percent = percentBetween(prev, ticks);
            self.prev = ticks;
        }
        return out;
    }
};

test "cp_time folds into busy and total" {
    const t = parseCpTime(&[_]u64{ 100, 0, 100, 700, 100 });
    try std.testing.expectEqual(@as(u64, 1000), t.total);
    try std.testing.expectEqual(@as(u64, 300), t.busy);
}
