//! A refresh is a worker thread, not a chain of callbacks on the UI thread.
//! The worker builds a complete snapshot over its own socket and arena, then
//! posts one `done` callback to the main queue. The window never waits on an
//! engine request; it keeps painting the previous snapshot until the swap.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const volumes_mod = @import("engine/volumes.zig");
const discovery = @import("engine/discovery.zig");
const format = @import("ui/format.zig");
const stats_pool = @import("stats_pool.zig");
const engine_events = @import("engine/events.zig");
const events_feed = @import("events_feed.zig");
const logs = @import("logs.zig");
const bulk = @import("bulk.zig");
const tray = @import("tray.zig");
const image_tasks = @import("image_tasks.zig");
const host = @import("host/host.zig");
const Snapshot = @import("snapshot.zig").Snapshot;
const Stat = stats_pool.Sample;

pub const Job = struct {
    state: *app.State,
    io: std.Io,
    gpa: std.mem.Allocator,
    socket: []u8,
    generation: u64,
    snapshot: Snapshot,
    follow: bool = false,
    follow_id: [64]u8 = undefined,
    follow_len: usize = 0,
    logs_timestamps: bool = false,
    error_text: []const u8 = "",
    engine_dead: bool = false,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    thread: ?std.Thread = null,
};

pub fn start(s: *app.State) void {
    if (s.client == null or s.job != null) return;
    const client = &s.client.?;
    const socket = s.gpa.dupe(u8, client.socket) catch {
        s.error_text = "out of memory preparing a refresh";
        s.redraw();
        return;
    };
    const job = s.gpa.create(Job) catch {
        s.gpa.free(socket);
        s.error_text = "out of memory preparing a refresh";
        s.redraw();
        return;
    };
    job.* = .{
        .state = s,
        .io = s.io,
        .gpa = s.gpa,
        .socket = socket,
        .generation = s.generation,
        .snapshot = Snapshot.init(s.gpa),
        .follow = s.log_opts.follow and s.detail_kind == .container,
        .logs_timestamps = s.log_opts.timestamps,
    };
    if (s.selected) |id| {
        job.follow_len = @min(id.len, job.follow_id.len);
        @memcpy(job.follow_id[0..job.follow_len], id[0..job.follow_len]);
    }
    job.thread = std.Thread.spawn(.{}, work, .{job}) catch {
        job.snapshot.deinit();
        s.gpa.free(socket);
        s.gpa.destroy(job);
        s.error_text = "could not start a refresh";
        s.redraw();
        if (s.auto_refresh) nanoui.App.after(5000, s, autoTick);
        return;
    };
    s.job = job;
    s.error_text = "";
    s.redraw();
}

fn work(job: *Job) void {
    load(job);
    nanoui.App.after(0, job, done);
}

fn load(job: *Job) void {
    var client = api.Client.init(job.io, job.gpa, job.socket);
    client.alloc = job.snapshot.arena.allocator();

    if (!discovery.probe(&client)) {
        job.error_text = "engine stopped answering";
        job.engine_dead = true;
        return;
    }
    const version = client.version() catch {
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not read the engine version";
        return;
    };
    client.adoptApi(version.ApiVersion);
    job.snapshot.containers = client.containers() catch {
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list containers";
        return;
    };

    job.snapshot.stats = job.snapshot.arena.allocator().alloc(Stat, job.snapshot.containers.len) catch &.{};
    for (job.snapshot.containers, 0..) |ct, i| {
        if (i < job.snapshot.stats.len) job.snapshot.stats[i] = .{ .id = ct.Id };
    }
    // Stats are sampled on every pass, whichever page is open: the worker is
    // off the UI thread, and a snapshot without them would show 0.0% for the
    // first few seconds after switching to Containers.
    stats_pool.sample(job.io, job.socket, client.api, job.snapshot.containers, job.snapshot.stats);

    // A failed list must abort the snapshot, not commit an empty one: losing
    // images/volumes/networks for a pass reads as "they were deleted".
    job.snapshot.images = client.images() catch {
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list images";
        return;
    };
    const volumes = client.volumes() catch {
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list volumes";
        return;
    };
    job.snapshot.volumes = volumes.Volumes;
    job.snapshot.networks = client.networks() catch {
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not list networks";
        return;
    };
    if (job.follow_len > 0) {
        const id = job.follow_id[0..job.follow_len];
        if (client.logs(id, 500, job.logs_timestamps)) |text| {
            job.snapshot.logs = text;
            job.snapshot.logs_id = job.snapshot.arena.allocator().dupe(u8, id) catch "";
            job.snapshot.logs_timestamps = job.logs_timestamps;
        } else |_| {}
    }

    job.snapshot.df = client.df() catch null;

    // Engine helper memory: `ps` on macOS, a /proc walk on Linux. Sampled here
    // on the worker; it is the one host figure that needs a process spawn.
    job.snapshot.engine_rss = host.engineRss(job.io, job.gpa);

    // Events are an interval, not a store: a failed call must not abort the
    // snapshot, and the cursor only moves when the interval actually arrived.
    const until_events = format.now();
    var since = job.state.events.since;
    if (since == 0) since = until_events - 600;
    if (client.events(since, until_events)) |body| {
        defer job.gpa.free(body);
        var fresh = engine_events.parse(job.snapshot.arena.allocator(), body);
        // A first load can cover thousands of events; keep the newest Cap so
        // the merge on the main thread stays small.
        if (fresh.len > engine_events.Cap) fresh = fresh[fresh.len - engine_events.Cap ..];
        job.snapshot.events = fresh;
        job.snapshot.events_since = since;
        job.snapshot.events_until = until_events;
    } else |_| {}
}

fn done(ctx: ?*anyopaque) callconv(.c) void {
    const job: *Job = @ptrCast(@alignCast(ctx.?));
    if (job.thread) |thread| thread.join();
    const s = job.state;
    if (s.job != job) {
        cleanup(job);
        return;
    }
    s.job = null;

    if (job.canceled.load(.acquire) or job.generation != s.generation) {
        cleanup(job);
        s.redraw();
        scheduleNext(s);
        return;
    }

    if (job.error_text.len > 0) {
        const n = @min(job.error_text.len, s.error_buf.len);
        @memcpy(s.error_buf[0..n], job.error_text[0..n]);
        s.error_text = s.error_buf[0..n];
        if (job.engine_dead) s.client = null;
        cleanup(job);
        // The tray promised live numbers a moment ago; the pass that just
        // failed is when that promise breaks.
        tray.update(s);
        s.redraw();
        scheduleNext(s);
        return;
    }

    commit(s, job);
    cleanup(job); // the snapshot arena was moved into the state
    // A `BERTH_ACT` verb runs after the commit, through the same path a click
    // uses, with a fresh list to resolve the name against.
    if (s.pending_act.len > 0) {
        const spec = s.pending_act;
        s.pending_act = "";
        s.pending_act_saved = spec;
        if (pendingAct(s)) return; // it started its own refresh
    }
    scheduleNext(s);
    s.redraw();
}

fn cleanup(job: *Job) void {
    job.snapshot.deinit();
    job.gpa.free(job.socket);
    job.gpa.destroy(job);
}

/// A click that asked for a refresh during this pass gets one immediately;
/// otherwise the next pass is five seconds after this one finished.
fn scheduleNext(s: *app.State) void {
    if (s.select_wanted) |index| {
        s.select_wanted = null;
        s.refresh_wanted = false;
        s.selectRuntime(index);
        return;
    }
    if (s.discover_wanted) {
        s.discover_wanted = false;
        s.refresh_wanted = false;
        s.discover();
        return;
    }
    if (s.refresh_wanted and s.client != null) {
        s.refresh_wanted = false;
        s.refresh();
        return;
    }
    s.refresh_wanted = false;
    if (s.auto_refresh) nanoui.App.after(5000, s, autoTick);
}

/// Stop a job at shutdown: the worker may be inside a socket read, so join it
/// before the state and its arena disappear.
pub fn discard(job: *Job) void {
    job.canceled.store(true, .release);
    if (job.thread) |thread| thread.join();
    cleanup(job);
}

fn commit(s: *app.State, job: *Job) void {
    const snap = &job.snapshot;
    // Decide the df before the old arena dies. A df may survive exactly one
    // rotation (pointing into the then-retired arena); a second timed-out pass
    // must drop it before freeing that arena, or the views read freed memory.
    var next_df: ?api.Df = null;
    if (snap.df) |df| {
        next_df = df;
        s.df_stale = false;
    } else if (s.df != null and !s.df_stale) {
        next_df = s.df;
        s.df_stale = true;
    } else {
        s.df_stale = false;
    }

    if (s.retired) |*old| old.deinit();
    s.retired = s.arena;
    s.arena = snap.arena;
    snap.arena = .init(job.gpa); // the arena now belongs to the state

    s.containers = snap.containers;
    // Once per snapshot, not once per frame: a checkbox tick that survives
    // the refresh must still name a row the engine lists.
    s.bulk.retain(s.containers);
    s.stats = snap.stats;
    s.images = snap.images;
    s.volumes = snap.volumes;
    s.networks = snap.networks;
    s.df = next_df;
    // A failed `ps` leaves the last known figure rather than flashing zero.
    if (snap.engine_rss) |rss| s.engine_rss = rss;
    // `/volumes` does not carry size; the df does. Use the last good df even
    // when this pass timed out, or every volume row loses its size again.
    if (s.df) |df| volumes_mod.mergeVolumeUsage(s.volumes, df);
    // Engine version/info come from discovery and stay in runtime memory;
    // copying them out of this arena on every pass is how a failed /info used
    // to leave a string pointing into the retired snapshot.
    if (s.client) |*client| client.alloc = s.arena.allocator();

    resolveSelection(s);
    events_feed.commit(s, snap.events_since, snap.events_until, snap.events);
    logs.commitSnapshot(s, snap.logs, snap.logs_id, snap.logs_timestamps);
    s.error_text = "";
    s.last_update = format.now();
    s.sample();
    tray.update(s);
    if (s.pending_image != .none) {
        const pending = s.pending_image;
        s.pending_image = .none;
        switch (pending) {
            .none => {},
            .pull => image_tasks.startPull(s),
            .run => image_tasks.startRun(s),
        }
    }
}

fn resolveSelection(s: *app.State) void {
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
            logs.clear(s);
            s.clearInspect();
        }
    }
    if (s.pending_select.len > 0) selectPending(s);
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

/// Resolves the container named in `pending_act` and runs the verb against it
/// through the same path the buttons use. `BERTH_ACT=pause:web` is how the
/// action paths are exercised without a mouse.
pub fn pendingAct(s: *app.State) bool {
    const spec = s.pending_act_saved;
    const colon = std.mem.indexOfScalar(u8, spec, ':') orelse return false;
    const verb = spec[0..colon];
    const name = spec[colon + 1 ..];
    for (s.containers) |ct| {
        if (!std.mem.eql(u8, ct.name(), name)) continue;
        var buf: [256]u8 = undefined;
        const Word = enum { stop, start, pause, unpause, resume_, restart, delete };
        const word_raw = if (std.mem.eql(u8, verb, "resume")) "resume_" else verb;
        const word = std.meta.stringToEnum(Word, word_raw) orelse return false;
        const method: []const u8 = if (word == .delete) "DELETE" else "POST";
        const toast: []const u8 = switch (word) {
            .stop => "Stopped",
            .start => "Started",
            .pause => "Paused",
            .unpause, .resume_ => "Resumed",
            .restart => "Restarted",
            .delete => "Deleted",
        };
        // Each branch has a comptime-known format, which bufPrint requires.
        const path: []const u8 = switch (word) {
            .stop => std.fmt.bufPrint(&buf, "/containers/{s}/stop", .{ct.Id}) catch return false,
            .start => std.fmt.bufPrint(&buf, "/containers/{s}/start", .{ct.Id}) catch return false,
            .pause => std.fmt.bufPrint(&buf, "/containers/{s}/pause", .{ct.Id}) catch return false,
            .unpause, .resume_ => std.fmt.bufPrint(&buf, "/containers/{s}/unpause", .{ct.Id}) catch return false,
            .restart => std.fmt.bufPrint(&buf, "/containers/{s}/restart", .{ct.Id}) catch return false,
            .delete => std.fmt.bufPrint(&buf, "/containers/{s}?force=1", .{ct.Id}) catch return false,
        };
        return s.act(method, path, toast);
    }
    return false;
}

/// One refresh, then one timer: scheduled from the end of a pass, never on top
/// of one that is still running.
pub fn autoTick(ctx: ?*anyopaque) callconv(.c) void {
    const s: *app.State = @ptrCast(@alignCast(ctx.?));
    if (s.job != null) return;
    if (s.client == null) {
        s.discover();
        return;
    }
    s.refresh();
}
