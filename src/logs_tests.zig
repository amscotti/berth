//! Log buffer ownership at the state boundary: refresh-follow tails and
//! detail reads both hand the pane page-owned text, and a stale clock must not
//! replace the one the user asked for.
const std = @import("std");
const app = @import("app.zig");
const logs = @import("logs.zig");

fn testState() app.State {
    return .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
    };
}

test "refresh log snapshots replace and clear page-owned text only on the same clock" {
    var state = testState();
    defer state.arena.deinit();
    defer logs.clear(&state);
    state.detail_kind = .container;
    state.selected = "abc";
    state.log_opts.timestamps = false;

    logs.commitSnapshot(&state, "pass one", "abc", false);
    try std.testing.expectEqualStrings("pass one", state.log_text);
    const first = state.log_buf.?.ptr;
    // A pass that started before the Times toggle cannot put the old clock
    // back over the current one.
    logs.commitSnapshot(&state, "stale clock", "abc", true);
    try std.testing.expectEqualStrings("pass one", state.log_text);
    try std.testing.expectEqual(first, state.log_buf.?.ptr);

    logs.commitSnapshot(&state, "pass two", "abc", false);
    try std.testing.expectEqualStrings("pass two", state.log_text);
    logs.clear(&state);
    try std.testing.expect(state.log_buf == null);
    try std.testing.expectEqualStrings("", state.log_text);
}
