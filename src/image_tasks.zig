//! Pulling and running images, as background jobs: the HTTP calls take
//! minutes on a real network, so they run off the UI thread like a refresh.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const api = @import("engine/api.zig");
const discovery = @import("engine/discovery.zig");
const image_ops = @import("engine/image_ops.zig");
const refs = @import("image_refs.zig");
const validation = @import("image_validation.zig");
const session = @import("session.zig");

pub const Pending = enum { none, pull, run };

pub const State = struct {
    pull_input: ui.TextBuffer = .{},
    pull_job: ?*PullJob = null,

    run_image: ui.TextBuffer = .{},
    run_name: ui.TextBuffer = .{},
    run_ports: ui.TextBuffer = .{},
    run_volumes: ui.TextBuffer = .{},
    run_env: ui.TextBuffer = .{},
    run_job: ?*RunJob = null,
    /// A failed submit leaves the message beside the field until the text
    /// changes; the rail is for completed actions, not unfinished forms.
    pull_attempted: bool = false,
    run_attempted: bool = false,

    pub fn deinit(self: *State) void {
        self.pull_input.deinit();
        self.run_image.deinit();
        self.run_name.deinit();
        self.run_ports.deinit();
        self.run_volumes.deinit();
        self.run_env.deinit();
    }

    pub fn pulling(self: *const State) bool {
        return self.pull_job != null;
    }

    pub fn running(self: *const State) bool {
        return self.run_job != null;
    }
};

pub fn startPull(s: *app.State) void {
    if (s.image_tasks.pulling()) return;
    const reference = s.image_tasks.pull_input.text();
    if (validation.pull(reference) != null) {
        s.image_tasks.pull_attempted = true;
        s.redraw();
        return;
    }
    var client = s.client orelse {
        s.notify("no engine is selected", true);
        return;
    };
    s.image_tasks.pull_attempted = false;
    const split = refs.splitReference(reference);

    const job = s.gpa.create(PullJob) catch {
        s.notify("could not start the pull", true);
        return;
    };
    job.* = .{ .io = s.io, .owner = s };
    copySocket(job, &client);
    job.repo_len = split.repo.len;
    @memcpy(job.repo[0..job.repo_len], split.repo);
    // A bare name pulls latest here, never "every tag of the repository".
    const tag = refs.effectiveTag(reference);
    job.tag_len = tag.len;
    @memcpy(job.tag[0..job.tag_len], tag);

    job.thread = std.Thread.spawn(.{}, pullWork, .{job}) catch {
        s.gpa.destroy(job);
        s.notify("could not start the pull", true);
        return;
    };
    s.image_tasks.pull_job = job;
    s.redraw();
}

pub fn prepareRun(s: *app.State, image: []const u8) void {
    s.image_tasks.run_image.set(image);
    // A prefill is only useful if the form is visible: open the card for it.
    s.creation_card_open = true;
    s.redraw();
}

pub fn startRun(s: *app.State) void {
    if (s.mutating()) {
        s.notify("wait for the current engine operation to finish", true);
        return;
    }
    const state = &s.image_tasks;
    const image = state.run_image.text();
    // The job carries fixed buffers: validate before copying so a truncated
    // request can never act on a different image.
    if (validation.run(image, state.run_name.text(), state.run_ports.text(), state.run_volumes.text(), state.run_env.text()) != null) {
        state.run_attempted = true;
        s.redraw();
        return;
    }
    var client = s.client orelse {
        s.notify("no engine is selected", true);
        return;
    };
    state.run_attempted = false;

    const job = s.gpa.create(RunJob) catch {
        s.notify("could not start the container", true);
        return;
    };
    job.* = .{ .io = s.io, .owner = s };
    copySocket(job, &client);
    copyFixed(&job.image, &job.image_len, image);
    copyFixed(&job.name, &job.name_len, state.run_name.text());
    copyFixed(&job.ports, &job.ports_len, state.run_ports.text());
    copyFixed(&job.volumes, &job.volumes_len, state.run_volumes.text());
    copyFixed(&job.env, &job.env_len, state.run_env.text());

    job.thread = std.Thread.spawn(.{}, runWork, .{job}) catch {
        s.gpa.destroy(job);
        s.notify("could not start the container", true);
        return;
    };
    s.image_tasks.run_job = job;
    s.redraw();
}

pub fn cancel(s: *app.State) void {
    // Detach, don't join: a pull against a dead network can block for the
    // read timeout, and quitting should not wait for it. The abandoned job
    // leaks by design — the same tradeoff engine/command.zig documents.
    if (s.image_tasks.pull_job) |job| {
        job.canceled.store(true, .release);
        if (job.thread) |thread| thread.detach();
        s.image_tasks.pull_job = null;
    }
    if (s.image_tasks.run_job) |job| {
        job.canceled.store(true, .release);
        if (job.thread) |thread| thread.detach();
        s.image_tasks.run_job = null;
    }
}

const PullJob = struct {
    io: std.Io,
    owner: *app.State = undefined,
    socket: [512]u8 = undefined,
    socket_len: usize = 0,
    api_version: [16]u8 = undefined,
    api_len: usize = 0,
    repo: [256]u8 = undefined,
    repo_len: usize = 0,
    tag: [128]u8 = undefined,
    tag_len: usize = 0,
    ok: bool = false,
    /// Page-owned (freed in pullDone) or empty: never a literal, which the
    /// free would turn into an invalid free.
    status: []const u8 = "",
    thread: ?std.Thread = null,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

fn pullWork(job: *PullJob) void {
    const page = std.heap.page_allocator;
    var arena = std.heap.ArenaAllocator.init(page);
    defer arena.deinit();
    var client = api.Client.init(job.io, page, job.socket[0..job.socket_len]);
    client.api = job.api_version[0..job.api_len];
    client.alloc = arena.allocator();

    const body = image_ops.pull(&client, job.repo[0..job.repo_len], job.tag[0..job.tag_len]) catch {
        job.status = page.dupe(u8, engineMessage(client.err_body, "engine did not answer")) catch "";
        postPull(job);
        return;
    };
    defer page.free(body);
    // The stream can report the failure itself on a 200 — "denied",
    // "manifest unknown" arrive as JSON lines with an error field.
    const result = lastStatus(arena.allocator(), body);
    job.ok = !result.failed;
    job.status = page.dupe(u8, result.text) catch "";
    postPull(job);
}

fn postPull(job: *PullJob) void {
    if (job.canceled.load(.acquire)) {
        if (job.status.len > 0) std.heap.page_allocator.free(job.status);
        return;
    }
    nanoui.App.after(0, job, pullDoneBridge);
}

fn pullDoneBridge(ctx: ?*anyopaque) callconv(.c) void {
    const job: *PullJob = @ptrCast(@alignCast(ctx.?));
    pullDone(job.owner, job);
}

const RunJob = struct {
    io: std.Io,
    owner: *app.State = undefined,
    socket: [512]u8 = undefined,
    socket_len: usize = 0,
    api_version: [16]u8 = undefined,
    api_len: usize = 0,
    image: [256]u8 = undefined,
    image_len: usize = 0,
    name: [64]u8 = undefined,
    name_len: usize = 0,
    ports: [512]u8 = undefined,
    ports_len: usize = 0,
    volumes: [512]u8 = undefined,
    volumes_len: usize = 0,
    env: [512]u8 = undefined,
    env_len: usize = 0,
    ok: bool = false,
    /// Page-owned (freed in runDone) or empty; an empty one with ok == false
    /// still means failure, worded by runDone.
    error_text: []const u8 = "",
    thread: ?std.Thread = null,
    canceled: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
};

fn runWork(job: *RunJob) void {
    const page = std.heap.page_allocator;
    var arena = std.heap.ArenaAllocator.init(page);
    defer arena.deinit();
    var client = api.Client.init(job.io, page, job.socket[0..job.socket_len]);
    client.api = job.api_version[0..job.api_len];
    client.alloc = arena.allocator();

    const body = buildBody(arena.allocator(), job) catch {
        job.error_text = page.dupe(u8, "could not build the container request") catch "";
        postRun(job);
        return;
    };
    const created = image_ops.create(&client, job.name[0..job.name_len], body) catch {
        job.error_text = page.dupe(u8, engineMessage(client.err_body, "could not create the container")) catch "";
        postRun(job);
        return;
    };
    image_ops.start(&client, created.Id) catch {
        job.error_text = page.dupe(u8, engineMessage(client.err_body, "could not start the container")) catch "";
        postRun(job);
        return;
    };
    job.ok = true;
    postRun(job);
}

/// Transport failures (socket gone, timeout) leave err_body empty; an empty
/// message must not read as success anywhere downstream.
fn engineMessage(body: []const u8, fallback: []const u8) []const u8 {
    return if (body.len > 0) body else fallback;
}

fn postRun(job: *RunJob) void {
    if (job.canceled.load(.acquire)) {
        if (job.error_text.len > 0) std.heap.page_allocator.free(job.error_text);
        return;
    }
    nanoui.App.after(0, job, runDoneBridge);
}

fn runDoneBridge(ctx: ?*anyopaque) callconv(.c) void {
    const job: *RunJob = @ptrCast(@alignCast(ctx.?));
    runDone(job.owner, job);
}

fn pullDone(s: *app.State, job: *PullJob) void {
    // A cancel that raced this callback already detached the thread and
    // abandoned the job; touching either here would be use-after-free.
    if (job.canceled.load(.acquire)) return;
    if (job.thread) |thread| thread.join();
    s.image_tasks.pull_job = null;
    const status = s.gpa.dupe(u8, job.status) catch "";
    if (job.status.len > 0) std.heap.page_allocator.free(job.status);
    const ok = job.ok;
    s.gpa.destroy(job);
    if (ok) {
        s.notifyFmt("{s}", .{refs.firstLine(if (status.len > 0) status else "pull finished")}, false);
    } else {
        s.notifyFmt("pull failed: {s}", .{refs.firstLine(if (status.len > 0) status else "engine did not answer")}, true);
    }
    if (status.len > 0) s.gpa.free(status);
    s.df.invalidate();
    s.refresh();
    s.redraw();
}

fn runDone(s: *app.State, job: *RunJob) void {
    // Same race as pullDone: cancel may have abandoned this job first.
    if (job.canceled.load(.acquire)) return;
    if (job.thread) |thread| thread.join();
    s.image_tasks.run_job = null;
    const error_text = s.gpa.dupe(u8, job.error_text) catch "";
    if (job.error_text.len > 0) std.heap.page_allocator.free(job.error_text);
    const ok = job.ok;
    s.gpa.destroy(job);
    if (ok) {
        s.notify("Container started", false);
    } else {
        s.notifyFmt("{s}", .{refs.firstLine(if (error_text.len > 0) error_text else "could not start the container")}, true);
    }
    if (error_text.len > 0) s.gpa.free(error_text);
    // Run is a mutation: refresh the new row, unless a queued switch or rescan
    // supersedes it. Its disk footprint is new too, so the df cache is stale.
    s.df.invalidate();
    s.refresh_wanted = true;
    session.@"resume"(s);
    s.redraw();
}

fn copySocket(job: anytype, client: *api.Client) void {
    job.socket_len = @min(client.socket.len, job.socket.len);
    @memcpy(job.socket[0..job.socket_len], client.socket[0..job.socket_len]);
    job.api_len = @min(client.api.len, job.api_version.len);
    @memcpy(job.api_version[0..job.api_len], client.api[0..job.api_len]);
}

fn copyFixed(target: anytype, len: *usize, text: []const u8) void {
    len.* = @min(text.len, target.len);
    @memcpy(target[0..len.*], text[0..len.*]);
}

fn buildBody(arena: std.mem.Allocator, job: *const RunJob) ![]const u8 {
    var out: std.Io.Writer.Allocating = .init(arena);
    var json: std.json.Stringify = .{ .writer = &out.writer };

    try json.beginObject();
    try json.objectField("Image");
    try json.write(job.image[0..job.image_len]);

    const env = job.env[0..job.env_len];
    if (env.len > 0) {
        try json.objectField("Env");
        try json.beginArray();
        var parts = std.mem.splitScalar(u8, env, ',');
        while (parts.next()) |raw| {
            const part = std.mem.trim(u8, raw, " ");
            if (part.len == 0) continue;
            try json.write(part);
        }
        try json.endArray();
    }

    const ports = job.ports[0..job.ports_len];
    const volumes = job.volumes[0..job.volumes_len];
    if (ports.len > 0 or volumes.len > 0) {
        try json.objectField("HostConfig");
        try json.beginObject();
        if (volumes.len > 0) {
            try json.objectField("Binds");
            try json.beginArray();
            var parts = std.mem.splitScalar(u8, volumes, ',');
            while (parts.next()) |raw| {
                const part = std.mem.trim(u8, raw, " ");
                if (part.len == 0) continue;
                try json.write(part);
            }
            try json.endArray();
        }
        if (ports.len > 0) {
            try json.objectField("PortBindings");
            try json.beginObject();
            var parts = std.mem.splitScalar(u8, ports, ',');
            while (parts.next()) |raw| {
                const part = std.mem.trim(u8, raw, " ");
                if (part.len == 0) continue;
                const binding = refs.parsePort(part) orelse continue;
                var key_buf: [48]u8 = undefined;
                var container = binding.container;
                if (std.mem.indexOfScalar(u8, container, '/') == null) {
                    container = std.fmt.bufPrint(&key_buf, "{s}/tcp", .{container}) catch container;
                }
                try json.objectField(container);
                try json.beginArray();
                try json.beginObject();
                if (binding.host_ip.len > 0) {
                    try json.objectField("HostIp");
                    try json.write(binding.host_ip);
                }
                try json.objectField("HostPort");
                try json.write(binding.host_port);
                try json.endObject();
                try json.endArray();
            }
            try json.endObject();
        }
        try json.endObject();
    }
    try json.endObject();
    return out.written();
}

const StreamStatus = struct { text: []const u8 = "", failed: bool = false };

/// What the pull stream ended on: a failing line wins over any progress that
/// came before it. Parsed into `alloc`, an arena the caller controls.
fn lastStatus(alloc: std.mem.Allocator, body: []const u8) StreamStatus {
    var last: StreamStatus = .{};
    var lines = std.mem.splitScalar(u8, body, '\n');
    while (lines.next()) |raw| {
        const line = std.mem.trim(u8, raw, " \r");
        if (line.len == 0) continue;
        const Doc = struct {
            status: []const u8 = "",
            @"error": []const u8 = "",
        };
        const parsed = std.json.parseFromSliceLeaky(Doc, alloc, line, .{
            .ignore_unknown_fields = true,
        }) catch continue;
        if (parsed.@"error".len > 0) return .{ .text = parsed.@"error", .failed = true };
        if (parsed.status.len > 0) last = .{ .text = parsed.status };
    }
    return last;
}

fn jobWith(image: []const u8, name: []const u8, ports: []const u8, volumes: []const u8, env: []const u8) RunJob {
    var job = RunJob{ .io = undefined };
    copyFixed(&job.image, &job.image_len, image);
    copyFixed(&job.name, &job.name_len, name);
    copyFixed(&job.ports, &job.ports_len, ports);
    copyFixed(&job.volumes, &job.volumes_len, volumes);
    copyFixed(&job.env, &job.env_len, env);
    return job;
}

test "an image run completion refreshes with nothing queued" {
    const Hook = struct {
        var starts: usize = 0;
        fn refresh(_: *app.State) void {
            starts += 1;
        }
        fn scan(gpa: std.mem.Allocator, io: std.Io, preferred: []const u8) !discovery.Found {
            _ = io;
            _ = preferred;
            return .{ .runtimes = try gpa.alloc(discovery.Runtime, 0) };
        }
    };
    Hook.starts = 0;
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .auto_refresh = false,
        .discovery_test = .{ .scan = Hook.scan, .start_refresh = Hook.refresh },
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/run-refresh.sock");
    state.df.noteAttempt();

    const job = try state.gpa.create(RunJob);
    job.* = .{ .io = state.io, .owner = &state, .ok = true };
    state.image_tasks.run_job = job;
    runDone(&state, job);

    try std.testing.expect(state.image_tasks.run_job == null);
    try std.testing.expect(!state.refresh_wanted);
    try std.testing.expect(state.df.wanted());
    try std.testing.expectEqual(@as(usize, 1), Hook.starts);
}

test "the create body says only what was asked for" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    var bare = jobWith("nginx:alpine", "", "", "", "");
    try std.testing.expectEqualStrings("{\"Image\":\"nginx:alpine\"}", try buildBody(arena.allocator(), &bare));

    var full = jobWith(
        "nginx:alpine",
        "web",
        "18082:80, 127.0.0.1:8081:81/udp",
        "data:/data",
        "A=1, B=2",
    );
    const body = try buildBody(arena.allocator(), &full);
    // The engine keys PortBindings by container port, defaulting to tcp, and
    // an address-bound entry needs its HostIp.
    try std.testing.expect(std.mem.indexOf(u8, body, "\"80/tcp\":[{\"HostPort\":\"18082\"}]") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"81/udp\":[{\"HostIp\":\"127.0.0.1\",\"HostPort\":\"8081\"}]") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"Binds\":[\"data:/data\"]") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"Env\":[\"A=1\",\"B=2\"]") != null);
    // The name travels in the query string, not the body.
    try std.testing.expect(std.mem.indexOf(u8, body, "web") == null);
}
