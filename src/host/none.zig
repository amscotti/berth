//! Nothing to read here: the figures stay zero and the view says so.
const std = @import("std");

pub const Sample = struct {
    mem_total: u64 = 0,
    mem_used: u64 = 0,
    cpu_percent: f32 = 0,
    engine_rss: u64 = 0,
};

pub const Sampler = struct {
    pub fn sample(_: *Sampler, _: std.Io, _: std.mem.Allocator) Sample {
        return .{};
    }
};
