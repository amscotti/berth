//! The batch a bulk confirmation names, frozen when its dialog opens. The
//! live selection and container states can change while the inline dialog is
//! up; the count on screen and the worker must still agree on one set.
const std = @import("std");
const api = @import("engine/api.zig");

pub const max = 512;
pub const id_len = 64;

pub const Batch = struct {
    ids: [max][id_len]u8 = undefined,
    lens: [max]u8 = undefined,
    count: usize = 0,

    /// Freezes ids that the caller already filtered by route. `max` is the
    /// selection cap, so the slice cannot be longer; the min guards callers.
    pub fn snapshot(self: *Batch, source: []const []const u8) void {
        self.count = @min(source.len, max);
        for (source[0..self.count], 0..) |wanted, i| {
            const n = @min(wanted.len, id_len);
            self.lens[i] = @intCast(n);
            @memcpy(self.ids[i][0..n], wanted[0..n]);
        }
    }

    pub fn id(self: *const Batch, index: usize) []const u8 {
        if (index >= self.count) return "";
        return self.ids[index][0..self.lens[index]];
    }

    /// Drops ids the engine no longer lists so the dialog's count shrinks with
    /// the bar after a commit, instead of promising a row resolve will skip.
    pub fn retain(self: *Batch, containers: []const api.Container) void {
        var kept: usize = 0;
        for (0..self.count) |i| {
            const wanted = self.id(i);
            var found = false;
            for (containers) |ct| {
                if (std.mem.eql(u8, ct.Id, wanted)) {
                    found = true;
                    break;
                }
            }
            if (!found) continue;
            if (kept != i) {
                self.ids[kept] = self.ids[i];
                self.lens[kept] = self.lens[i];
            }
            kept += 1;
        }
        self.count = kept;
    }

    /// The snapshot minus rows the engine no longer lists. A container that
    /// only changed state stays: the user confirmed the container, not a
    /// particular state transition.
    pub fn live(self: *const Batch, containers: []const api.Container, out: *[max][]const u8) usize {
        var n: usize = 0;
        for (0..self.count) |i| {
            const wanted = self.id(i);
            var found = false;
            for (containers) |ct| {
                if (std.mem.eql(u8, ct.Id, wanted)) {
                    found = true;
                    break;
                }
            }
            if (!found) continue;
            out[n] = wanted;
            n += 1;
        }
        return n;
    }
};

test "a confirmed batch ignores later selection and state changes" {
    var batch: Batch = .{};
    batch.snapshot(&[_][]const u8{"a"});

    // `b` started after the dialog opened, but it was never confirmed; `a`
    // stopped but was named, so it is still the batch.
    const after = [_]api.Container{
        .{ .Id = "a", .State = "exited" },
        .{ .Id = "b", .State = "running" },
    };
    var out: [max][]const u8 = undefined;
    try std.testing.expectEqual(@as(usize, 1), batch.live(after[0..], &out));
    try std.testing.expectEqualStrings("a", out[0]);

    // A confirmed container that disappeared has nothing left to act on.
    const gone = [_]api.Container{
        .{ .Id = "b", .State = "running" },
    };
    try std.testing.expectEqual(@as(usize, 0), batch.live(gone[0..], &out));
}

test "a refresh prunes vanished rows from the confirmation" {
    var batch: Batch = .{};
    batch.snapshot(&[_][]const u8{ "a", "b", "c" });

    const a_and_c = [_]api.Container{
        .{ .Id = "a", .State = "running" },
        .{ .Id = "c", .State = "running" },
    };
    batch.retain(a_and_c[0..]);
    try std.testing.expectEqual(@as(usize, 2), batch.count);
    try std.testing.expectEqualStrings("a", batch.id(0));
    try std.testing.expectEqualStrings("c", batch.id(1));

    const none = [_]api.Container{.{ .Id = "other", .State = "running" }};
    batch.retain(none[0..]);
    try std.testing.expectEqual(@as(usize, 0), batch.count);
}
