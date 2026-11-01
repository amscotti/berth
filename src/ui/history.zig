//! A short ring of samples, so the overview can show what has been happening
//! rather than only what is happening now.
//!
//! Fixed capacity and a shift on overflow: 120 floats is 480 bytes, and the
//! cost of moving them once every few seconds is nothing next to the cost of
//! a ring buffer's index arithmetic in every read.
const std = @import("std");

pub const Cap = 120;

pub const Which = enum { cpu, mem, running, disk };

pub const Sample = struct {
    cpu: f32 = 0,
    mem: f32 = 0,
    running: f32 = 0,
    disk: f32 = 0,
};

pub const History = struct {
    cpu: [Cap]f32 = @splat(0),
    mem: [Cap]f32 = @splat(0),
    running: [Cap]f32 = @splat(0),
    disk: [Cap]f32 = @splat(0),
    count: usize = 0,

    pub fn push(h: *History, s: Sample) void {
        if (h.count == Cap) {
            // Drop the oldest point by shifting everything down one.
            std.mem.copyForwards(f32, h.cpu[0 .. Cap - 1], h.cpu[1..Cap]);
            std.mem.copyForwards(f32, h.mem[0 .. Cap - 1], h.mem[1..Cap]);
            std.mem.copyForwards(f32, h.running[0 .. Cap - 1], h.running[1..Cap]);
            std.mem.copyForwards(f32, h.disk[0 .. Cap - 1], h.disk[1..Cap]);
            h.count = Cap - 1;
        }
        h.cpu[h.count] = s.cpu;
        h.mem[h.count] = s.mem;
        h.running[h.count] = s.running;
        h.disk[h.count] = s.disk;
        h.count += 1;
    }

    pub fn values(h: *const History, which: Which) []const f32 {
        return switch (which) {
            .cpu => h.cpu[0..h.count],
            .mem => h.mem[0..h.count],
            .running => h.running[0..h.count],
            .disk => h.disk[0..h.count],
        };
    }

    /// The latest value, or zero before the first sample.
    pub fn latest(h: *const History, which: Which) f32 {
        const v = h.values(which);
        return if (v.len == 0) 0 else v[v.len - 1];
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
    // The window keeps the newest Cap samples: 11..=130.
    try std.testing.expectEqual(@as(f32, @floatFromInt(Cap + 5)), h.latest(.cpu));
    try std.testing.expectEqual(@as(f32, 11), h.values(.cpu)[0]);
    try std.testing.expectEqual(@as(f32, @floatFromInt(Cap + 5)), h.peak(.cpu));
}
