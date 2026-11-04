//! Container stats, fetched together instead of one per refresh turn.
//!
//! `/containers/{id}/stats?stream=false` waits two collection cycles (~2 s)
//! inside the engine. Doing that for each container in series made the window
//! unresponsive for two seconds per running container, so a bounded pool asks
//! for all of them at once and the UI waits for one cycle, not N.
const std = @import("std");
const api = @import("engine/api.zig");

/// One container's figures, from a single stats sample.
pub const Sample = struct {
    id: []const u8 = "",
    cpu: f64 = 0,
    /// False when the engine sent no previous CPU sample, so `cpu` is not a
    /// measurement and must render as a dash.
    cpu_known: bool = false,
    mem: u64 = 0,
    limit: u64 = 0,
    net_rx: u64 = 0,
    net_tx: u64 = 0,
    blk_read: u64 = 0,
    blk_write: u64 = 0,
    pids: ?u32 = null,
    /// False until a stats call succeeded: an unknown figure must read as a
    /// dash, not as the zero the struct was initialized with.
    sampled: bool = false,
};

/// Reduces a parsed stats document to the numbers the UI keeps. CPU stays
/// optional past this point: a missing previous sample is not zero.
pub fn fromStats(id: []const u8, st: api.Stats) Sample {
    const net = st.netTotals();
    const disk = st.diskTotals();
    const cpu = st.cpuPercent();
    return .{
        .id = id,
        .cpu = cpu orelse 0,
        .cpu_known = cpu != null,
        .mem = st.memUsed(),
        .limit = st.memLimit(),
        .net_rx = net.rx,
        .net_tx = net.tx,
        .blk_read = disk.read,
        .blk_write = disk.write,
        .pids = st.pidsCurrent(),
        .sampled = true,
    };
}

/// The containers' CPU share of the whole machine. Null unless every running
/// container has a real measurement: a partial sum is a lower bound, not the
/// figure the overview and menu bar claim to show.
pub fn aggregateCpuPercent(containers: []const api.Container, stats: []const Sample, cores: f32) ?f32 {
    var sum: f32 = 0;
    for (containers, 0..) |ct, i| {
        if (!ct.isUp()) continue;
        if (i >= stats.len or !stats[i].sampled or !stats[i].cpu_known) return null;
        sum += @as(f32, @floatCast(stats[i].cpu));
    }
    return sum / @max(cores, 1);
}

/// True only when every up container has a sample. A sum over a subset is a
/// lower bound, not the containers' memory use, so the UI must label it.
pub fn allSampled(containers: []const api.Container, stats: []const Sample) bool {
    for (containers, 0..) |ct, i| {
        if (!ct.isUp()) continue;
        if (i >= stats.len or !stats[i].sampled) return false;
    }
    return true;
}

/// Container memory for the overview and menu bar. Engine figures are u64 and
/// can be hostile maxima; saturating keeps the sum from wrapping into a tiny
/// "current memory" claim.
pub fn memoryUsed(containers: []const api.Container, stats: []const Sample) u64 {
    var total: u64 = 0;
    for (containers, 0..) |ct, i| {
        if (!ct.isUp() or i >= stats.len or !stats[i].sampled) continue;
        total = total +| stats[i].mem;
    }
    return total;
}

/// The engine holds every `/stats?stream=false` request for two collection
/// cycles (~2 s), so a pass costs `ceil(running / workers) × 2 s`. The cap has
/// to cover the container counts this app should keep inside its 5 s refresh
/// period: 64 workers put 64 containers in one cycle and 128 in two. These
/// threads block on a Unix socket instead of competing for a core, so 64 of
/// them cost stack and address space but not CPU; each owns its connection and
/// parser arena.
const workers_max = 64;

/// One worker per running container, capped at `workers_max`. `sample` returns
/// before this for an empty pass, so production never starts zero threads.
fn workerCount(running: usize) usize {
    return @min(running, workers_max);
}

const Job = struct {
    io: std.Io,
    socket: []const u8,
    api_version: []const u8,
    containers: []const api.Container,
    out: []Sample,
    first: usize,
    stride: usize,
    canceled: *const std.atomic.Value(bool),
    /// Test hook: successful samples per worker, indexed by `first`.
    assignments: ?[]usize = null,
};

fn work(job: Job) void {
    // A worker owns its parser arena: the parsed Stats carry slices into it,
    // and only the small numeric sample escapes.
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    var client = api.Client.init(job.io, std.heap.page_allocator, job.socket);
    client.api = job.api_version;
    client.alloc = arena.allocator();

    std.debug.assert(job.stride > 0);
    var running_index: usize = 0;
    for (job.containers, 0..) |ct, i| {
        if (!ct.isUp()) continue;
        // Workers stride over running containers, not list indices: stopped
        // rows must not land several running ones in the same worker's residue
        // class and stretch the pass back out. The counter wraps at `stride`,
        // so it cannot overflow however many containers the list holds.
        const slot = running_index;
        running_index += 1;
        if (running_index == job.stride) running_index = 0;
        if (slot != job.first or i >= job.out.len) continue;
        // A quit joins this thread; do not begin another two-second request
        // once shutdown has been requested.
        if (job.canceled.load(.acquire)) return;
        const st = client.stats(ct.Id) catch {
            _ = arena.reset(.retain_capacity);
            continue;
        };
        job.out[i] = fromStats(ct.Id, st);
        if (job.assignments) |counts| counts[job.first] += 1;
        // The parsed body is dead once its numbers are copied out. Reset per
        // container, not per worker: otherwise 64 workers each hold every
        // response they saw until the whole pass ends.
        _ = arena.reset(.retain_capacity);
        // Only a request already in flight delays the quit join; start no more.
        if (job.canceled.load(.acquire)) return;
    }
}

pub fn sample(
    io: std.Io,
    socket: []const u8,
    api_version: []const u8,
    containers: []const api.Container,
    out: []Sample,
    canceled: *const std.atomic.Value(bool),
) void {
    var running: usize = 0;
    for (containers) |ct| {
        if (ct.isUp()) running += 1;
    }
    if (running == 0 or canceled.load(.acquire)) return;
    sampleWithWorkers(io, socket, api_version, containers, out, workerCount(running), canceled, null);
}

/// The pool proper, with the worker count exposed so tests can pin a small
/// stride instead of depending on how many containers are running.
fn sampleWithWorkers(
    io: std.Io,
    socket: []const u8,
    api_version: []const u8,
    containers: []const api.Container,
    out: []Sample,
    workers: usize,
    canceled: *const std.atomic.Value(bool),
    assignments: ?[]usize,
) void {
    if (workers == 0 or canceled.load(.acquire)) return;
    std.debug.assert(workers <= workers_max);
    var threads: [workers_max]std.Thread = undefined;
    var spawned: usize = 0;
    for (0..workers) |i| {
        threads[spawned] = std.Thread.spawn(.{}, work, .{Job{
            .io = io,
            .socket = socket,
            .api_version = api_version,
            .containers = containers,
            .out = out,
            .first = i,
            .stride = workers,
            .canceled = canceled,
            .assignments = assignments,
        }}) catch break;
        spawned += 1;
    }
    for (threads[0..spawned]) |thread| thread.join();
    // A refused thread means the thread table is exhausted. Running the
    // remaining slots serially would turn one pass into up to 64 consecutive
    // two-second waits; those samples stay unknown and the next pass retries.
}

test "a sample without a previous cpu reading stays unknown" {
    const json =
        \\{"cpu_stats":{"cpu_usage":{"total_usage":3000000},"system_cpu_usage":40000000,"online_cpus":4}}
    ;
    var parsed = try std.json.parseFromSlice(api.Stats, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    const got = fromStats("abc", parsed.value);
    try std.testing.expect(got.sampled);
    try std.testing.expect(!got.cpu_known);
}

test "a sample with a previous cpu reading keeps the percentage" {
    const json =
        \\{"cpu_stats":{"cpu_usage":{"total_usage":3000000},"system_cpu_usage":40000000,"online_cpus":4},
        \\ "precpu_stats":{"cpu_usage":{"total_usage":1000000},"system_cpu_usage":20000000}}
    ;
    var parsed = try std.json.parseFromSlice(api.Stats, std.testing.allocator, json, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    const got = fromStats("abc", parsed.value);
    try std.testing.expect(got.cpu_known);
    try std.testing.expectApproxEqAbs(@as(f64, 40.0), got.cpu, 0.001);
}

test "an aggregate with one unknown cpu is unknown, not a lower bound" {
    const containers = [_]api.Container{
        .{ .Id = "a", .State = "running" },
        .{ .Id = "b", .State = "running" },
        .{ .Id = "c", .State = "exited" },
    };
    const known = [_]Sample{
        .{ .id = "a", .cpu = 20, .cpu_known = true, .sampled = true },
        .{ .id = "b", .cpu = 40, .cpu_known = true, .sampled = true },
        .{ .id = "c", .cpu = 999, .cpu_known = true, .sampled = true },
    };
    try std.testing.expectApproxEqAbs(@as(f32, 30), aggregateCpuPercent(&containers, &known, 2).?, 0.001);

    const mixed = [_]Sample{
        .{ .id = "a", .cpu = 20, .cpu_known = true, .sampled = true },
        .{ .id = "b", .cpu_known = false, .sampled = true },
        .{ .id = "c" },
    };
    try std.testing.expect(aggregateCpuPercent(&containers, &mixed, 2) == null);

    const none_up = [_]api.Container{.{ .Id = "c", .State = "exited" }};
    try std.testing.expectEqual(@as(?f32, 0), aggregateCpuPercent(&none_up, &mixed, 2));
}

test "a partial stats pass is not reported as complete" {
    const containers = [_]api.Container{
        .{ .Id = "a", .State = "running" },
        .{ .Id = "b", .State = "running" },
        .{ .Id = "c", .State = "exited" },
    };
    const all = [_]Sample{
        .{ .id = "a", .sampled = true },
        .{ .id = "b", .sampled = true },
        .{},
    };
    try std.testing.expect(allSampled(&containers, &all));

    const partial = [_]Sample{
        .{ .id = "a", .sampled = true },
        .{ .id = "b", .sampled = false },
        .{ .id = "c", .sampled = true },
    };
    try std.testing.expect(!allSampled(&containers, &partial));
    try std.testing.expect(!allSampled(&containers, all[0..1]));
    // No running containers is a complete zero, not a partial sample.
    const stopped = [_]api.Container{.{ .Id = "c", .State = "exited" }};
    try std.testing.expect(allSampled(&stopped, &.{}));
}

test "overview and tray memory sums saturate instead of wrapping" {
    const max = std.math.maxInt(u64);
    const containers = [_]api.Container{
        .{ .Id = "a", .State = "running" },
        .{ .Id = "b", .State = "running" },
        .{ .Id = "c", .State = "exited" },
    };
    const stats = [_]Sample{
        .{ .id = "a", .mem = max, .sampled = true },
        .{ .id = "b", .mem = 5, .sampled = true },
        .{ .id = "c", .mem = max, .sampled = true },
    };
    try std.testing.expectEqual(max, memoryUsed(&containers, &stats));

    const normal = [_]Sample{
        .{ .id = "a", .mem = 10, .sampled = true },
        .{ .id = "b", .mem = 20, .sampled = true },
        .{ .id = "c", .mem = 999, .sampled = true },
    };
    try std.testing.expectEqual(@as(u64, 30), memoryUsed(&containers, &normal));
    try std.testing.expectEqual(@as(u64, 0), memoryUsed(&containers, &.{}));
}

test "worker count covers every running container and never starts zero" {
    try std.testing.expectEqual(@as(usize, 0), workerCount(0));
    try std.testing.expectEqual(@as(usize, 1), workerCount(1));
    try std.testing.expectEqual(@as(usize, 7), workerCount(7));
    try std.testing.expectEqual(@as(usize, workers_max - 1), workerCount(workers_max - 1));
    try std.testing.expectEqual(workers_max, workerCount(workers_max));
    try std.testing.expectEqual(workers_max, workerCount(10_000));
}

test "the cap keeps common container counts inside one refresh period" {
    // Nine containers used to take two batches with the old cap of eight; the
    // wider pool puts them in one.
    try std.testing.expectEqual(@as(usize, 9), workerCount(9));
    try std.testing.expectEqual(workers_max, workerCount(100));
    // 100 running over 64 workers is two ~2 s batches, inside 5 s.
    try std.testing.expectEqual(@as(usize, 2), @as(usize, 100 / workers_max) + @intFromBool(100 % workers_max != 0));
    try std.testing.expectEqual(workers_max, workerCount(std.math.maxInt(usize)));
}

test "a canceled sample starts no workers" {
    var canceled = std.atomic.Value(bool).init(true);
    const containers = [_]api.Container{.{ .Id = "a", .State = "running" }};
    var out = [_]Sample{.{}};
    // The socket is never opened: cancellation is checked before the spawn.
    sample(std.testing.io, "/nonexistent/berth.sock", "v1.43", &containers, &out, &canceled);
    try std.testing.expect(!out[0].sampled);
}

test "sample spreads interleaved running containers evenly across workers" {
    const MockEngine = @import("engine/test_server.zig").MockEngine;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);
    var mock: MockEngine = .{ .io = std.testing.io, .listener = &listener };
    try mock.start();
    defer mock.finish() catch {};

    const containers = [_]api.Container{
        .{ .Id = "r0", .State = "running" },
        .{ .Id = "s0", .State = "exited" },
        .{ .Id = "r1", .State = "running" },
        .{ .Id = "s1", .State = "exited" },
        .{ .Id = "r2", .State = "running" },
        .{ .Id = "s2", .State = "exited" },
        .{ .Id = "r3", .State = "running" },
    };
    var out: [containers.len]Sample = @splat(.{});
    var canceled = std.atomic.Value(bool).init(false);
    var assignments: [2]usize = @splat(0);
    // Two workers over four running containers, with stopped rows interleaved:
    // every running row must be requested exactly once, and no stopped row at
    // all. Counting running ordinals rather than list indices is what keeps the
    // two workers equally loaded here; index-stride would give [4, 0].
    sampleWithWorkers(std.testing.io, path, "v1.43", &containers, &out, 2, &canceled, &assignments);
    try mock.finish();

    try std.testing.expectEqual(@as(usize, 2), assignments[0]);
    try std.testing.expectEqual(@as(usize, 2), assignments[1]);

    const running_ids = [_][]const u8{ "r0", "r1", "r2", "r3" };
    var seen: [running_ids.len]bool = @splat(false);
    try std.testing.expectEqual(running_ids.len, mock.count);
    for (0..mock.count) |i| {
        const id = mock.containerId(i);
        var known = false;
        for (running_ids, 0..) |want, k| {
            if (!std.mem.eql(u8, id, want)) continue;
            try std.testing.expect(!seen[k]);
            seen[k] = true;
            known = true;
            break;
        }
        // A stopped row in this list is a bug even if every running id arrived.
        try std.testing.expect(known);
    }
    for (seen) |got| try std.testing.expect(got);
    for (containers, 0..) |ct, i| {
        try std.testing.expectEqual(ct.isUp(), out[i].sampled);
    }
}

test "a canceled sample stops after the first response" {
    const MockEngine = @import("engine/test_server.zig").MockEngine;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var dirbuf: [256]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &dirbuf);
    var pathbuf: [320]u8 = undefined;
    const path = try std.fmt.bufPrint(&pathbuf, "{s}/engine.sock", .{dirbuf[0..n]});

    var addr = try std.Io.net.UnixAddress.init(path);
    var listener = try addr.listen(std.testing.io, .{});
    defer listener.deinit(std.testing.io);

    var canceled = std.atomic.Value(bool).init(false);
    var mock: MockEngine = .{
        .io = std.testing.io,
        .listener = &listener,
        .cancel_after = 1,
        .canceled = &canceled,
    };
    try mock.start();
    defer mock.finish() catch {};

    const containers = [_]api.Container{
        .{ .Id = "r0", .State = "running" },
        .{ .Id = "r1", .State = "running" },
        .{ .Id = "r2", .State = "running" },
    };
    var out: [containers.len]Sample = @splat(.{});
    // One worker so "after the first response" means before the next request,
    // not a race with the other workers' already-open connections.
    sampleWithWorkers(std.testing.io, path, "v1.43", &containers, &out, 1, &canceled, null);
    try mock.finish();

    try std.testing.expectEqual(@as(usize, 1), mock.count);
    try std.testing.expect(out[0].sampled);
    try std.testing.expect(!out[1].sampled);
    try std.testing.expect(!out[2].sampled);
}
