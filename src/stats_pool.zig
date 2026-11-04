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

const workers_max = 8;

const Job = struct {
    io: std.Io,
    socket: []const u8,
    api_version: []const u8,
    containers: []const api.Container,
    out: []Sample,
    first: usize,
    stride: usize,
};

fn work(job: Job) void {
    // A worker owns its parser arena: the parsed Stats carry slices into it,
    // and only the small numeric sample escapes.
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    var client = api.Client.init(job.io, std.heap.page_allocator, job.socket);
    client.api = job.api_version;
    client.alloc = arena.allocator();

    var i = job.first;
    while (i < job.containers.len) : (i += job.stride) {
        const ct = job.containers[i];
        if (!ct.isUp() or i >= job.out.len) continue;
        const st = client.stats(ct.Id) catch continue;
        job.out[i] = fromStats(ct.Id, st);
    }
}

pub fn sample(
    io: std.Io,
    socket: []const u8,
    api_version: []const u8,
    containers: []const api.Container,
    out: []Sample,
) void {
    var running: usize = 0;
    for (containers) |ct| {
        if (ct.isUp()) running += 1;
    }
    if (running == 0) return;

    const workers = @min(running, workers_max);
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
        }}) catch break;
        spawned += 1;
    }
    for (threads[0..spawned]) |thread| thread.join();
    // A refused thread must not silently drop a container's figures.
    for (spawned..workers) |i| work(.{
        .io = io,
        .socket = socket,
        .api_version = api_version,
        .containers = containers,
        .out = out,
        .first = i,
        .stride = workers,
    });
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
