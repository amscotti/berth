//! The application: one state, one snapshot, one dispatch to the views. Each
//! refresh runs on a worker thread (`refresh.zig`) and commits in one step, so
//! the UI thread never waits on the engine.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const api = @import("engine/api.zig");
const cleanup_mod = @import("cleanup.zig");
const detail_job = @import("detail_job.zig");
const lifetime = @import("engine/lifetime.zig");
const stats_pool = @import("stats_pool.zig");
const refresh_mod = @import("refresh.zig");
const auto_tick = @import("auto_tick.zig");
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
const actions = @import("actions.zig");
const logs = @import("logs.zig");
const image_tasks = @import("image_tasks.zig");
const settings = @import("settings.zig");
const sheets = @import("sheets.zig");
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
pub const PruneOptions = actions.PruneOptions;

/// One container's live figures, from a single stats sample.
pub const Stat = stats_pool.Sample;

pub const Toast = struct {
    text: []const u8,
    bad: bool = false,
};

pub const Pending = actions.Pending;

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
    /// The one engine mutation in flight, if any.
    actions: actions.State = .{},
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
    /// Engine identity on screen, kept across failed scans to spot real changes.
    last_engine_id: ?[]u8 = null,
    last_engine_socket: ?[]u8 = null,
    last_engine_dialect: ?discovery.Dialect = null,
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
    /// Background socket scan; runtimes/client stay until it is adopted.
    discovery_job: ?*session.Job = null,
    discovery_test: ?session.TestHook = null, // fake scan for tests
    /// Bumped when the engine changes; a late job from the old engine is
    /// discarded instead of committing its snapshot over the new one.
    generation: u64 = 0,
    /// A refresh was asked for while a pass was in flight.
    refresh_wanted: bool = false,
    /// A rescan was asked for while a pass or scan was in flight.
    discover_wanted: bool = false,
    /// An engine chosen while a pass was in flight.
    select_wanted: ?usize = null,
    /// One discovery retry timer, not one per failed scan.
    retry_pending: bool = false,
    containers: []api.Container = &.{},
    stats: []Stat = &.{},
    /// False when any up container had no stats this pass; container memory is
    /// then a lower bound and must render as partial, not as the total.
    stats_complete: bool = false,
    images: []api.Image = &.{},
    volumes: []api.Volume = &.{},
    networks: []api.Network = &.{},
    events: events.Feed = .{},
    /// The last completed pass could not read `/events`; distinct from an
    /// empty ring, which means the engine really has nothing to show.
    events_error: bool = false,
    df: ?api.Df = null,
    /// The last completed pass could not read `/system/df`.
    df_error: bool = false,
    /// The engine's message for that failed df; cleared by a successful one.
    df_error_text: []const u8 = "",
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
    /// JSON was cut at the pane's cap; the copy toast says so.
    inspect_truncated: bool = false,
    /// Restarts and the exact start time for the selected container, read from
    /// one inspect on selection — not per refresh, which would double requests.
    detail_lifetime: ?lifetime.Lifetime = null,
    /// The one background read of the inspector's facts, logs, or JSON, so a
    /// selection or Refresh never waits on the engine inside the frame.
    detail_job: ?*detail_job.Job = null,
    /// Tests join the detail worker and call its completion directly.
    detail_dispatch: bool = true,
    /// The two halves of a selection read finish independently, so a log
    /// refresh must not make the lifetime facts look unread.
    lifetime_loading: bool = false,
    logs_loading: bool = false,
    /// True while the raw inspect document is being read for the pane.
    inspect_loading: bool = false,
    /// A Times/Refresh or Inspect click that arrived while the detail worker
    /// was busy. One shot: the next completion satisfies it or drops it.
    logs_wanted: bool = false,
    inspect_wanted: bool = false,
    /// Bumped by every selection change and every detail read; a late worker
    /// compares it before touching any of the state above.
    detail_generation: u64 = 0,
    /// A name to select as soon as the list arrives (`BERTH_SELECT`), so the
    /// detail pane can be snapshotted headlessly.
    pending_select: []const u8 = "",
    /// `verb:name` to run once the list arrives (`BERTH_ACT`), so the action
    /// path can be exercised without a mouse: "stop:web", "restart:db".
    pending_act: []const u8 = "",
    /// A sheet that needs a row or engine from the first snapshot
    /// (`BERTH_SHEET`), so dialogs can be photographed headlessly.
    pending_sheet: []const u8 = "",
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
    /// One automatic pass is already scheduled; another resume must not stack
    /// a second timer that would fire behind the same refresh.
    auto_tick_pending: bool = false,
    /// Timers handed to the main queue and not yet fired. The queue has no
    /// cancel, so the state owns them until the callback or teardown frees one.
    auto_ticks: std.ArrayList(*auto_tick.Tick) = .empty,
    /// Identifies the tick currently armed. A pass, switch or scan that starts
    /// later bumps this, so an older callback stands down instead of firing
    /// seconds after the chain it belongs to was replaced.
    auto_tick_token: u64 = 0,

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

    /// True while any background job is changing the engine. Pulling an image
    /// is a read, so it deliberately does not count; the rest cannot overlap
    /// without making the toast and follow-up refresh describe the wrong one.
    pub fn mutating(s: *const State) bool {
        return s.actions.running() or s.bulk.running() or s.image_tasks.running() or s.ops.running();
    }

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
        var running: f32 = 0;
        for (s.containers) |ct| {
            if (!ct.isUp()) continue;
            running += 1;
        }
        const mem: f32 = @floatFromInt(stats_pool.memoryUsed(s.containers, s.stats));
        var disk: f32 = 0;
        if (s.df) |df| disk = @floatFromInt(@max(@as(i64, 0), df.totalBytes()));
        // The table's CPU column is per core (docker stats style). The
        // overview says "of N cores", so its history must be the share of the
        // whole machine or the dots and the percentage disagree by N.
        const cores: f32 = if (s.activeRuntime()) |rt| @floatFromInt(@max(rt.info.NCPU, 1)) else 1;
        const cpu = stats_pool.aggregateCpuPercent(s.containers, s.stats, cores);
        s.host_sample = s.host.sample(s.io, s.gpa);
        s.host_sample.engine_rss = s.engine_rss;
        s.history.push(.{
            .cpu = cpu orelse 0,
            .cpu_known = cpu != null,
            .mem = mem,
            .mem_known = s.stats_complete,
            .running = running,
            .disk = disk,
            .host_cpu = s.host_sample.cpu_percent,
            .host_mem = @floatFromInt(s.host_sample.mem_used),
        });
    }

    /// Opens a container's detail pane immediately with the facts the list
    /// already knows; inspect and logs arrive later from `detail_job`.
    pub fn select(s: *State, ct: *const api.Container) void {
        s.detail_kind = .container;
        s.selected = ct.Id;
        s.selected_name = ct.name();
        detail_job.invalidate(s);
        s.clearInspect();
        s.pane_inspect = false;
        logs.clear(s);
        detail_job.startContainer(s, ct.Id);
    }

    pub fn selectImage(s: *State, im: *const api.Image) void {
        s.detail_kind = .image;
        s.selected = im.Id;
        s.selected_name = if (im.dangling()) "untagged" else im.tag();
        detail_job.invalidate(s);
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn selectVolume(s: *State, v: *const api.Volume) void {
        s.detail_kind = .volume;
        s.selected = v.Name;
        s.selected_name = v.Name;
        detail_job.invalidate(s);
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn selectNetwork(s: *State, n: *const api.Network) void {
        s.detail_kind = .network;
        s.selected = if (n.Id.len > 0) n.Id else n.Name;
        s.selected_name = n.Name;
        detail_job.invalidate(s);
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    pub fn clearSelection(s: *State) void {
        s.detail_kind = .container;
        s.selected = null;
        s.selected_name = "";
        detail_job.invalidate(s);
        logs.clear(s);
        s.clearInspect();
        s.pane_inspect = false;
        s.redraw();
    }

    /// The pane's raw JSON, on the same terms: read on demand, owned by the
    /// state, cleared with the selection it describes.
    pub fn reloadInspect(s: *State) void {
        detail_job.startInspect(s);
    }

    pub fn reloadLogs(s: *State) void {
        detail_job.startLogs(s);
    }

    /// The view the window calls every frame.
    pub fn view(s: *State, c: *ui.Context) void {
        shell.view(s, c);
    }

    /// Same lifetime as the logs: the pane's JSON must survive snapshot swaps
    /// and belong to exactly one selected object.
    pub fn clearInspectJson(s: *State) void {
        // The detail worker pretty-prints on the page allocator and the buffer
        // is owned here afterwards, so the matching free is the page one.
        if (s.inspect_buf) |buf| std.heap.page_allocator.free(buf);
        s.inspect_buf = null;
        s.inspect_truncated = false;
    }

    pub fn clearInspect(s: *State) void {
        s.clearInspectJson();
        s.clearLifetime();
    }

    /// Free the run command with the facts it belongs to; the pane might open
    /// another container before the next snapshot rotates the lists. The
    /// detail worker allocates it on the page allocator so it never shares the
    /// main thread's allocation bookkeeping across threads.
    pub fn clearLifetime(s: *State) void {
        if (s.detail_lifetime) |*life| {
            if (life.run_cmd) |cmd| std.heap.page_allocator.free(cmd);
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
        s.stats_complete = false;
        s.images = &.{};
        s.volumes = &.{};
        s.networks = &.{};
        s.events = .{};
        s.events_error = false;
        s.df = null;
        s.df_error = false;
        s.df_error_text = "";
        s.df_stale = false;
        s.history.clearEngine();
        s.bulk.clear();
        // A pending answer names the old engine's row; it cannot survive the
        // switch. This also resets the cleanup opt-ins and closes the form.
        sheets.dismiss(s);
        s.engine_rss = 0;
        s.error_text = "";
        // Nothing from the old engine is on screen any more: the views show
        // their skeletons until the new engine's first snapshot lands.
        s.last_update = 0;
        s.selected = null;
        s.selected_name = "";
        detail_job.invalidate(s);
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

    pub fn useSocket(s: *State, path: []const u8) void {
        session.useSocket(s, path);
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
        // Tests have no event loop to run the expiry, and the callback would
        // outlive the state they put on the stack.
        if (!builtin.is_test) nanoui.App.after(4000, s, expireToast);
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
        if (s.actions.running()) {
            s.notify(if (actions.pruning(s)) "cleanup is running" else "wait for the current engine operation to finish", true);
            return;
        }
        // The sheet owns the modal slot: a stop confirmation or create form
        // underneath it would be a second, contradictory question.
        sheets.dismiss(s);
        s.prune_open = true;
        s.redraw();
    }

    pub fn closePrune(s: *State) void {
        s.prune_open = false;
        // Cancel is a decision too: a tick from this visit must not turn into
        // a destructive default the next time the sheet opens.
        sheets.resetPruneOpts(s);
        s.redraw();
    }

    /// Starts the cleanup job; containers first, then their leavings. The run
    /// itself is in `actions.zig` so it cannot hold a frame.
    pub fn runPrune(s: *State) void {
        cleanup_mod.runPrune(s);
    }

    /// Opens a container's published port in the default browser. A binding on
    /// a specific address goes there; the wildcard is localhost. The engine has
    /// no scheme hint, so 443 is https and everything else is http. The address
    /// is validated first: an engine-supplied string must not choose the URL.
    pub fn openPort(s: *State, binding: api.HostBinding) void {
        const where = api.safeBrowserHost(binding.ip);
        var host_buf: [64]u8 = undefined;
        const host_text = if (std.mem.indexOfScalar(u8, where, ':') != null)
            std.fmt.bufPrint(&host_buf, "[{s}]", .{where}) catch return
        else
            where;
        var buf: [96]u8 = undefined;
        const scheme: []const u8 = if (binding.port == 443 or binding.port == 8443) "https" else "http";
        const url = std.fmt.bufPrint(&buf, "{s}://{s}:{d}", .{ scheme, host_text, binding.port }) catch return;
        if (!nanoui.shell.openUrl(url)) s.notify("could not open the browser", true);
    }

    /// Runs one action on the action worker. Returns whether the job started,
    /// so the refresh harness does not arm a timer over the pass the action
    /// will request when it finishes.
    pub fn act(s: *State, method: []const u8, path: []const u8, verb: []const u8) bool {
        return actions.act(s, method, path, verb);
    }

    pub fn actFmt(s: *State, method: []const u8, comptime path_fmt: []const u8, id: []const u8, verb: []const u8) bool {
        return actions.actFmt(s, method, path_fmt, id, verb);
    }

    pub fn confirm(s: *State, id: []const u8, name: []const u8, verb: []const u8, method: []const u8, comptime path_fmt: []const u8) void {
        actions.confirm(s, id, name, verb, method, path_fmt);
    }

    pub fn clearPending(s: *State) void {
        actions.clearPending(s);
    }

    pub fn resolvePending(s: *State, ok: bool) void {
        actions.resolvePending(s, ok);
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
