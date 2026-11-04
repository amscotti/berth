//! The inline sheets share one modal slot: a confirmation, a bulk action, the
//! cleanup choices, or a create form. Opening one dismisses the rest, because
//! two stacked destructive questions cannot both be answered unambiguously.
const std = @import("std");
const app = @import("app.zig");

pub fn open(s: *const app.State) bool {
    return s.pending != null or s.bulk.pending != null or s.ops.pending != null or
        s.prune_open or s.create_dialog.open != null;
}

/// Closes every sheet and forgets choices that belonged to the one just
/// dismissed. Selections in the table stay; only the pending questions go.
pub fn dismiss(s: *app.State) void {
    s.clearPending();
    s.bulk.pending = null;
    s.bulk.pending_batch.count = 0;
    s.bulk.project_len = 0;
    s.ops.dismissPending();
    s.prune_open = false;
    resetPruneOpts(s);
    if (s.create_dialog.open != null) {
        s.create_dialog.open = null;
        s.create_dialog.generation +%= 1;
        s.create_dialog.attempted = false;
    }
}

/// Volumes and networks outlive their containers, so they are opt-in for one
/// cleanup only. Build cache keeps the user's preference: `buildCacheEnabled`
/// suppresses it on Podman without destroying the choice for Docker.
pub fn resetPruneOpts(s: *app.State) void {
    s.prune_opts.volumes = false;
    s.prune_opts.networks = false;
}

fn testState() app.State {
    return .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
}

test "a cancelled cleanup forgets the volume and network opt-ins" {
    var state = testState();
    defer state.arena.deinit();

    state.prune_opts = .{ .volumes = true, .networks = true };
    state.openPrune();
    try std.testing.expect(state.prune_open);
    try std.testing.expect(!state.prune_opts.volumes);
    try std.testing.expect(!state.prune_opts.networks);

    state.prune_opts.volumes = true;
    state.prune_opts.networks = true;
    state.closePrune();
    try std.testing.expect(!state.prune_open);
    try std.testing.expect(!state.prune_opts.volumes);
    try std.testing.expect(!state.prune_opts.networks);
}

test "an engine switch clears the sheets and their opt-ins" {
    var state = testState();
    defer state.arena.deinit();

    state.prune_open = true;
    state.prune_opts = .{ .volumes = true, .networks = true };
    state.bulk.pending = .{ .route = .stop };
    state.create_dialog.open = .volume;
    state.create_dialog.attempted = true;

    state.clearData();
    try std.testing.expect(!state.prune_open);
    try std.testing.expect(!state.prune_opts.volumes);
    try std.testing.expect(!state.prune_opts.networks);
    try std.testing.expect(state.bulk.pending == null);
    try std.testing.expect(state.create_dialog.open == null);
    try std.testing.expect(!state.create_dialog.attempted);
}

test "a Podman visit does not clear the Docker build-cache preference" {
    var state = testState();
    defer state.arena.deinit();

    // Volumes and networks are per-cleanup opt-ins. Build cache is a saved
    // preference: the Podman sheet hides it via `buildCacheEnabled`, but the
    // stored value survives for the next Docker engine.
    state.prune_opts = .{ .build_cache = true, .volumes = true, .networks = true };
    resetPruneOpts(&state);
    try std.testing.expect(state.prune_opts.build_cache);
    try std.testing.expect(!state.prune_opts.volumes);
    try std.testing.expect(!state.prune_opts.networks);
}
