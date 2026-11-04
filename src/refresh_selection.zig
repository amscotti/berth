//! Re-resolve the inspector selection after a snapshot swap. Ids, not slices,
//! are the stable handle: every refresh rebuilds the lists in a new arena.
const std = @import("std");
const app = @import("app.zig");
const logs = @import("logs.zig");
const detail_job = @import("detail_job.zig");
const ops = @import("ops.zig");

pub fn resolve(s: *app.State) void {
    if (s.selected) |old_id| {
        var kept = false;
        switch (s.detail_kind) {
            .container => for (s.containers) |ct| {
                if (std.mem.eql(u8, ct.Id, old_id)) {
                    s.selected = ct.Id;
                    s.selected_name = ct.name();
                    kept = true;
                    break;
                }
            },
            .image => for (s.images) |im| {
                if (std.mem.eql(u8, im.Id, old_id)) {
                    s.selected = im.Id;
                    s.selected_name = if (im.dangling()) "untagged" else im.tag();
                    kept = true;
                    break;
                }
            },
            .volume => for (s.volumes) |v| {
                if (std.mem.eql(u8, v.Name, old_id)) {
                    s.selected = v.Name;
                    s.selected_name = v.Name;
                    kept = true;
                    break;
                }
            },
            .network => for (s.networks) |n| {
                if (std.mem.eql(u8, n.Id, old_id) or std.mem.eql(u8, n.Name, old_id)) {
                    s.selected = if (n.Id.len > 0) n.Id else n.Name;
                    s.selected_name = n.Name;
                    kept = true;
                    break;
                }
            },
        }
        if (!kept) {
            s.selected = null;
            s.selected_name = "";
            detail_job.invalidate(s);
            logs.clear(s);
            s.clearInspect();
        }
    }
    if (s.pending_select.len > 0) selectPending(s);
    if (s.pending_sheet.len > 0) applyPendingSheet(s);
}

/// Headless snapshots need sheets whose subject only exists after a pass:
/// a bulk selection with real rows, a container confirmation, or an engine
/// confirmation with its live count.
fn applyPendingSheet(s: *app.State) void {
    const spec = s.pending_sheet;
    s.pending_sheet = "";
    if (std.mem.eql(u8, spec, "bulk-all")) {
        s.bulk.enabled = true;
        _ = s.bulk.selectShown(s.containers, s.filter, s.search.containers.text());
        return;
    }
    if (std.mem.startsWith(u8, spec, "confirm:")) {
        const rest = spec["confirm:".len..];
        const colon = std.mem.indexOfScalar(u8, rest, ':') orelse return;
        const verb = rest[0..colon];
        const want = rest[colon + 1 ..];
        for (s.containers) |ct| {
            if (!std.mem.eql(u8, ct.name(), want)) continue;
            if (std.mem.eql(u8, verb, "delete")) {
                s.confirm(ct.Id, ct.name(), "Delete", "DELETE", "/containers/{s}?force=1");
            } else {
                s.confirm(ct.Id, ct.name(), "Stop", "POST", "/containers/{s}/stop");
            }
            return;
        }
        return;
    }
    if (std.mem.eql(u8, spec, "engine-stop") or std.mem.eql(u8, spec, "engine-restart")) {
        const rt = s.activeRuntime() orelse return;
        ops.confirm(s, if (std.mem.eql(u8, spec, "engine-stop")) .stop else .restart, rt, app.runningCount(s));
    }
}

/// `BERTH_SELECT` for headless snapshots: a container name, or
/// `image:tag`, `volume:name`, `network:name` for the other inspectors.
fn selectPending(s: *app.State) void {
    const spec = s.pending_select;
    if (spec.len == 0) return;
    s.pending_select = "";

    if (std.mem.startsWith(u8, spec, "image:")) {
        const want = spec["image:".len..];
        for (s.images) |im| {
            const label = if (im.dangling()) "untagged" else im.tag();
            if (std.mem.eql(u8, label, want) or std.mem.eql(u8, im.Id, want)) {
                s.selectImage(&im);
                return;
            }
        }
    } else if (std.mem.startsWith(u8, spec, "volume:")) {
        const want = spec["volume:".len..];
        for (s.volumes) |v| {
            if (std.mem.eql(u8, v.Name, want)) {
                s.selectVolume(&v);
                return;
            }
        }
    } else if (std.mem.startsWith(u8, spec, "network:")) {
        const want = spec["network:".len..];
        for (s.networks) |n| {
            if (std.mem.eql(u8, n.Name, want)) {
                s.selectNetwork(&n);
                return;
            }
        }
    } else {
        for (s.containers) |ct| {
            if (std.mem.eql(u8, ct.name(), spec)) {
                s.select(&ct);
                return;
            }
        }
    }
}
