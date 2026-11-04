//! Engine mutations as background jobs. Lifecycle actions, volume/network
//! creation and cleanup all touch the engine the same way, so they share one
//! worker shape: copy the address and inputs, run with a bounded client, then
//! report once on the main thread. Only one mutation runs at a time; a second
//! click is refused rather than queued against a target that may have changed.
const std = @import("std");
const nanoui = @import("nanoui");
const app = @import("app.zig");
const api = @import("engine/api.zig");
const create_ops = @import("engine/create.zig");
const prune = @import("engine/prune.zig");
const sheets = @import("sheets.zig");
const session = @import("session.zig");
const format = @import("ui/format.zig");

pub const max_steps = 5;

/// What a cleanup run may remove; volumes and networks are opt-in.
pub const PruneOptions = struct {
    containers: bool = true,
    images: bool = true,
    volumes: bool = false,
    networks: bool = false,
    build_cache: bool = true,
};

pub const Kind = enum { action, create, prune };

/// Kept separate from `create.zig`'s dialog enum so the job does not depend on
/// the view that opened it.
pub const CreateKind = enum { volume, network };

/// One prune step's result, so a skip can stay a skip instead of becoming a
/// reported failure.
pub const StepOutcome = enum { done, skipped, failed };

/// A destructive action waiting on its confirmation.
pub const Pending = struct {
    id: []const u8,
    name: []const u8,
    method: []const u8,
    path: []const u8,
    verb: []const u8,
};

pub const State = struct {
    job: ?*Job = null,
    /// Tests have no main queue to run completions; they join the worker and
    /// call `finish` themselves. Production always dispatches.
    dispatch: bool = true,

    pub fn running(self: *const State) bool {
        return self.job != null;
    }
};

pub const Job = struct {
    state: *app.State,
    gpa: std.mem.Allocator,
    io: std.Io,
    kind: Kind,
    /// Copied before the spawn: the worker must never read a mutable State
    /// field, and the source slices may point into an arena that rotates.
    socket: [512]u8 = undefined,
    socket_len: usize = 0,
    api_version: [16]u8 = undefined,
    api_len: usize = 0,
    method: [8]u8 = undefined,
    method_len: usize = 0,
    path: [320]u8 = undefined,
    path_len: usize = 0,
    verb: [32]u8 = undefined,
    verb_len: usize = 0,
    create_kind: CreateKind = .volume,
    /// The dialog instance that submitted this create; a newer form with the
    /// same kind and name must keep its input when this job reports.
    form_generation: u64 = 0,
    name: [128]u8 = undefined,
    name_len: usize = 0,
    engine: prune.Engine = .docker,
    steps: [max_steps]prune.Kind = undefined,
    step_count: usize = 0,
    outcomes: [max_steps]StepOutcome = @splat(.skipped),
    ok: bool = false,
    /// Page-owned when non-empty: the worker copies the engine's words before
    /// its arena dies, and the completion frees them.
    error_text: []const u8 = "",
    reclaimed: u64 = 0,
    failed: bool = false,
    skipped_build_cache: bool = false,
    thread: ?std.Thread = null,
    dispatch: bool = true,
    /// Set only by teardown: the worker must not post a callback once the
    /// state is about to disappear.
    abandoned: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

/// One cleanup run's chosen steps, in the order they must execute.
pub const Plan = struct {
    steps: [max_steps]prune.Kind = undefined,
    count: usize = 0,
};

/// Containers first: removing a stopped container can make its image and
/// volumes unused for the prunes that follow in the same pass. Unsupported
/// routes are left out of the plan; the worker still records a late 404 as a
/// skip.
pub fn planPrune(opts: PruneOptions, engine: prune.Engine) Plan {
    var plan: Plan = .{};
    const candidates = [_]struct { on: bool, kind: prune.Kind }{
        .{ .on = opts.containers, .kind = .containers },
        .{ .on = opts.images, .kind = .images },
        .{ .on = opts.volumes, .kind = .volumes },
        .{ .on = opts.networks, .kind = .networks },
        .{ .on = opts.build_cache, .kind = .build_cache },
    };
    for (candidates) |candidate| {
        if (!candidate.on) continue;
        if (!prune.supported(candidate.kind, engine)) continue;
        plan.steps[plan.count] = candidate.kind;
        plan.count += 1;
    }
    return plan;
}

fn busy(s: *app.State) bool {
    if (!s.mutating()) return false;
    s.notify("wait for the current engine operation to finish", true);
    return true;
}

/// Whether the action in flight is a cleanup run; the prune sheet names it in
/// its refusal instead of using the generic engine-operation wording.
pub fn pruning(s: *const app.State) bool {
    const job = s.actions.job orelse return false;
    return job.kind == .prune;
}

/// Runs one action against the engine. Returns whether the job started, so the
/// refresh harness can tell whether its own refresh will happen.
pub fn act(s: *app.State, method: []const u8, path: []const u8, verb: []const u8) bool {
    if (busy(s)) return false;
    const job = make(s, .action) orelse return false;
    if (!copy(&job.method, &job.method_len, method) or
        !copy(&job.path, &job.path_len, path) or
        !copy(&job.verb, &job.verb_len, verb))
    {
        s.gpa.destroy(job);
        s.notify("that action's request is too long", true);
        return false;
    }
    return start(s, job);
}

/// `act` with the path formatted at the call site: a literal `"{s}"` passed
/// through `act` would reach the engine unexpanded.
pub fn actFmt(s: *app.State, method: []const u8, comptime path_fmt: []const u8, id: []const u8, verb: []const u8) bool {
    var buf: [160]u8 = undefined;
    const path = std.fmt.bufPrint(&buf, path_fmt, .{id}) catch return false;
    return act(s, method, path, verb);
}

/// Creates one volume or network. The dialog has already validated the name;
/// this only copies it where the worker can read it after the dialog changes.
pub fn create(s: *app.State, kind: CreateKind, name: []const u8) bool {
    if (busy(s)) return false;
    const job = make(s, .create) orelse return false;
    job.create_kind = kind;
    job.form_generation = s.create_dialog.generation;
    if (!copy(&job.name, &job.name_len, name)) {
        s.gpa.destroy(job);
        s.notify("that name is too long", true);
        return false;
    }
    return start(s, job);
}

/// The cleanup sheet's run: one worker over the selected steps, in order.
pub fn runPrune(s: *app.State) bool {
    if (busy(s)) return false;
    const engine = engineFor(s);
    const plan = planPrune(s.prune_opts, engine);
    const job = make(s, .prune) orelse return false;
    job.engine = engine;
    job.step_count = plan.count;
    @memcpy(job.steps[0..plan.count], plan.steps[0..plan.count]);
    if (!start(s, job)) return false;
    // The choices are frozen in the job; close the sheet so a second click
    // cannot look like it changed the run. A failed start keeps it open.
    s.prune_open = false;
    sheets.resetPruneOpts(s);
    s.redraw();
    return true;
}

fn engineFor(s: *app.State) prune.Engine {
    if (s.activeRuntime()) |rt| return switch (rt.dialect) {
        .podman => .podman,
        .docker => .docker,
    };
    return .docker;
}

fn make(s: *app.State, kind: Kind) ?*Job {
    const job = s.gpa.create(Job) catch {
        s.notify("could not start the action", true);
        return null;
    };
    job.* = .{ .state = s, .gpa = s.gpa, .io = s.io, .kind = kind };
    return job;
}

fn copy(target: []u8, len: *usize, text: []const u8) bool {
    if (text.len > target.len) return false;
    @memcpy(target[0..text.len], text);
    len.* = text.len;
    return true;
}

fn start(s: *app.State, job: *Job) bool {
    if (s.client == null) {
        s.gpa.destroy(job);
        s.notify("no engine is selected", true);
        return false;
    }
    const client = &s.client.?;
    // Truncating an address would silently target a different socket.
    if (client.socket.len > job.socket.len or client.api.len > job.api_version.len) {
        s.gpa.destroy(job);
        s.notify("the engine address is too long", true);
        return false;
    }
    @memcpy(job.socket[0..client.socket.len], client.socket);
    job.socket_len = client.socket.len;
    @memcpy(job.api_version[0..client.api.len], client.api);
    job.api_len = client.api.len;
    job.dispatch = s.actions.dispatch;

    job.thread = std.Thread.spawn(.{}, work, .{job}) catch {
        s.gpa.destroy(job);
        s.notify("could not start the action", true);
        return false;
    };
    s.actions.job = job;
    s.redraw();
    return true;
}

fn work(job: *Job) void {
    run(job);
    if (!job.dispatch) return; // the test joins and calls finish itself
    if (job.abandoned.load(.acquire)) return; // teardown owns the leak
    nanoui.App.after(0, job, finishBridge);
}

/// The worker body: one job's engine calls, without dispatch. Public so tests
/// can drive it against a mock engine without a main queue.
pub fn run(job: *Job) void {
    const page = std.heap.page_allocator;
    var arena = std.heap.ArenaAllocator.init(page);
    defer arena.deinit();
    var client = api.Client.init(job.io, page, job.socket[0..job.socket_len]);
    client.api = job.api_version[0..job.api_len];
    client.alloc = arena.allocator();
    // Every request keeps the client's 5 s deadline. Prune steps run one at a
    // time, so a hung step spends its own budget instead of the whole run's.

    switch (job.kind) {
        .action => runAction(job, &client, page),
        .create => runCreate(job, &client, page),
        .prune => runPruneSteps(job, &client),
    }
}

fn runAction(job: *Job, client: *api.Client, page: std.mem.Allocator) void {
    client.action(job.method[0..job.method_len], job.path[0..job.path_len]) catch {
        // The engine usually names the problem ("cannot pause a stopped
        // container"); the completion shows its words rather than a generic
        // failure when it has them.
        job.error_text = page.dupe(u8, engineMessage(client.err_body, "action failed")) catch "";
        return;
    };
    job.ok = true;
}

fn runCreate(job: *Job, client: *api.Client, page: std.mem.Allocator) void {
    const name = job.name[0..job.name_len];
    const result = switch (job.create_kind) {
        .volume => create_ops.volume(client, name),
        .network => create_ops.network(client, name),
    };
    result catch {
        job.error_text = page.dupe(u8, engineMessage(client.err_body, "the engine did not answer")) catch "";
        return;
    };
    job.ok = true;
}

fn runPruneSteps(job: *Job, client: *api.Client) void {
    for (job.steps[0..job.step_count], 0..) |kind, i| {
        var freed: u64 = 0;
        prune.runInto(client, kind, job.engine, &freed) catch |e| {
            // Keep the bytes a multi-pass step freed before it failed.
            prune.addReclaimed(&job.reclaimed, freed);
            if (e == error.EngineUnsupported) {
                job.outcomes[i] = .skipped;
                if (kind == .build_cache) job.skipped_build_cache = true;
            } else {
                job.outcomes[i] = .failed;
                job.failed = true;
            }
            continue;
        };
        prune.addReclaimed(&job.reclaimed, freed);
        job.outcomes[i] = .done;
    }
}

/// Transport failures (socket gone, timeout) leave `err_body` empty; an empty
/// message must not read as success anywhere downstream.
fn engineMessage(body: []const u8, fallback: []const u8) []const u8 {
    return if (body.len > 0) body else fallback;
}

fn finishBridge(ctx: ?*anyopaque) callconv(.c) void {
    const job: *Job = @ptrCast(@alignCast(ctx.?));
    finish(job);
}

/// The completion callback's body, public for the same reason as `run`.
/// Joins and frees the job. The abandoned check keeps a callback queued before
/// teardown from touching a state that is about to disappear; that job is
/// deliberately leaked because its worker may still be inside a socket read.
pub fn finish(job: *Job) void {
    if (job.abandoned.load(.acquire)) return;
    if (job.thread) |thread| thread.join();

    const s = job.state;
    const current = s.actions.job == job;
    if (!current) {
        release(job);
        return;
    }
    s.actions.job = null;
    report(s, job);
    release(job);
    // Mutation completions always want a fresh snapshot. The disk figures are
    // part of that: pruning or creating a volume changes what the cached
    // `/system/df` would claim.
    s.df.invalidate();
    // A queued switch or rescan supersedes the refresh (both clear
    // refresh_wanted), and if another mutation is still running its finish
    // will resume instead.
    s.refresh_wanted = true;
    session.@"resume"(s);
    s.redraw();
}

fn report(s: *app.State, job: *const Job) void {
    switch (job.kind) {
        .action => if (job.ok)
            // Copy through the toast buffer: the verb lives in the job, which
            // is freed right after this returns.
            s.notifyFmt("{s}", .{job.verb[0..job.verb_len]}, false)
        else
            s.notifyFmt("{s}", .{engineMessage(job.error_text, "action failed")}, true),
        .create => if (job.ok) {
            // The input is kept until the engine answers, so a failure leaves
            // the dialog exactly as the user left it. Close only the form that
            // started this job; a dialog opened elsewhere meanwhile stays.
            if (sameCreateDialog(s, job)) s.create_dialog.open = null;
            s.notifyFmt("{s} {s} created", .{
                if (job.create_kind == .volume) "Volume" else "Network",
                job.name[0..job.name_len],
            }, false);
        } else {
            s.notifyFmt("could not create the {s}: {s}", .{
                createNoun(job.create_kind),
                engineMessage(job.error_text, "the engine did not answer"),
            }, true);
        },
        .prune => {
            var bytes_buf: [32]u8 = undefined;
            const freed_text = format.bytesBuf(&bytes_buf, job.reclaimed);
            if (job.failed) {
                s.notifyFmt("Cleanup freed {s}, with errors", .{freed_text}, true);
            } else if (job.skipped_build_cache) {
                s.notifyFmt("Freed {s} · build cache skipped", .{freed_text}, false);
            } else {
                s.notifyFmt("Freed {s}", .{freed_text}, false);
            }
        },
    }
}

fn sameCreateDialog(s: *app.State, job: *const Job) bool {
    if (s.create_dialog.open == null) return false;
    return s.create_dialog.generation == job.form_generation;
}

fn createNoun(kind: CreateKind) []const u8 {
    return switch (kind) {
        .volume => "volume",
        .network => "network",
    };
}

fn release(job: *Job) void {
    if (job.error_text.len > 0) std.heap.page_allocator.free(job.error_text);
    job.gpa.destroy(job);
}

/// Teardown: never wait for a worker that may be blocked on a socket. The job
/// is deliberately leaked; `abandoned` keeps any queued callback away from the
/// state that is about to disappear.
pub fn discard(s: *app.State) void {
    const job = s.actions.job orelse return;
    job.abandoned.store(true, .release);
    if (job.thread) |thread| thread.detach();
    s.actions.job = null;
}

pub fn confirm(s: *app.State, id: []const u8, name: []const u8, verb: []const u8, method: []const u8, comptime path_fmt: []const u8) void {
    const path = std.fmt.allocPrint(s.gpa, path_fmt, .{id}) catch return;
    const id_copy = s.gpa.dupe(u8, id) catch {
        s.gpa.free(path);
        return;
    };
    const name_copy = s.gpa.dupe(u8, name) catch {
        s.gpa.free(path);
        s.gpa.free(id_copy);
        return;
    };
    // A confirmation owns the modal slot: a prune sheet or bulk question
    // under it would describe a different set of containers.
    sheets.dismiss(s);
    s.pending = .{
        .id = id_copy,
        .name = name_copy,
        .method = method,
        .path = path,
        .verb = verb,
    };
    s.redraw();
}

pub fn clearPending(s: *app.State) void {
    const p = s.pending orelse return;
    s.pending = null;
    s.gpa.free(p.id);
    s.gpa.free(p.name);
    s.gpa.free(p.path);
}

pub fn resolvePending(s: *app.State, ok: bool) void {
    const p = s.pending orelse return;
    // Do not discard the question when the action could not start. The user
    // can retry after the busy operation or the missing engine is resolved.
    if (ok and !act(s, p.method, p.path, p.verb)) return;
    s.pending = null;
    if (!ok) s.redraw();
    s.gpa.free(p.id);
    s.gpa.free(p.name);
    s.gpa.free(p.path);
}
