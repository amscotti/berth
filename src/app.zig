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
const History = @import("ui/history.zig").History;
const host = @import("host/host.zig");
const settings = @import("settings.zig");
const builtin = @import("builtin");

extern "c" fn getenv(name: [*:0]const u8) ?[*:0]u8;

/// Whether the desktop says dark. macOS is handled by the framework (passing
/// no theme follows the system, live); on Linux the GTK theme name is the
/// signal GTK itself uses, and there is no event for it here.
fn systemPrefersDark() bool {
    if (builtin.os.tag == .macos) return false;
    const raw = getenv("GTK_THEME") orelse return false;
    const name = std.mem.span(raw);
    var i: usize = 0;
    while (i + 4 <= name.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(name[i .. i + 4], "dark")) return true;
    }
    return false;
}
const shell = @import("ui/shell.zig");

/// What the container list shows. Filters are per view and stay where the
/// user left them, but they are visible as chips — a filtered list must never
/// look like a short one.
pub const Filter = enum {
    all,
    running,
    stopped,

    pub fn label(self: Filter) []const u8 {
        return switch (self) {
            .all => "All",
            .running => "Running",
            .stopped => "Stopped",
        };
    }

    pub fn keeps(self: Filter, ct: *const api.Container) bool {
        return switch (self) {
            .all => true,
            .running => ct.isRunning(),
            .stopped => !ct.isRunning(),
        };
    }
};

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

const Step = enum { idle, ping, version, info, containers, images, volumes, networks, stats, df, done };

/// One container's live figures, from a single stats sample.
pub const Stat = struct {
    id: []const u8 = "",
    cpu: f64 = 0,
    mem: u64 = 0,
    limit: u64 = 0,
};

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
    filter: Filter = .all,
    runtimes: []discovery.Runtime = &.{},
    active: usize = 0,

    client: ?api.Client = null,
    arena: std.heap.ArenaAllocator,
    /// The arena from the previous refresh. Kept alive for one cycle because
    /// the frame that is already on screen still points into it; freeing it
    /// immediately is a use-after-free that shows up as a segfault in memcpy.
    retired: ?std.heap.ArenaAllocator = null,

    containers: []api.Container = &.{},
    stats: []Stat = &.{},
    stat_i: usize = 0,
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    df: ?api.Df = null,

    /// Light, dark, or whatever the desktop says.
    appearance: settings.Appearance = .system,
    /// Set by main: rebuilds the window when the appearance changes, because
    /// the theme is a window-creation option, not something to mutate.
    recreate: ?*const fn (*State) void = null,

    /// The container whose detail pane is open, by id.
    selected: ?[]const u8 = null,
    selected_name: []const u8 = "",
    log_text: []const u8 = "",
    /// A name to select as soon as the list arrives (`BERTH_SELECT`), so the
    /// detail pane can be snapshotted headlessly.
    pending_select: []const u8 = "",
    /// `verb:name` to run once the list arrives (`BERTH_ACT`), so the action
    /// path can be exercised without a mouse: "stop:web", "restart:db".
    pending_act: []const u8 = "",
    /// The act being run, kept while `pending_act` is cleared.
    pending_act_saved: []const u8 = "",

    /// What the overview graphs: a few minutes of what the engine has been
    /// doing, sampled once per refresh.
    history: History = .{},
    /// The machine itself, sampled alongside the engine.
    host: host.Sampler = .{},
    host_sample: host.Sample = .{},
    /// Keep sampling on a timer, so the graphs move without a click.
    auto_refresh: bool = true,

    step: Step = .idle,
    error_text: []const u8 = "",
    last_update: i64 = 0,
    toast: ?Toast = null,
    pending: ?Pending = null,

    /// The theme to open a window with. `null` means "follow the system",
    /// which the framework does live on macOS; Linux has no such notification
    /// here, so the desktop's own setting is read once instead.
    pub fn themeFor(s: *State) ?ui.Theme {
        return switch (s.appearance) {
            .system => if (builtin.os.tag == .macos) null else if (systemPrefersDark()) ui.Theme.dark_theme else ui.Theme.light,
            .light => ui.Theme.light,
            .dark => ui.Theme.dark_theme,
        };
    }

    /// Resolves the container named in `pending_act` and runs the verb against
    /// it through the same path the buttons use.
    fn runPendingAct(s: *State) void {
        const spec = s.pending_act_saved;
        const colon = std.mem.indexOfScalar(u8, spec, ':') orelse return;
        const verb = spec[0..colon];
        const name = spec[colon + 1 ..];
        for (s.containers) |ct| {
            if (!std.mem.eql(u8, ct.name(), name)) continue;
            var buf: [256]u8 = undefined;
            if (std.mem.eql(u8, verb, "stop")) {
                const path = std.fmt.bufPrint(&buf, "/containers/{s}/stop", .{ct.Id}) catch return;
                s.act("POST", path, "Stopped");
            } else if (std.mem.eql(u8, verb, "start")) {
                const path = std.fmt.bufPrint(&buf, "/containers/{s}/start", .{ct.Id}) catch return;
                s.act("POST", path, "Started");
            } else if (std.mem.eql(u8, verb, "restart")) {
                const path = std.fmt.bufPrint(&buf, "/containers/{s}/restart", .{ct.Id}) catch return;
                s.act("POST", path, "Restarted");
            } else if (std.mem.eql(u8, verb, "delete")) {
                const path = std.fmt.bufPrint(&buf, "/containers/{s}?force=1", .{ct.Id}) catch return;
                s.act("DELETE", path, "Deleted");
            }
            return;
        }
    }

    /// Saves the choice, then opens the window again with it — after this
    /// frame, not during it: closing the window being painted is asking for
    /// trouble.
    pub fn setAppearance(s: *State, appearance: settings.Appearance) void {
        s.appearance = appearance;
        settings.save(s.gpa, s.io, .{ .appearance = appearance }) catch {};
        nanoui.App.after(0, s, applyAppearance);
    }

    fn applyAppearance(ctx: ?*anyopaque) callconv(.c) void {
        const s: *State = @ptrCast(@alignCast(ctx.?));
        if (s.recreate) |reopen| reopen(s);
    }

    /// One point for the graphs: what the containers are using, how many are
    /// up, and how much disk the engine holds.
    fn sample(s: *State) void {
        var cpu: f32 = 0;
        var mem: f32 = 0;
        var running: f32 = 0;
        for (s.containers, 0..) |ct, i| {
            if (!ct.isRunning()) continue;
            running += 1;
            if (i < s.stats.len) {
                cpu += @floatCast(s.stats[i].cpu);
                mem += @floatFromInt(s.stats[i].mem);
            }
        }
        var disk: f32 = 0;
        if (s.df) |df| disk = @floatFromInt(@max(@as(i64, 0), df.imageBytes()));
        s.host_sample = s.host.sample(s.io, s.gpa);
        s.history.push(.{
            .cpu = cpu,
            .mem = mem,
            .running = running,
            .disk = disk,
            .host_cpu = s.host_sample.cpu_percent,
            .host_mem = @floatFromInt(s.host_sample.mem_used),
        });
    }

    /// Opens a container's detail pane and reads its last 200 log lines.
    pub fn select(s: *State, ct: *const api.Container) void {
        s.selected = ct.Id;
        s.selected_name = ct.name();
        s.loadLogs(ct.Id);
    }

    pub fn clearSelection(s: *State) void {
        s.selected = null;
        s.selected_name = "";
        s.log_text = "";
        s.redraw();
    }

    fn loadLogs(s: *State, id: []const u8) void {
        var client = s.client orelse return;
        s.log_text = client.logs(id, 200) catch "could not read this container's logs";
        s.redraw();
    }

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
        if (s.retired) |*r| r.deinit();
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
        if (s.retired) |*old| old.deinit();
        s.retired = s.arena; // one more cycle before this is freed
        s.arena = .init(s.gpa);
        s.containers = &.{};
        s.stats = &.{};
        s.stat_i = 0;
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
            // One slot per container, filled a turn at a time: each sample
            // waits two collection cycles, so doing them all at once would
            // freeze the window for seconds.
            s.stats = s.arena.allocator().alloc(Stat, s.containers.len) catch &.{};
            for (s.containers, 0..) |ct, i| {
                if (i < s.stats.len) s.stats[i] = .{ .id = ct.Id };
            }
            s.stat_i = 0;
            // The id and name we were holding point into the arena that is now
            // retired: find the container again in the fresh snapshot.
            if (s.selected) |old_id| {
                var kept = false;
                for (s.containers) |ct| {
                    if (std.mem.eql(u8, ct.Id, old_id)) {
                        s.selected = ct.Id;
                        s.selected_name = ct.name();
                        kept = true;
                        break;
                    }
                }
                if (!kept) {
                    s.selected = null;
                    s.selected_name = "";
                    s.log_text = "";
                } else {
                    s.loadLogs(s.selected.?);
                }
            }
            if (s.pending_select.len > 0) {
                for (s.containers) |ct| {
                    if (std.mem.eql(u8, ct.name(), s.pending_select)) {
                        s.selected = ct.Id;
                        s.selected_name = ct.name();
                        s.loadLogs(ct.Id);
                        break;
                    }
                }
            }
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
            s.step = .stats;
        },
        .stats => {
            if (s.stat_i >= s.containers.len) {
                s.stat_i = 0;
                s.step = .df;
            } else {
                const ct = s.containers[s.stat_i];
                if (ct.isRunning() and s.stat_i < s.stats.len) {
                    if (client.stats(ct.Id)) |st| {
                        s.stats[s.stat_i] = .{
                            .id = ct.Id,
                            .cpu = st.cpuPercent(),
                            .mem = st.memUsed(),
                            .limit = st.memLimit(),
                        };
                    } else |_| {}
                }
                s.stat_i += 1;
                s.redraw();
                // Stay on this step, but let the window paint between samples.
                nanoui.App.after(0, s, tick);
                return;
            }
        },
        .df => {
            s.df = client.df() catch null;
            s.last_update = format.now();
            s.step = .done;
            s.sample();
            // Only now is it safe to act: `act` starts a new refresh, and
            // doing that from inside a step let this chain overwrite it.
            if (s.pending_act.len > 0) {
                const spec = s.pending_act;
                s.pending_act = "";
                s.pending_act_saved = spec;
                s.runPendingAct();
            }
            s.redraw();
            if (s.auto_refresh) nanoui.App.after(5000, s, autoTick);
        },
        .done, .idle => return,
    }
    if (s.step != .done) nanoui.App.after(0, s, tick) else s.redraw();
}

/// A container's figures, or zeros when none were collected (a stopped
/// container has no stats at all).
pub fn statFor(s: *State, id: []const u8) Stat {
    for (s.stats) |st| {
        if (std.mem.eql(u8, st.id, id)) return st;
    }
    return .{};
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

/// The timer that keeps the graphs fed: one refresh, then it re-arms itself
/// from the end of that refresh.
fn autoTick(ctx: ?*anyopaque) callconv(.c) void {
    const s: *State = @ptrCast(@alignCast(ctx.?));
    if (s.step != .done) return; // a refresh is already in flight
    s.refresh();
}

/// How many containers are running, for the header count.
pub fn runningCount(s: *State) usize {
    var n: usize = 0;
    for (s.containers) |c| {
        if (c.isRunning()) n += 1;
    }
    return n;
}
