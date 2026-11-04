//! The cleanup run itself: which prune endpoints to call, in which order, and
//! how to report what came back. Split from `app.zig` to keep both inside the
//! 600-line rule.
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
    const engine: prune.Engine = if (s.activeRuntime()) |rt|
        switch (rt.dialect) {
            .podman => .podman,
            .docker => .docker,
        }
    else
        .docker;
    var reclaimed: u64 = 0;
    var failed = false;
    var skipped_build_cache = false;
    const build_cache_on = prune.buildCacheEnabled(s.prune_opts.build_cache, engine);
    const steps = [_]struct { on: bool, kind: prune.Kind }{
        .{ .on = s.prune_opts.containers, .kind = .containers },
        .{ .on = s.prune_opts.images, .kind = .images },
        .{ .on = s.prune_opts.volumes, .kind = .volumes },
        .{ .on = s.prune_opts.networks, .kind = .networks },
        .{ .on = build_cache_on, .kind = .build_cache },
    };
    for (steps) |step| {
        if (!step.on) continue;
        // An engine that does not have this route is a skipped step, not a
        // failed one; Podman has no build-cache endpoint at all.
        if (!prune.supported(step.kind, engine)) continue;
        var freed: u64 = 0;
        prune.runInto(&client, step.kind, engine, &freed) catch |e| {
            // Keep the bytes a multi-pass step freed before it failed.
            reclaimed += freed;
            if (e == error.EngineUnsupported) {
                if (step.kind == .build_cache) skipped_build_cache = true;
                continue;
            }
            failed = true;
            continue;
        };
        reclaimed += freed;
    }
    s.prune_open = false;
    var bytes_buf: [32]u8 = undefined;
    const freed_text = format.bytesBuf(&bytes_buf, reclaimed);
    if (failed) {
        s.notifyFmt("Cleanup freed {s}, with errors", .{freed_text}, true);
    } else if (skipped_build_cache) {
        s.notifyFmt("Freed {s} · build cache skipped", .{freed_text}, false);
    } else {
        s.notifyFmt("Freed {s}", .{freed_text}, false);
    }
    s.refresh();
}
