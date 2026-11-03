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

/// Resident memory of the engine's own host processes, or null when the
/// platform cannot tell. Called from the refresh worker: on macOS this spawns
/// `ps`, which must not happen between a click and its frame.
pub fn engineRss(io: std.Io, gpa: std.mem.Allocator) ?u64 {
    return switch (builtin.os.tag) {
        .macos => impl.engineRss(io, gpa),
        .linux => impl.engineRss(io, gpa, &impl.ENGINE_PROCESSES),
        else => null,
    };
}
