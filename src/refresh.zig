//! A refresh is a worker thread, not a chain of callbacks on the UI thread.
//! The worker builds a complete snapshot over its own socket and arena, then
//! posts one `done` callback to the main queue. The window never waits on an
//! engine request; it keeps painting the previous snapshot until the swap.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const discovery = @import("engine/discovery.zig");
const format = @import("ui/format.zig");
const stats_pool = @import("stats_pool.zig");
const Snapshot = @import("snapshot.zig").Snapshot;
const Stat = stats_pool.Sample;

pub const Job = struct {
    state: *app.State,
    io: std.Io,
    gpa: std.mem.Allocator,
    socket: []u8,
    generation: u64,
    snapshot: Snapshot,
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
    };
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
    job.snapshot.version = version;

    job.snapshot.info = client.info() catch {
        job.error_text = if (client.err_body.len > 0) client.err_body else "could not read engine info";
        return;
    };
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
    job.snapshot.df = client.df() catch null;
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
        if (s.runPendingAct()) return; // it started its own refresh
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
    if (s.retired) |*old| old.deinit();
    s.retired = s.arena;
    s.arena = snap.arena;
    snap.arena = .init(job.gpa); // the arena now belongs to the state

    s.containers = snap.containers;
    s.stats = snap.stats;
    s.images = snap.images;
    s.volumes = snap.volumes;
    s.networks = snap.networks;
    // A timed-out df must not erase figures that were valid a moment ago.
    if (snap.df) |df| s.df = df;
    if (s.activeRuntime()) |rt| {
        rt.version = snap.version;
        rt.info = snap.info;
    }
    if (s.client) |*client| client.alloc = s.arena.allocator();

    resolveSelection(s);
    s.error_text = "";
    s.last_update = format.now();
    s.sample();
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
            s.clearLogs();
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
