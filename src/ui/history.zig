//! A short ring of samples, so the overview can show what has been happening
//! rather than only what is happening now.
//!
//! Fixed capacity and a shift on overflow: 120 floats is 480 bytes, and the
//! cost of moving them once every few seconds is nothing next to the cost of
//! a ring buffer's index arithmetic in every read.
const std = @import("std");

pub const Cap = 120;

pub const Which = enum { cpu, mem, running, disk, host_cpu, host_mem };

pub const Sample = struct {
    cpu: f32 = 0,
    mem: f32 = 0,
    running: f32 = 0,
    disk: f32 = 0,
    host_cpu: f32 = 0,
    host_mem: f32 = 0,
};

pub const History = struct {
    cpu: [Cap]f32 = @splat(0),
    mem: [Cap]f32 = @splat(0),
    running: [Cap]f32 = @splat(0),
    disk: [Cap]f32 = @splat(0),
    host_cpu: [Cap]f32 = @splat(0),
    host_mem: [Cap]f32 = @splat(0),
    count: usize = 0,

    pub fn push(h: *History, s: Sample) void {
        if (h.count == Cap) {
            // Drop the oldest point by shifting everything down one.
            std.mem.copyForwards(f32, h.cpu[0 .. Cap - 1], h.cpu[1..Cap]);
            std.mem.copyForwards(f32, h.mem[0 .. Cap - 1], h.mem[1..Cap]);
            std.mem.copyForwards(f32, h.running[0 .. Cap - 1], h.running[1..Cap]);
            std.mem.copyForwards(f32, h.disk[0 .. Cap - 1], h.disk[1..Cap]);
            std.mem.copyForwards(f32, h.host_cpu[0 .. Cap - 1], h.host_cpu[1..Cap]);
            std.mem.copyForwards(f32, h.host_mem[0 .. Cap - 1], h.host_mem[1..Cap]);
            h.count = Cap - 1;
        }
        h.cpu[h.count] = s.cpu;
        h.mem[h.count] = s.mem;
        h.running[h.count] = s.running;
        h.disk[h.count] = s.disk;
        h.host_cpu[h.count] = s.host_cpu;
        h.host_mem[h.count] = s.host_mem;
        h.count += 1;
    }

    pub fn values(h: *const History, which: Which) []const f32 {
        return switch (which) {
            .cpu => h.cpu[0..h.count],
            .mem => h.mem[0..h.count],
            .running => h.running[0..h.count],
            .disk => h.disk[0..h.count],
            .host_cpu => h.host_cpu[0..h.count],
            .host_mem => h.host_mem[0..h.count],
        };
    }

    /// The latest value, or zero before the first sample.
    pub fn latest(h: *const History, which: Which) f32 {
        const v = h.values(which);
        return if (v.len == 0) 0 else v[v.len - 1];
    }

    /// Forget the series when the engine changes. The host card reads its
    /// current sample, not this ring, so resetting the count is honest and
    /// stops the new engine's graph from starting with ten minutes of zeros.
    pub fn clearEngine(h: *History) void {
        h.count = 0;
    }

    /// The busiest value in the series, for scaling a graph.
    pub fn peak(h: *const History, which: Which) f32 {
        var most: f32 = 0;
        for (h.values(which)) |v| most = @max(most, v);
        return most;
    }
};

test "history fills, then drops the oldest" {
    var h: History = .{};
    try std.testing.expectEqual(@as(usize, 0), h.values(.cpu).len);
    h.push(.{ .cpu = 1 });
    try std.testing.expectEqual(@as(f32, 1), h.latest(.cpu));
    var i: usize = 2;
    while (i <= Cap + 5) : (i += 1) h.push(.{ .cpu = @floatFromInt(i) });
    try std.testing.expectEqual(@as(usize, Cap), h.count);
    // 1, then 2..=Cap+5 is Cap+5 samples; the window keeps the newest Cap of
    // them, which start at (Cap + 6) - Cap = 6 and end at Cap + 5.
    try std.testing.expectEqual(@as(f32, @floatFromInt(Cap + 5)), h.latest(.cpu));
    try std.testing.expectEqual(@as(f32, 6), h.values(.cpu)[0]);
    try std.testing.expectEqual(@as(f32, @floatFromInt(Cap + 5)), h.peak(.cpu));
}

test "an empty history reads as zero, not uninitialised memory" {
    var h: History = .{};
    try std.testing.expectEqual(@as(f32, 0), h.latest(.mem));
    try std.testing.expectEqual(@as(f32, 0), h.peak(.host_cpu));
    try std.testing.expectEqual(@as(usize, 0), h.values(.disk).len);
}

test "each series keeps its own column" {
    var h: History = .{};
    h.push(.{ .cpu = 1, .mem = 2, .running = 3, .disk = 4, .host_cpu = 5, .host_mem = 6 });
    h.push(.{ .cpu = 7, .mem = 8, .running = 9, .disk = 10, .host_cpu = 11, .host_mem = 12 });
    try std.testing.expectEqual(@as(f32, 7), h.latest(.cpu));
    try std.testing.expectEqual(@as(f32, 8), h.latest(.mem));
    try std.testing.expectEqual(@as(f32, 9), h.latest(.running));
    try std.testing.expectEqual(@as(f32, 10), h.latest(.disk));
    try std.testing.expectEqual(@as(f32, 11), h.latest(.host_cpu));
    try std.testing.expectEqual(@as(f32, 12), h.latest(.host_mem));
    try std.testing.expectEqual(@as(f32, 7), h.peak(.cpu));
    try std.testing.expectEqual(@as(f32, 12), h.peak(.host_mem));
}
