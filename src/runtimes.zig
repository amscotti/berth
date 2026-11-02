//! Runtime records outlive a refresh and are rebuilt only on discovery: the
//! strings belong to the state, not a snapshot arena, so they are freed here.
const app = @import("app.zig");
const discovery = @import("engine/discovery.zig");

pub fn drop(s: *app.State) void {
    if (s.runtimes.len == 0) return;
    for (s.runtimes) |*r| discovery.freeRuntime(s.gpa, r);
    s.gpa.free(s.runtimes);
    s.runtimes = &.{};
}
