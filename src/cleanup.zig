//! The cleanup run itself: which prune endpoints to call, in which order, and
//! how to report what came back. Split from `app.zig` to keep both inside the
//! 600-line rule.
const std = @import("std");
const app = @import("app.zig");
const prune = @import("engine/prune.zig");
const format = @import("ui/format.zig");

/// Containers first: removing a stopped container can make its image and
/// volumes unused for the prunes that follow in the same pass.
pub fn runPrune(s: *app.State) void {
    var client = s.client orelse {
        s.closePrune();
        return;
    };
    var reclaimed: u64 = 0;
    var failed = false;
    const steps = [_]struct { on: bool, kind: prune.Kind }{
        .{ .on = s.prune_opts.containers, .kind = .containers },
        .{ .on = s.prune_opts.images, .kind = .images },
        .{ .on = s.prune_opts.volumes, .kind = .volumes },
        .{ .on = s.prune_opts.networks, .kind = .networks },
        .{ .on = s.prune_opts.build_cache, .kind = .build_cache },
    };
    for (steps) |step| {
        if (!step.on) continue;
        reclaimed += prune.run(&client, step.kind) catch blk: {
            failed = true;
            break :blk 0;
        };
    }
    s.prune_open = false;
    if (failed) {
        s.notify("Cleanup finished with errors", true);
    } else {
        var buf: [32]u8 = undefined;
        s.notifyFmt("Freed {s}", .{format.bytesBuf(&buf, reclaimed)}, false);
    }
    s.refresh();
}
