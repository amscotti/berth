//! The application: one state, one snapshot, one dispatch to the views. Each
//! refresh runs on a worker thread (`refresh.zig`) and commits in one step, so
//! the UI thread never waits on the engine.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const api = @import("engine/api.zig");
const prune = @import("engine/prune.zig");
const cleanup_mod = @import("cleanup.zig");
const inspect = @import("engine/inspect.zig");
const lifetime = @import("engine/lifetime.zig");
const stats_pool = @import("stats_pool.zig");
const refresh_mod = @import("refresh.zig");
const discovery = @import("engine/discovery.zig");
const format = @import("ui/format.zig");
const widgets = @import("ui/widgets.zig");
const layout = @import("ui/layout.zig");
const History = @import("ui/history.zig").History;
const host = @import("host/host.zig");
const tabs = @import("tabs.zig");
const runtimes_mod = @import("runtimes.zig");
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

pub const Tab = tabs.Tab;
pub const Filter = tabs.Filter;
pub const tabFromName = tabs.tabFromName;

/// What the right-hand inspector is showing. `selected` holds the id for a
/// container or image, and the name for a volume or network.
pub const DetailKind = enum { container, image, volume, network };

/// What a cleanup run may remove; volumes and networks are opt-in.
pub const PruneOptions = struct {
    containers: bool = true,
    images: bool = true,
    volumes: bool = false,
    networks: bool = false,
    build_cache: bool = true,
};

/// One container's live figures, from a single stats sample.
pub const Stat = stats_pool.Sample;

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
    search: tabs.Search = .{},
    /// Recomputed once per frame from the window's width; views read it
    /// instead of guessing how much room the rail and inspector took.
    layout: layout.Widths = .{},
    sort_containers: widgets.Sort = .{ .key = 1 },
    sort_images: widgets.Sort = .{ .key = 1 },
    sort_volumes: widgets.Sort = .{ .key = 1 },
    sort_networks: widgets.Sort = .{ .key = 1 },
    runtimes: []discovery.Runtime = &.{},
    active: usize = 0,

    client: ?api.Client = null,
    arena: std.heap.ArenaAllocator,
    /// The arena from the previous refresh. Kept alive for one cycle because
    /// the frame that is already on screen still points into it; freeing it
    /// immediately is a use-after-free that shows up as a segfault in memcpy.
    retired: ?std.heap.ArenaAllocator = null,
    /// The background refresh, if one is in flight.
    job: ?*refresh_mod.Job = null,
    /// Bumped when the engine changes; a late job from the old engine is
    /// discarded instead of committing its snapshot over the new one.
    generation: u64 = 0,
    /// A refresh was asked for while a pass was in flight.
    refresh_wanted: bool = false,

    containers: []api.Container = &.{},
    stats: []Stat = &.{},
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    df: ?api.Df = null,

    /// Light, dark, or whatever the desktop says.
    appearance: settings.Appearance = .system,
    /// Set by main: rebuilds the window when the appearance changes, because
    /// the theme is a window-creation option, not something to mutate.
    recreate: ?*const fn (*State) void = null,

    /// The row whose detail pane is open; the meaning depends on `detail_kind`.
    detail_kind: DetailKind = .container,
    selected: ?[]const u8 = null,
    selected_name: []const u8 = "",
    log_text: []const u8 = "",
    /// Logs are owned by the state, not an arena: they must survive a snapshot
    /// swap, and are refreshed only when the selection changes or on request.
    log_buf: ?[]u8 = null,
    /// The detail pane's mode: curated facts, or the engine's own JSON.
    pane_inspect: bool = false,
    inspect_buf: ?[]u8 = null,
    /// Restarts and the exact start time for the selected container, read from
    /// one inspect on selection — not per refresh, which would double requests.
    detail_lifetime: ?lifetime.Lifetime = null,
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

    error_text: []const u8 = "",
    /// Backs an error copied out of a finished job's arena.
    error_buf: [256]u8 = undefined,
    last_update: i64 = 0,
    toast: ?Toast = null,
    toast_until: i64 = 0,
    /// Backs a formatted toast; `Toast.text` is a slice, not an owner.
    toast_buf: [160]u8 = undefined,
    pending: ?Pending = null,
    prune_open: bool = false,
    prune_opts: PruneOptions = .{},

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
    /// it through the same path the buttons use. Lives in `refresh.zig`, where
    /// its caller is, so this file stays inside the line rule.
    pub fn runPendingAct(s: *State) bool {
        return refresh_mod.pendingAct(s);
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

    /// The machine's own figures do not depend on an engine answering. Sampling
    /// them on the failure paths keeps the HOST card on screen while the
    /// engine is down, which is exactly when a monitor is being looked at.
    fn sampleHost(s: *State) void {
        s.host_sample = s.host.sample(s.io, s.gpa);
        s.redraw();
    }

    /// One point for the graphs: what the containers are using, how many are
    /// up, and how much disk the engine holds.
    pub fn sample(s: *State) void {
        var cpu: f32 = 0;
        var mem: f32 = 0;
        var running: f32 = 0;
        for (s.containers, 0..) |ct, i| {
            if (!ct.isUp()) continue;
            running += 1;
            if (i < s.stats.len) {
                cpu += @floatCast(s.stats[i].cpu);
                mem += @floatFromInt(s.stats[i].mem);
            }
        }
        var disk: f32 = 0;
        if (s.df) |df| disk = @floatFromInt(@max(@as(i64, 0), df.totalBytes()));
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
        s.detail_kind = .container;
        s.selected = ct.Id;
        s.selected_name = ct.name();
        s.clearInspect();
        s.pane_inspect = false;
        // The cheap, always-shown fact first: an engine that answers nothing
        // still opens the pane with what the list already knows.
        s.loadLifetime(ct.Id);
        s.loadLogs(ct.Id);
    }

    /// One inspect per selection: the restart count and the exact start time
    /// live there, and asking per refresh would double the request count.
    fn loadLifetime(s: *State, id: []const u8) void {
        var client = s.client orelse return;
        const raw = inspect.fetch(&client, .container, id, s.gpa) catch return;
        defer s.gpa.free(raw);
        s.detail_lifetime = lifetime.fromInspect(s.gpa, raw);
    }

    pub fn selectImage(s: *State, im: *const api.Image) void {
        s.detail_kind = .image;
        s.selected = im.Id;
        s.selected_name = if (im.dangling()) "untagged" else im.tag();
        s.clearLogs();
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn selectVolume(s: *State, v: *const api.Volume) void {
        s.detail_kind = .volume;
        s.selected = v.Name;
        s.selected_name = v.Name;
        s.clearLogs();
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn selectNetwork(s: *State, n: *const api.Network) void {
        s.detail_kind = .network;
        s.selected = if (n.Id.len > 0) n.Id else n.Name;
        s.selected_name = n.Name;
        s.clearLogs();
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn clearSelection(s: *State) void {
        s.detail_kind = .container;
        s.selected = null;
        s.selected_name = "";
        s.clearLogs();
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    /// Logs are read on demand, not on every refresh: the copy belongs to the
    /// state so a snapshot swap cannot invalidate it.
    pub fn reloadLogs(s: *State) void {
        if (s.detail_kind != .container) return;
        const id = s.selected orelse return;
        s.loadLogs(id);
    }

    /// The pane's raw JSON, on the same terms: read on demand, owned by the
    /// state, cleared with the selection it describes.
    pub fn reloadInspect(s: *State) void {
        const id = s.selected orelse return;
        // Only the JSON is being replaced; the lifetime facts belong to the
        // selection and must survive a glance at the raw document.
        s.clearInspectJson();
        var client = s.client orelse return;
        const kind: inspect.Kind = switch (s.detail_kind) {
            .container => .container,
            .image => .image,
            .volume => .volume,
            .network => .network,
        };
        const raw = inspect.fetch(&client, kind, id, s.gpa) catch {
            if (client.err_body.len > 0) {
                s.notifyFmt("{s}", .{client.err_body}, true);
            } else {
                s.notify("could not read the inspect JSON", true);
            }
            s.redraw();
            return;
        };
        defer s.gpa.free(raw);
        s.inspect_buf = inspect.pretty(s.gpa, raw, 256 * 1024) catch null;
        s.redraw();
    }

    fn loadLogs(s: *State, id: []const u8) void {
        s.clearLogs();
        var client = s.client orelse return;
        const text = client.logs(id, 200) catch {
            s.log_text = "could not read this container's logs";
            s.redraw();
            return;
        };
        const copy = s.gpa.dupe(u8, text) catch {
            s.log_text = "could not read this container's logs";
            s.redraw();
            return;
        };
        s.log_buf = copy;
        s.log_text = copy;
        s.redraw();
    }

    /// The view the window calls every frame.
    pub fn view(s: *State, c: *ui.Context) void {
        shell.view(s, c);
    }

    fn dropRuntimes(s: *State) void {
        runtimes_mod.drop(s);
    }

    /// Free the log copy that belongs to the current selection.
    pub fn clearLogs(s: *State) void {
        if (s.log_buf) |buf| s.gpa.free(buf);
        s.log_buf = null;
        s.log_text = "";
    }

    /// Same lifetime as the logs: the pane's JSON must survive snapshot swaps
    /// and belong to exactly one selected object.
    pub fn clearInspectJson(s: *State) void {
        if (s.inspect_buf) |buf| s.gpa.free(buf);
        s.inspect_buf = null;
    }

    pub fn clearInspect(s: *State) void {
        s.clearInspectJson();
        if (s.detail_lifetime) |*life| {
            if (life.run_cmd) |cmd| s.gpa.free(cmd);
        }
        s.detail_lifetime = null;
    }

    /// Drop the visible snapshot; only for switching engines.
    fn clearData(s: *State) void {
        if (s.retired) |*old| old.deinit();
        s.retired = s.arena;
        s.arena = .init(s.gpa);
        s.containers = &.{};
        s.stats = &.{};
        s.images = &.{};
        s.volumes = &.{};
        s.networks = &.{};
        s.df = null;
        s.error_text = "";
        // Nothing from the old engine is on screen any more: the views show
        // their skeletons until the new engine's first snapshot lands.
        s.last_update = 0;
        s.selected = null;
        s.selected_name = "";
        s.clearLogs();
        s.clearInspect();
        if (s.client) |*c| c.alloc = s.arena.allocator();
    }

    pub fn activeRuntime(s: *State) ?*discovery.Runtime {
        if (s.runtimes.len == 0) return null;
        if (s.active >= s.runtimes.len) return null;
        return &s.runtimes[s.active];
    }

    pub fn selectRuntime(s: *State, index: usize) void {
        if (index >= s.runtimes.len or s.job != null) return;
        s.active = index;
        s.generation += 1;
        const rt = &s.runtimes[index];
        s.client = api.Client.init(s.io, s.gpa, rt.socket);
        s.clearData();
        s.refresh();
    }

    /// Rescans for engines, picks the first that answers, and loads it.
    pub fn discover(s: *State) void {
        if (s.job != null) return;
        s.generation += 1;
        // The old client points at a socket owned by `runtimes`; stop using it
        // before those strings are freed.
        s.client = null;
        s.dropRuntimes();
        const found = discovery.scan(s.gpa, s.io) catch {
            s.error_text = "could not scan for container engines";
            s.sampleHost();
            s.retryDiscover();
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
            s.sampleHost();
            s.retryDiscover();
            return;
        }
        const rt = &s.runtimes[s.active];
        if (!rt.reachable) {
            s.error_text = "no container engine is answering";
            s.sampleHost();
            s.retryDiscover();
            return;
        }
        s.client = api.Client.init(s.io, s.gpa, rt.socket);
        s.client.?.alloc = s.arena.allocator();
        s.refresh();
    }

    /// Re-probe after a failed scan. Fifteen seconds, not five: probing every
    /// candidate is only cheap when one of them answers.
    fn retryDiscover(s: *State) void {
        if (s.auto_refresh) nanoui.App.after(15_000, s, refresh_mod.autoTick);
    }

    pub fn refresh(s: *State) void {
        if (s.client == null) return;
        if (s.job != null) {
            s.refresh_wanted = true;
            return;
        }
        refresh_mod.start(s);
    }

    pub fn redraw(s: *State) void {
        if (s.window) |w| w.redraw();
    }

    pub fn notify(s: *State, text: []const u8, bad: bool) void {
        s.toast = .{ .text = text, .bad = bad };
        s.toast_until = format.now() + 4;
        nanoui.App.after(4000, s, expireToast);
        s.redraw();
    }

    /// Toasts are transient now: the rail is the only place they appear, so a
    /// stale "Stopped" must not sit there forever.
    fn expireToast(ctx: ?*anyopaque) callconv(.c) void {
        const s: *State = @ptrCast(@alignCast(ctx.?));
        if (s.toast == null) return;
        if (format.now() < s.toast_until) {
            nanoui.App.after(1000, s, expireToast);
            return;
        }
        s.toast = null;
        s.redraw();
    }

    /// A toast with numbers in it, copied into `toast_buf` for the frame.
    pub fn notifyFmt(s: *State, comptime fmt: []const u8, args: anytype, bad: bool) void {
        const text = std.fmt.bufPrint(&s.toast_buf, fmt, args) catch "action failed";
        s.notify(text, bad);
    }

    pub fn openPrune(s: *State) void {
        s.prune_open = true;
        s.redraw();
    }

    pub fn closePrune(s: *State) void {
        s.prune_open = false;
        s.redraw();
    }

    /// Runs the selected prune endpoints; containers first, then their leavings.
    /// Lives in `cleanup.zig` beside its dialog; this indirection keeps the
    /// state machine inside the 600-line rule.
    pub fn runPrune(s: *State) void {
        cleanup_mod.runPrune(s);
    }

    /// Opens a container's published port in the default browser. A binding on
    /// a specific address goes there; the wildcard is localhost. The engine has
    /// no scheme hint, so 443 is https and everything else is http.
    pub fn openPort(s: *State, binding: api.HostBinding) void {
        const where = if (binding.ip.len > 0) binding.ip else "localhost";
        var buf: [80]u8 = undefined;
        const scheme: []const u8 = if (binding.port == 443 or binding.port == 8443) "https" else "http";
        const url = std.fmt.bufPrint(&buf, "{s}://{s}:{d}", .{ scheme, where, binding.port }) catch return;
        if (!nanoui.shell.openUrl(url)) s.notify("could not open the browser", true);
    }

    /// Runs one action against the engine, then refreshes. `path` must already
    /// carry the container's id — `actFmt` formats it. Returns whether a
    /// refresh was actually started, so a caller that also owns the timer does
    /// not re-arm it over the top of the new pass.
    pub fn act(s: *State, method: []const u8, path: []const u8, verb: []const u8) bool {
        var client = s.client orelse return false;
        client.action(method, path) catch {
            // The engine usually names the problem ("cannot pause a stopped
            // container"); show its words rather than a generic failure.
            if (client.err_body.len > 0) {
                s.notifyFmt("{s}", .{client.err_body}, true);
            } else {
                s.notify("action failed", true);
            }
            return false;
        };
        s.notify(verb, false);
        s.refresh();
        return true;
    }

    /// `act` with the path formatted at the call site, the way `confirm` does:
    /// a literal `"{s}"` passed through `act` would reach the engine unexpanded.
    pub fn actFmt(s: *State, method: []const u8, comptime path_fmt: []const u8, id: []const u8, verb: []const u8) bool {
        var buf: [160]u8 = undefined;
        const path = std.fmt.bufPrint(&buf, path_fmt, .{id}) catch return false;
        return s.act(method, path, verb);
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
        if (ok) {
            _ = s.act(p.method, p.path, p.verb);
        } else s.redraw();
        s.gpa.free(p.id);
        s.gpa.free(p.name);
        s.gpa.free(p.path);
    }
};

/// A container's figures, or zeros when none were collected (a stopped
/// container has no stats at all).
pub fn statFor(s: *State, id: []const u8) Stat {
    for (s.stats) |st| {
        if (std.mem.eql(u8, st.id, id)) return st;
    }
    return .{};
}

/// How many containers are running, for the header count.
pub fn runningCount(s: *State) usize {
    var n: usize = 0;
    for (s.containers) |c| {
        if (c.isUp()) n += 1;
    }
    return n;
}
