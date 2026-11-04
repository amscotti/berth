//! The cleanup sheet's entry point. The run itself is an action job, so it is
//! kept in `actions.zig`; this stays beside the dialog it belongs to.
const app = @import("app.zig");
const actions = @import("actions.zig");

pub fn runPrune(s: *app.State) void {
    _ = actions.runPrune(s);
}
