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
    mem: u64 = 0,
    limit: u64 = 0,
    net_rx: u64 = 0,
    net_tx: u64 = 0,
    blk_read: u64 = 0,
    blk_write: u64 = 0,
    pids: ?u32 = null,
};

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
        const net = st.netTotals();
        const disk = st.diskTotals();
        job.out[i] = .{
            .id = ct.Id,
            .cpu = st.cpuPercent(),
            .mem = st.memUsed(),
            .limit = st.memLimit(),
            .net_rx = net.rx,
            .net_tx = net.tx,
            .blk_read = disk.read,
            .blk_write = disk.write,
            .pids = st.pidsCurrent(),
        };
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
