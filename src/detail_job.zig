//! One selected object's inspector reads, off the UI thread. Inspect and the
//! logs endpoint can each spend seconds inside the engine, so selecting a row
//! used to freeze the frame until both answered. The worker owns copies of the
//! address and id, builds its own client and arena, and returns page-owned
//! text; the main thread applies it only while that selection is still on
//! screen.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const inspect = @import("engine/inspect.zig");
const lifetime = @import("engine/lifetime.zig");
const logs = @import("logs.zig");

/// What a detail read was asked to fetch. A selection wants the lifetime and
/// logs; the log Refresh and timestamps toggle want logs only; the Inspect tab
/// wants the raw document only.
pub const Job = struct {
    /// Only read by the completion, never by the worker: the worker must not
    /// touch State while the frame is painting from it.
    state: *app.State,
    gpa: std.mem.Allocator,
    io: std.Io,
    detail_kind: app.DetailKind = .container,
    inspect_kind: inspect.Kind = .container,
    /// Copied before the spawn: the source may live in an arena that rotates.
    socket: [512]u8 = undefined,
    socket_len: usize = 0,
    api_version: [16]u8 = undefined,
    api_len: usize = 0,
    id: [256]u8 = undefined,
    id_len: usize = 0,
    generation: u64 = 0,
    timestamps: bool = false,
    fetch_lifetime: bool = false,
    fetch_logs: bool = false,
    fetch_inspect: bool = false,
    /// Page-owned, like the text below. Moved into the state on apply.
    lifetime_ok: bool = false,
    lifetime: lifetime.Lifetime = .{},
    log_ok: bool = false,
    log_text: []const u8 = "",
    inspect_ok: bool = false,
    inspect_text: []const u8 = "",
    inspect_truncated: bool = false,
    inspect_error: []const u8 = "",
    thread: ?std.Thread = null,
    /// Tests join the worker and call `finish` themselves.
    dispatch: bool = true,
    /// Set when a newer selection or job supersedes this one; `finish` must
    /// not touch the state, only release the worker's copies.
    abandoned: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    /// Set only at teardown: the callback queue is going away, so the worker
    /// must not post into it. The deliberately leaked job bounds the quit.
    detached: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

fn copyFixed(target: []u8, len: *usize, text: []const u8) bool {
    if (text.len > target.len) return false;
    @memcpy(target[0..text.len], text);
    len.* = text.len;
    return true;
}

fn make(s: *app.State) ?*Job {
    const job = s.gpa.create(Job) catch return null;
    job.* = .{
        .state = s,
        .gpa = s.gpa,
        .io = s.io,
        .dispatch = s.detail_dispatch,
    };
    return job;
}

/// Fills the worker's private copies. The client's socket and API version are
/// what the request must use, even if another refresh changes them meanwhile.
fn fill(s: *app.State, job: *Job, id: []const u8) bool {
    const client = &(s.client orelse return false);
    return copyFixed(&job.socket, &job.socket_len, client.socket) and
        copyFixed(&job.api_version, &job.api_len, client.api) and
        copyFixed(&job.id, &job.id_len, id);
}

/// A new selection always replaces whatever detail read was in flight.
pub fn startContainer(s: *app.State, id: []const u8) void {
    const job = make(s) orelse {
        s.lifetime_loading = false;
        s.logs_loading = false;
        return;
    };
    job.detail_kind = .container;
    job.inspect_kind = .container;
    job.fetch_lifetime = true;
    job.fetch_logs = true;
    job.timestamps = s.log_opts.timestamps;
    if (!fill(s, job, id)) {
        s.gpa.destroy(job);
        s.lifetime_loading = false;
        s.logs_loading = false;
        return;
    }
    _ = begin(s, job, true);
}

/// The log pane's Refresh and timestamps toggle. A running read is refused
/// rather than layered over: it already owns the one job slot.
pub fn startLogs(s: *app.State) void {
    if (s.detail_kind != .container) return;
    const id = s.selected orelse return;
    if (s.detail_job != null) {
        // The running job was captured with the old clock. Remember the click
        // so its completion can start the read the toggle actually asked for.
        s.logs_wanted = true;
        return;
    }
    if (s.client == null) {
        // The synchronous path cleared before finding out there was no
        // engine; an old tail would otherwise look like the current one.
        logs.clear(s);
        s.logs_loading = false;
        s.logs_wanted = false;
        s.redraw();
        return;
    }
    const job = make(s) orelse return;
    job.detail_kind = .container;
    job.fetch_logs = true;
    job.timestamps = s.log_opts.timestamps;
    if (!fill(s, job, id)) {
        s.gpa.destroy(job);
        return;
    }
    if (!begin(s, job, false)) return;
    s.logs_wanted = false;
    // The old tail belongs to the old options; the pane says Reading… until
    // the new one lands instead of showing bytes in the wrong clock.
    logs.clear(s);
}

/// The Inspect tab's first open and its Refresh. The curated lifetime facts
/// and logs belong to the selection, so this replaces only the JSON.
pub fn startInspect(s: *app.State) void {
    const id = s.selected orelse return;
    if (s.detail_job != null) {
        // Opening the tab while the selection read owns the slot is not a
        // lost click: the completion retries it once with this flag.
        s.inspect_wanted = true;
        return;
    }
    if (s.client == null) {
        s.clearInspectJson();
        s.inspect_loading = false;
        s.inspect_wanted = false;
        s.redraw();
        return;
    }
    const job = make(s) orelse return;
    job.detail_kind = s.detail_kind;
    job.inspect_kind = switch (s.detail_kind) {
        .container => .container,
        .image => .image,
        .volume => .volume,
        .network => .network,
    };
    job.fetch_inspect = true;
    if (!fill(s, job, id)) {
        s.gpa.destroy(job);
        return;
    }
    if (!begin(s, job, false)) return;
    s.inspect_wanted = false;
    // The old document may describe a different revision of the object; the
    // pane shows Reading… until the fresh one arrives.
    s.clearInspectJson();
    s.inspect_loading = true;
    s.redraw();
}

/// Everything a selection owns is being replaced: invalidate any queued
/// completion before it can apply to the new row or page.
pub fn invalidate(s: *app.State) void {
    s.detail_generation +%= 1;
    s.lifetime_loading = false;
    s.logs_loading = false;
    s.inspect_loading = false;
    s.logs_wanted = false;
    s.inspect_wanted = false;
    abandon(s);
}

fn begin(s: *app.State, job: *Job, replace: bool) bool {
    if (s.detail_job != null) {
        if (!replace) {
            s.gpa.destroy(job);
            return false;
        }
        abandon(s);
    }
    // A new job token, not just a new selection: a logs refresh that started
    // later must win even when an older read for the same row finishes last.
    s.detail_generation +%= 1;
    job.generation = s.detail_generation;
    s.lifetime_loading = job.fetch_lifetime;
    s.logs_loading = job.fetch_logs;
    s.inspect_loading = job.fetch_inspect;
    job.thread = std.Thread.spawn(.{}, work, .{job}) catch {
        s.lifetime_loading = false;
        s.logs_loading = false;
        s.inspect_loading = false;
        s.logs_wanted = false;
        s.inspect_wanted = false;
        s.gpa.destroy(job);
        s.notify("could not start reading the selection", true);
        return false;
    };
    s.detail_job = job;
    s.redraw();
    return true;
}

/// Drop a superseded job without waiting for its socket read. It still owns
/// the job record, so its completion must only release those copies.
fn abandon(s: *app.State) void {
    const job = s.detail_job orelse return;
    s.detail_job = null;
    job.abandoned.store(true, .release);
    if (job.thread) |thread| {
        thread.detach();
        job.thread = null;
    }
}

fn work(job: *Job) void {
    run(job);
    if (!job.dispatch) return; // the test joins and calls `finish` itself
    if (job.detached.load(.acquire)) return; // teardown owns the leak
    nanoui.App.after(0, job, finishBridge);
}

/// The worker body, without dispatch. Public so tests can drive it against
/// the mock engine and join it deterministically.
pub fn run(job: *Job) void {
    const page = std.heap.page_allocator;
    // Parsed bodies live in this arena and die here; everything the main
    // thread keeps is copied to the page allocator first.
    var arena = std.heap.ArenaAllocator.init(page);
    defer arena.deinit();
    var client = api.Client.init(job.io, page, job.socket[0..job.socket_len]);
    client.api = job.api_version[0..job.api_len];
    client.alloc = arena.allocator();
    const id = job.id[0..job.id_len];

    if (job.fetch_lifetime) {
        if (inspect.fetch(&client, .container, id, page)) |raw| {
            defer page.free(raw);
            job.lifetime = lifetime.fromInspect(page, raw);
            job.lifetime_ok = true;
        } else |_| {}
    }

    if (job.fetch_logs) {
        if (client.logs(id, 500, job.timestamps)) |text| {
            const owned: ?[]u8 = if (job.timestamps)
                logs.localize(page, text)
            else
                page.dupe(u8, text) catch null;
            if (owned) |buf| {
                job.log_text = buf;
                job.log_ok = true;
            }
        } else |_| {}
    }

    if (job.fetch_inspect) {
        if (inspect.fetch(&client, job.inspect_kind, id, page)) |raw| {
            defer page.free(raw);
            if (inspect.pretty(page, raw, 256 * 1024)) |pretty| {
                job.inspect_text = pretty.text;
                job.inspect_truncated = pretty.truncated;
                job.inspect_ok = true;
            } else |_| {
                job.inspect_error = page.dupe(u8, "could not read the inspect JSON") catch "";
            }
        } else |_| {
            const text = if (client.err_body.len > 0) client.err_body else "could not read the inspect JSON";
            job.inspect_error = page.dupe(u8, text) catch "";
        }
    }
}

fn finishBridge(ctx: ?*anyopaque) callconv(.c) void {
    const job: *Job = @ptrCast(@alignCast(ctx.?));
    finish(job);
}

/// Applies one finished read on the main thread. Public for the same reason
/// `run` is: tests drive the completion without an event loop.
pub fn finish(job: *Job) void {
    if (job.abandoned.load(.acquire)) {
        release(job);
        return;
    }
    if (job.thread) |thread| {
        thread.join();
        job.thread = null;
    }

    const s = job.state;
    if (s.detail_job != job) {
        release(job);
        return;
    }
    s.detail_job = null;

    const same_row = s.detail_kind == job.detail_kind and
        s.selected != null and std.mem.eql(u8, s.selected.?, job.id[0..job.id_len]);
    if (!same_row or job.generation != s.detail_generation) {
        // No newer job owns the pane (the pointer matched above), so clear the
        // flags this read set; otherwise a dropped result would leave the
        // inspector spinning forever.
        s.lifetime_loading = false;
        s.logs_loading = false;
        s.inspect_loading = false;
        s.logs_wanted = false;
        s.inspect_wanted = false;
        release(job);
        return;
    }

    apply(s, job);

    // A Refresh or Times click during the read could not start a second job.
    // Consume the one-shot now, unless this job already fetched logs with the
    // clock the pane is currently asking for.
    const wants_logs = s.logs_wanted;
    s.logs_wanted = false;
    const log_result_is_current = job.fetch_logs and job.timestamps == s.log_opts.timestamps;
    release(job);
    s.redraw();
    if (wants_logs and !log_result_is_current) startLogs(s);

    // Opening Inspect during the selection read is retried once. If the log
    // retry just took the slot, the next completion picks this up instead.
    if (s.inspect_wanted and s.pane_inspect and !s.inspect_loading) {
        if (s.detail_job == null) {
            s.inspect_wanted = false;
            startInspect(s);
        }
    } else if (s.detail_job == null) {
        s.inspect_wanted = false;
    }
}

fn apply(s: *app.State, job: *Job) void {
    if (job.fetch_lifetime) {
        s.clearLifetime();
        if (job.lifetime_ok) {
            s.detail_lifetime = job.lifetime;
            job.lifetime = .{}; // moved into the state
        }
        s.lifetime_loading = false;
    }

    if (job.fetch_logs) {
        // A toggle that moved while this worker was reading makes the result
        // describe the wrong clock; leave the pane and let the one-shot retry.
        if (job.timestamps == s.log_opts.timestamps) {
            logs.clear(s);
            if (job.log_ok) {
                s.log_buf = if (job.log_text.len > 0) @constCast(job.log_text) else null;
                s.log_text = job.log_text;
                job.log_text = ""; // owned by the state now
            } else {
                // The same words the synchronous path used: an empty pane would
                // read as "this container has never logged anything".
                s.log_text = "could not read this container's logs";
            }
        }
        s.logs_loading = false;
    }

    if (job.fetch_inspect) {
        s.clearInspectJson();
        if (job.inspect_ok) {
            s.inspect_buf = if (job.inspect_text.len > 0) @constCast(job.inspect_text) else null;
            s.inspect_truncated = job.inspect_truncated;
            job.inspect_text = ""; // owned by the state now
        }
        s.inspect_loading = false;
        if (!job.inspect_ok) {
            const text = if (job.inspect_error.len > 0) job.inspect_error else "could not read the inspect JSON";
            // Copy through the toast buffer before the job's copy is freed.
            s.notifyFmt("{s}", .{text}, true);
        }
    }
}

fn release(job: *Job) void {
    const page = std.heap.page_allocator;
    if (job.lifetime.run_cmd) |cmd| page.free(cmd);
    if (job.log_text.len > 0) page.free(job.log_text);
    if (job.inspect_text.len > 0) page.free(job.inspect_text);
    if (job.inspect_error.len > 0) page.free(job.inspect_error);
    job.gpa.destroy(job);
}

/// Teardown: detach rather than join, because the worker may be inside a log
/// read with a 5 s deadline. The job leaks by design, like the other workers;
/// its completion can no longer touch the freed state.
pub fn cancel(s: *app.State) void {
    const job = s.detail_job orelse return;
    s.detail_job = null;
    job.abandoned.store(true, .release);
    job.detached.store(true, .release);
    if (job.thread) |thread| {
        thread.detach();
        job.thread = null;
    }
}
