//! Tests: protocol, fixtures and formatting, with no window and no engine.
test {
    _ = @import("engine/http_uds.zig");
    _ = @import("engine/api.zig");
    _ = @import("engine/stats.zig");
    _ = @import("engine/discovery.zig");
    _ = @import("engine/prune.zig");
    _ = @import("tabs.zig");
    _ = @import("settings.zig");
    _ = @import("ui/views/lists.zig");
    _ = @import("ui/views/stacks.zig");
    _ = @import("ui/format.zig");
    _ = @import("ui/widgets.zig");
    _ = @import("ui/theme.zig");
    _ = @import("host/host.zig");
    _ = @import("ui/history.zig");
}
