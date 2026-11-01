//! The host machine, whichever it is: one sampler that answers the same three
//! questions everywhere — how much memory, how much in use, how busy the CPU.
const std = @import("std");
const builtin = @import("builtin");

const impl = switch (builtin.os.tag) {
    .linux => @import("linux.zig"),
    .macos => @import("macos.zig"),
    else => @import("none.zig"),
};

pub const Sample = impl.Sample;
pub const Sampler = impl.Sampler;
