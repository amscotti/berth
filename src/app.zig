//! The application: one state, one refresh machine, one dispatch to the views.
//!
//! Refresh runs as a chain of one-shot callbacks rather than a thread: each
//! turn performs exactly one request and reschedules itself with
//! `App.after(0, …)`, so the window paints between requests and there is no
//! shared mutable state to lock. A local engine answers a container list in
//! milliseconds; `/system/df` is the one slow call, which is why it is last and
//! cached.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const api = @import("engine/api.zig");
const discovery = @import("engine/discovery.zig");
const format = @import("ui/format.zig");
const shell = @import("ui/shell.zig");

pub const Tab = enum {
    overview,
    containers,
    images,
    volumes,
    networks,
    stacks,
    disk,
    runtimes,
    settings,

    pub fn label(self: Tab) []const u8 {
        return switch (self) {
            .overview => "Overview",
            .containers => "Containers",
            .images => "Images",
            .volumes => "Volumes",
            .networks => "Networks",
            .stacks => "Stacks",
            .disk => "Disk",
            .runtimes => "Runtimes",
            .settings => "Settings",
        };
    }

    /// The fixed icon vocabulary from the design contract: these are the only
    /// names the framework draws on both platforms.
    pub fn icon(self: Tab) []const u8 {
        return switch (self) {
            .overview => "cpu",
            .containers => "cube",
            .images => "doc",
            .volumes => "folder",
            .networks => "network",
            .stacks => "code",
            .disk => "internaldrive",
            .runtimes => "desktopcomputer",
            .settings => "pencil",
        };
    }
};

const Step = enum { idle, ping, version, info, containers, images, volumes, networks, df, done };

pub const Toast = struct {
    text: []const u8,
    bad: bool = false,
};

pub const Pending = struct {
    id: []const u8,
    name: []const u8,
    method: []const u8,
    path: []const u8,
    verb: []const u8,
};

pub const State = struct {
    gpa: std.mem.Allocator,
    io: std.Io,
    app: *nanoui.App,
    window: ?*nanoui.Window = null,

    tab: Tab = .overview,
    runtimes: []discovery.Runtime = &.{},
    active: usize = 0,

    client: ?api.Client = null,
    arena: std.heap.ArenaAllocator,

    containers: []api.Container = &.{},
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    df: ?api.Df = null,

    step: Step = .idle,
    error_text: []const u8 = "",
    last_update: i64 = 0,
    toast: ?Toast = null,
    pending: ?Pending = null,

    /// The view the window calls every frame.
    pub fn view(s: *State, c: *ui.Context) void {
        shell.view(s, c);
    }

    pub fn create(gpa: std.mem.Allocator, io: std.Io, app: *nanoui.App) !*State {
        const s = try gpa.create(State);
        s.* = .{ .gpa = gpa, .io = io, .app = app, .arena = .init(gpa) };
        return s;
    }

    pub fn destroy(s: *State) void {
        s.dropRuntimes();
        s.arena.deinit();
        s.gpa.destroy(s);
    }

    fn dropRuntimes(s: *State) void {
        if (s.runtimes.len == 0) return;
        for (s.runtimes) |r| {
            s.gpa.free(r.name);
            s.gpa.free(r.socket);
            if (r.error_text.len > 0) s.gpa.free(r.error_text);
        }
        s.gpa.free(s.runtimes);
        s.runtimes = &.{};
    }

    /// A fresh snapshot: the old arena goes, the parsed data with it.
    fn resetData(s: *State) void {
        s.arena.deinit();
        s.arena = .init(s.gpa);
        s.containers = &.{};
        s.images = &.{};
        s.volumes = &.{};
        s.networks = &.{};
        s.df = null;
        s.error_text = "";
        if (s.client) |*c| c.alloc = s.arena.allocator();
    }

    pub fn activeRuntime(s: *State) ?*discovery.Runtime {
        if (s.runtimes.len == 0) return null;
        if (s.active >= s.runtimes.len) return null;
        return &s.runtimes[s.active];
    }

    pub fn selectRuntime(s: *State, index: usize) void {
        if (index >= s.runtimes.len) return;
        s.active = index;
        const rt = &s.runtimes[index];
        s.client = api.Client.init(s.io, s.gpa, rt.socket);
        s.client.?.alloc = s.arena.allocator();
        s.refresh();
    }

    /// Rescans for engines, picks the first that answers, and loads it.
    pub fn discover(s: *State) void {
        s.dropRuntimes();
        const found = discovery.scan(s.gpa, s.io) catch {
            s.error_text = "could not scan for container engines";
            s.redraw();
            return;
        };
        s.runtimes = found.runtimes;
        s.active = 0;
        for (s.runtimes, 0..) |r, i| {
            if (r.reachable) {
                s.active = i;
                break;
            }
        }
        if (s.runtimes.len == 0) {
            s.error_text = "no container engine found";
            s.redraw();
            return;
        }
        const rt = &s.runtimes[s.active];
        if (!rt.reachable) {
            s.error_text = "no container engine is answering";
            s.redraw();
            return;
        }
        s.client = api.Client.init(s.io, s.gpa, rt.socket);
        s.client.?.alloc = s.arena.allocator();
        s.refresh();
    }

    pub fn refresh(s: *State) void {
        if (s.client == null) return;
        s.resetData();
        s.step = .ping;
        nanoui.App.after(0, s, tick);
        s.redraw();
    }

    pub fn redraw(s: *State) void {
        if (s.window) |w| w.redraw();
    }

    pub fn notify(s: *State, text: []const u8, bad: bool) void {
        s.toast = .{ .text = text, .bad = bad };
        s.redraw();
    }

    /// Runs one action against the engine, then refreshes.
    pub fn act(s: *State, method: []const u8, path: []const u8, verb: []const u8) void {
        var client = s.client orelse return;
        client.action(method, path) catch {
            s.notify("action failed", true);
            return;
        };
        s.notify(verb, false);
        s.refresh();
    }

    pub fn confirm(s: *State, id: []const u8, name: []const u8, verb: []const u8, method: []const u8, comptime path_fmt: []const u8) void {
        const path = std.fmt.allocPrint(s.gpa, path_fmt, .{id}) catch return;
        s.pending = .{
            .id = s.gpa.dupe(u8, id) catch return,
            .name = s.gpa.dupe(u8, name) catch return,
            .method = method,
            .path = path,
            .verb = verb,
        };
        s.redraw();
    }

    pub fn resolvePending(s: *State, ok: bool) void {
        const p = s.pending orelse return;
        s.pending = null;
        if (ok) s.act(p.method, p.path, p.verb) else s.redraw();
        s.gpa.free(p.id);
        s.gpa.free(p.name);
        s.gpa.free(p.path);
    }
};

/// One request per turn, rescheduling itself, so the frame keeps painting.
fn tick(ctx: ?*anyopaque) callconv(.c) void {
    const s: *State = @ptrCast(@alignCast(ctx.?));
    if (s.client == null) return;
    // A pointer into the state, not to a temporary: `&(s.client orelse return)`
    // aliases a copy, so an API version adopted here would be thrown away.
    const client = &s.client.?;

    switch (s.step) {
        .ping => {
            if (!discovery.probe(client)) {
                s.error_text = "engine stopped answering";
                s.step = .done;
                s.redraw();
                return;
            }
            s.step = .version;
        },
        .version => {
            const v = client.version() catch {
                s.error_text = if (client.err_body.len > 0) client.err_body else "could not read the engine version";
                s.step = .done;
                s.redraw();
                return;
            };
            if (s.activeRuntime()) |rt| rt.version = v;
            // Speak whatever this engine speaks: asking Docker 29 for 1.43
            // routes earns a 400, not an answer.
            client.adoptApi(v.ApiVersion);
            s.step = .info;
        },
        .info => {
            const info = client.info() catch {
                s.error_text = if (client.err_body.len > 0) client.err_body else "could not read engine info";
                s.step = .done;
                s.redraw();
                return;
            };
            if (s.activeRuntime()) |rt| rt.info = info;
            s.step = .containers;
        },
        .containers => {
            s.containers = client.containers() catch {
                s.error_text = if (client.err_body.len > 0) client.err_body else "could not list containers";
                s.step = .done;
                s.redraw();
                return;
            };
            s.redraw();
            s.step = .images;
        },
        .images => {
            s.images = client.images() catch &.{};
            s.redraw();
            s.step = .volumes;
        },
        .volumes => {
            const list = client.volumes() catch api.ListResponse(api.Volume){};
            s.volumes = list.Volumes;
            s.redraw();
            s.step = .networks;
        },
        .networks => {
            s.networks = client.networks() catch &.{};
            s.redraw();
            s.step = .df;
        },
        .df => {
            s.df = client.df() catch null;
            s.last_update = format.now();
            s.step = .done;
            s.redraw();
        },
        .done, .idle => return,
    }
    if (s.step != .done) nanoui.App.after(0, s, tick) else s.redraw();
}

/// A tab by its name, for `BERTH_TAB`. A plain table: reflection over an
/// enum's field names changed shape in 0.17 and buys nothing here.
pub fn tabFromName(name: []const u8) ?Tab {
    const pairs = [_]struct { n: []const u8, t: Tab }{
        .{ .n = "overview", .t = .overview }, .{ .n = "containers", .t = .containers },
        .{ .n = "images", .t = .images },     .{ .n = "volumes", .t = .volumes },
        .{ .n = "networks", .t = .networks }, .{ .n = "stacks", .t = .stacks },
        .{ .n = "disk", .t = .disk },         .{ .n = "runtimes", .t = .runtimes },
        .{ .n = "settings", .t = .settings },
    };
    for (pairs) |pair| {
        if (std.ascii.eqlIgnoreCase(name, pair.n)) return pair.t;
    }
    return null;
}

/// How many containers are running, for the header count.
pub fn runningCount(s: *State) usize {
    var n: usize = 0;
    for (s.containers) |c| {
        if (c.isRunning()) n += 1;
    }
    return n;
}
