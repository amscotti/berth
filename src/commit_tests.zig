//! The commit-time df decision: a pass that started before a mutation must not
//! put its pre-mutation disk figures back over the invalidated cache.
const std = @import("std");
const app = @import("app.zig");
const refresh = @import("refresh.zig");
const Snapshot = @import("snapshot.zig").Snapshot;

test "commit ignores a df from a generation the cache has moved past" {
    const gpa = std.testing.allocator;
    var state: app.State = .{
        .gpa = gpa,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(gpa),
        .auto_refresh = false,
    };
    defer {
        state.df.reset();
        if (state.retired) |*old| old.deinit();
        state.arena.deinit();
    }

    // A good document from before the mutation.
    try std.testing.expect(state.df.adopt(gpa, .{ .ImageUsage = .{ .TotalSize = 10 } }, 100));
    state.df.invalidate();
    const generation = state.df.generation;

    var completion: refresh.Completion = .{ .gpa = gpa };
    var job: refresh.Job = .{
        .state = &state,
        .io = std.testing.io,
        .gpa = gpa,
        .socket = try gpa.dupe(u8, "/nonexistent/commit.sock"),
        .generation = 0,
        .snapshot = Snapshot.init(gpa),
        .completion = &completion,
        .want_df = true,
        // The job started before the mutation's invalidation.
        .df_generation = generation - 1,
    };
    defer gpa.free(job.socket);
    defer job.snapshot.deinit();
    job.snapshot.df_attempted = true;
    job.snapshot.df = .{ .ImageUsage = .{ .TotalSize = 999 } };
    job.snapshot.df_at = 200;

    refresh.commit(&state, &job);

    // The stale document was not adopted, the invalidation still asks for a
    // fresh pass, and the generation advanced instead of going backwards.
    try std.testing.expectEqual(@as(i64, 10), state.df.value.?.ImageUsage.?.TotalSize);
    try std.testing.expectEqual(@as(i64, 100), state.df.at);
    try std.testing.expect(state.df.wanted());
    try std.testing.expectEqual(generation + 1, state.df.generation);
}
