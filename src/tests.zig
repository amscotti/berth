//! Tests: protocol, fixtures and formatting, with no window and no engine.
test {
    _ = @import("engine/http_uds.zig");
    _ = @import("engine/api.zig");
    _ = @import("engine/discovery.zig");
    _ = @import("ui/format.zig");
}
