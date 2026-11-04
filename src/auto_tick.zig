//! The automatic refresh timer. A pass re-arms itself from the end of the
//! chain, never on top of one still running. The queue has no cancel, so a
//! tick that outlives the state is detached: its callback frees only itself.
const std = @import("std");
const builtin = @import("builtin");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const refresh = @import("refresh.zig");

pub const Tick = struct {
    /// Nulled by teardown. The callback must not touch the state after this;
    /// it still owns the tick allocation and frees it.
    state: ?*app.State,
    token: u64,
};

/// Arms the one automatic pass timer. The flag is set even under the test
/// runner, which has no event loop; it lets tests prove repeated resumes do
/// not stack timers without scheduling a callback against a stack state.
pub fn arm(s: *app.State) void {
    if (!s.auto_refresh or s.auto_tick_pending) return;
    s.auto_tick_pending = true;
    if (builtin.is_test) return;
    // Outside the state's allocator: a callback that arrives after teardown
    // still has to free this without touching anything the state owned. One
    // that never arrives is one page reclaimed by the OS at exit.
    const tick = std.heap.page_allocator.create(Tick) catch {
        s.auto_tick_pending = false;
        return;
    };
    tick.* = .{ .state = s, .token = s.auto_tick_token };
    s.auto_ticks.append(s.gpa, tick) catch {
        std.heap.page_allocator.destroy(tick);
        s.auto_tick_pending = false;
        return;
    };
    nanoui.App.after(5000, tick, bridge);
}

fn bridge(ctx: ?*anyopaque) callconv(.c) void {
    const tick: *Tick = @ptrCast(@alignCast(ctx.?));
    const s = tick.state orelse {
        // Teardown detached this tick. The queue cannot cancel it, so the
        // callback is the last owner: free it without touching the state or
        // its list, which may already be gone.
        std.heap.page_allocator.destroy(tick);
        return;
    };
    for (s.auto_ticks.items, 0..) |pending, i| {
        if (pending != tick) continue;
        _ = s.auto_ticks.orderedRemove(i);
        break;
    }
    const token = tick.token;
    std.heap.page_allocator.destroy(tick);
    refresh.autoTick(s, token);
}

/// Detaches timers the main queue will never run because the state is going
/// away. Their allocations stay alive for the queued callback, which will free
/// them once it finds the null state; a callback that never runs leaks one
/// small tick at process exit and nothing else.
pub fn discard(s: *app.State) void {
    for (s.auto_ticks.items) |tick| tick.state = null;
    s.auto_ticks.deinit(s.gpa);
    s.auto_tick_pending = false;
}

test "a late bridge after teardown frees only the detached tick" {
    const state = try std.testing.allocator.create(app.State);
    state.* = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = undefined,
        .auto_tick_pending = true,
    };
    const tick = try std.heap.page_allocator.create(Tick);
    tick.* = .{ .state = state, .token = 0 };
    try state.auto_ticks.append(state.gpa, tick);
    discard(state);
    try std.testing.expect(tick.state == null);
    // The state is really gone before the callback arrives. It must free the
    // tick exactly once and touch neither the state nor the cleared list.
    std.testing.allocator.destroy(state);
    bridge(@ptrCast(tick));
}
