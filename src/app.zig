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
const events = @import("engine/events.zig");
const format = @import("ui/format.zig");
const widgets = @import("ui/widgets.zig");
const layout = @import("ui/layout.zig");
const History = @import("ui/history.zig").History;
const host = @import("host/host.zig");
const create_mod = @import("create.zig");
const tabs = @import("tabs.zig");
const session = @import("session.zig");
const ops = @import("ops.zig");
const bulk = @import("bulk.zig");
const logs = @import("logs.zig");
const image_tasks = @import("image_tasks.zig");
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
    /// Engine lifecycle commands, one at a time.
    ops: ops.State = .{},
    /// Container checkbox mode and its pending bulk action.
    bulk: bulk.State = .{},
    /// The menu-bar mini-monitor, where the platform has one.
    tray: ?*nanoui.Tray = null,
    /// Pull and run image jobs and their dialog text.
    image_tasks: image_tasks.State = .{},
    /// The volume/network creation dialog: which one is open, and its name.
    create_dialog: create_mod.State = .{},
    /// The Images page's creation card starts collapsed; Run… from an image
    /// opens it, as does clicking its header.
    creation_card_open: bool = false,
    /// The engine's host-process RSS, committed with each snapshot; the host
    /// bar reads it back through the last sample.
    engine_rss: u64 = 0,
    /// A headless harness action (`BERTH_PULL`, `BERTH_RUN_IMAGE`) waiting for
    /// the first snapshot, when a client exists to run it against.
    pending_image: image_tasks.Pending = .none,
    /// Set first in teardown: AppKit closes the window after the state is
    /// gone, and that close callback must not touch freed memory.
    closing: bool = false,
    sort_containers: widgets.Sort = .{ .key = 1 },
    sort_images: widgets.Sort = .{ .key = 1 },
    sort_volumes: widgets.Sort = .{ .key = 1 },
    sort_networks: widgets.Sort = .{ .key = 1 },
    runtimes: []discovery.Runtime = &.{},
    active: usize = 0,
    /// The socket last chosen or typed; probed before the built-in candidates.
    preferred_socket: ?[]u8 = null,
    /// Identity of the engine currently on screen, kept across failed scans so
    /// a later scan can tell a real engine change from another alias.
    last_engine_id: ?[]u8 = null,
    last_engine_socket: ?[]u8 = null,
    /// The custom-socket field and the settings engine menu.
    socket_input: ui.TextBuffer = .{},
    engine_menu_open: bool = false,
    /// The runtime kind whose sockets are expanded on the Runtimes page.
    expanded_kind: ?discovery.Kind = null,

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
    /// A rescan was asked for while a pass was in flight.
    discover_wanted: bool = false,
    /// An engine chosen while a pass was in flight.
    select_wanted: ?usize = null,
    /// One discovery retry timer, not one per failed scan.
    retry_pending: bool = false,

    containers: []api.Container = &.{},
    stats: []Stat = &.{},
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    events: events.Feed = .{},
    df: ?api.Df = null,
    /// A df may survive one arena rotation; drop it on the next failed pass.
    df_stale: bool = false,

    /// Light, dark, or whatever the desktop says.
    appearance: settings.Appearance = .system,
    /// Set by main: rebuilds the window when the appearance changes, because
    /// the theme is a window-creation option, not something to mutate.
    recreate: ?*const fn (*State) void = null,
    /// The content size the closed window had, so the tray's reopen puts it
    /// back where the user arranged it.
    last_size: ?[2]f64 = null,

    /// The row whose detail pane is open; the meaning depends on `detail_kind`.
    detail_kind: DetailKind = .container,
    selected: ?[]const u8 = null,
    selected_name: []const u8 = "",
    log_text: []const u8 = "",
    log_opts: logs.State = .{},
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

    /// Saves the choice, then opens the window again with it — after this
    /// frame, not during it: closing the window being painted is asking for
    /// trouble.
    pub fn setAppearance(s: *State, appearance: settings.Appearance) void {
        s.appearance = appearance;
        settings.save(s.gpa, s.io, .{ .appearance = appearance, .socket = s.preferred_socket orelse "" }) catch {};
        nanoui.App.after(0, s, applyAppearance);
    }

    fn applyAppearance(ctx: ?*anyopaque) callconv(.c) void {
        const s: *State = @ptrCast(@alignCast(ctx.?));
        if (s.recreate) |reopen| reopen(s);
    }

    /// The machine's own figures do not depend on an engine answering. Sampling
    /// them on the failure paths keeps the HOST card on screen while the
    /// engine is down, which is exactly when a monitor is being looked at.
    pub fn sampleHost(s: *State) void {
        s.host_sample = s.host.sample(s.io, s.gpa);
        s.host_sample.engine_rss = s.engine_rss;
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
            if (i < s.stats.len and s.stats[i].sampled) {
                cpu += @floatCast(s.stats[i].cpu);
                mem += @floatFromInt(s.stats[i].mem);
            }
        }
        var disk: f32 = 0;
        if (s.df) |df| disk = @floatFromInt(@max(@as(i64, 0), df.totalBytes()));
        // The table's CPU column is per core (docker stats style). The
        // overview says "of N cores", so its history must be the share of the
        // whole machine or the dots and the percentage disagree by N.
        const cores: f32 = if (s.activeRuntime()) |rt| @floatFromInt(@max(rt.info.NCPU, 1)) else 1;
        s.host_sample = s.host.sample(s.io, s.gpa);
        s.host_sample.engine_rss = s.engine_rss;
        s.history.push(.{
            .cpu = cpu / cores,
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
        logs.load(s, ct.Id);
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
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn selectVolume(s: *State, v: *const api.Volume) void {
        s.detail_kind = .volume;
        s.selected = v.Name;
        s.selected_name = v.Name;
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn selectNetwork(s: *State, n: *const api.Network) void {
        s.detail_kind = .network;
        s.selected = if (n.Id.len > 0) n.Id else n.Name;
        s.selected_name = n.Name;
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn clearSelection(s: *State) void {
        s.detail_kind = .container;
        s.selected = null;
        s.selected_name = "";
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
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

    /// The view the window calls every frame.
    pub fn view(s: *State, c: *ui.Context) void {
        shell.view(s, c);
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
    pub fn clearData(s: *State) void {
        if (s.retired) |*old| old.deinit();
        s.retired = s.arena;
        s.arena = .init(s.gpa);
        s.containers = &.{};
        s.stats = &.{};
        s.images = &.{};
        s.volumes = &.{};
        s.networks = &.{};
        s.events = .{};
        s.df = null;
        s.df_stale = false;
        s.history.clearEngine();
        s.clearPending();
        s.bulk.clear();
        s.prune_open = false;
        s.create_dialog.open = null;
        s.engine_rss = 0;
        s.error_text = "";
        // Nothing from the old engine is on screen any more: the views show
        // their skeletons until the new engine's first snapshot lands.
        s.last_update = 0;
        s.selected = null;
        s.selected_name = "";
        logs.clear(s);
        s.clearInspect();
        if (s.client) |*c| c.alloc = s.arena.allocator();
    }

    pub fn activeRuntime(s: *State) ?*discovery.Runtime {
        if (s.runtimes.len == 0) return null;
        if (s.active >= s.runtimes.len) return null;
        return &s.runtimes[s.active];
    }

    pub fn selectRuntime(s: *State, index: usize) void {
        session.selectRuntime(s, index);
    }

    pub fn discover(s: *State) void {
        session.discover(s);
    }

    pub fn refresh(s: *State) void {
        session.refresh(s);
    }

    pub fn useSocket(s: *State, path: []const u8) bool {
        return session.useSocket(s, path);
    }

    pub fn forgetSocket(s: *State) void {
        session.forgetSocket(s);
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
        const id_copy = s.gpa.dupe(u8, id) catch {
            s.gpa.free(path);
            return;
        };
        const name_copy = s.gpa.dupe(u8, name) catch {
            s.gpa.free(path);
            s.gpa.free(id_copy);
            return;
        };
        s.clearPending();
        s.pending = .{
            .id = id_copy,
            .name = name_copy,
            .method = method,
            .path = path,
            .verb = verb,
        };
        s.redraw();
    }

    pub fn clearPending(s: *State) void {
        const p = s.pending orelse return;
        s.pending = null;
        s.gpa.free(p.id);
        s.gpa.free(p.name);
        s.gpa.free(p.path);
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
