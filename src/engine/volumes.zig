//! Filling volume sizes from the df document, separate from the API types so
//! `api.zig` can stay inside the line rule.
const std = @import("std");
const api = @import("api.zig");

/// `/volumes` omits UsageData on Docker; `/system/df` carries it. Fill the
/// missing sizes so the volume table matches the Disk page.
pub fn mergeVolumeUsage(volumes: []api.Volume, df: api.Df) void {
    const usage = df.Volumes orelse return;
    for (volumes) |*v| {
        // -1 is "not measured", so the df may still fill it.
        if (v.UsageData) |u| if (u.Size >= 0) continue;
        for (usage) |u| {
            if (std.mem.eql(u8, v.Name, u.Name)) {
                if (u.UsageData) |next| {
                    // Keep a known RefCount when the df did not measure size.
                    if (next.Size >= 0) v.UsageData = next;
                }
                break;
            }
        }
    }
}
