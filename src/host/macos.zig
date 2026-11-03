//! The Mac, through Mach host statistics.
//!
//! Current macOS does not publish `vm.page_active_count` or `kern.cp_time`
//! through sysctl any more, so the numbers come from `host_statistics`, the
//! same call `vm_stat` and Activity Monitor make. The struct below is the
//! `vm_statistics64` prefix through `total_uncompressed_pages_in_compressor`
//! (38 integers, the REV1 count), which every macOS that has the 64-bit call
//! fills; later revisions only append fields we do not read.
const std = @import("std");

pub const Sample = struct {
    mem_total: u64 = 0,
    mem_used: u64 = 0,
    cpu_percent: f32 = 0,
    engine_rss: u64 = 0,
};

const mach_port_t = u32;
const kern_return_t = c_int;
const mach_msg_type_number_t = u32;

extern "c" fn sysctlbyname(
    name: [*:0]const u8,
    oldp: ?*anyopaque,
    oldlenp: ?*usize,
    newp: ?*anyopaque,
    newlen: usize,
) c_int;

extern "c" fn mach_host_self() mach_port_t;
extern "c" fn host_statistics(
    host: mach_port_t,
    flavor: c_int,
    info: ?*anyopaque,
    count: *mach_msg_type_number_t,
) kern_return_t;
extern "c" fn host_statistics64(
    host: mach_port_t,
    flavor: c_int,
    info: ?*anyopaque,
    count: *mach_msg_type_number_t,
) kern_return_t;

const HOST_CPU_LOAD_INFO: c_int = 3;
const HOST_VM_INFO64: c_int = 4;

/// `vm_statistics64` through `total_uncompressed_pages_in_compressor`.
const VmStats = extern struct {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
};

/// `HOST_VM_INFO64_REV1_COUNT`: the prefix this struct describes.
const vm_info_count: mach_msg_type_number_t = @sizeOf(VmStats) / @sizeOf(u32);

const CpuLoad = extern struct {
    cpu_ticks: [4]u32,
};

/// `HOST_CPU_LOAD_INFO_COUNT`.
const cpu_info_count: mach_msg_type_number_t = @sizeOf(CpuLoad) / @sizeOf(u32);

// CPU_STATE_USER, SYSTEM, IDLE, NICE, in the order the kernel returns them.
const cpu_user = 0;
const cpu_system = 1;
const cpu_idle = 2;
const cpu_nice = 3;

/// A sysctl that answers with one integer, of either 32 or 64 bits — macOS is
/// inconsistent about which, so the returned length decides.
fn intByName(name: [*:0]const u8) ?u64 {
    var buf: [8]u8 = @splat(0);
    var len: usize = buf.len;
    if (sysctlbyname(name, &buf, &len, null, 0) != 0) return null;
    return switch (len) {
        4 => std.mem.readInt(u32, buf[0..4], .little),
        8 => std.mem.readInt(u64, buf[0..8], .little),
        else => null,
    };
}

/// The CPU share between two tick snapshots. The counters are 32-bit and wrap
/// after roughly 497 days of uptime, so the difference is wrapping — a plain
/// subtraction would report one bogus idle frame at the wrap.
pub fn percentBetween(prev: [4]u32, now: [4]u32) f32 {
    const user = now[cpu_user] -% prev[cpu_user];
    const system = now[cpu_system] -% prev[cpu_system];
    const nice = now[cpu_nice] -% prev[cpu_nice];
    const idle = now[cpu_idle] -% prev[cpu_idle];
    const busy: u32 = user +% system +% nice;
    const total: u32 = busy +% idle;
    if (total == 0) return 0;
    return @as(f32, @floatFromInt(busy)) / @as(f32, @floatFromInt(total)) * 100.0;
}

pub const Sampler = struct {
    host: mach_port_t = 0,
    prev: ?[4]u32 = null,

    pub fn sample(self: *Sampler, io: std.Io, gpa: std.mem.Allocator) Sample {
        _ = io;
        _ = gpa;
        var out: Sample = .{};

        // A send right, kept for the life of the process: taking one per
        // sample would leak a port name every five seconds.
        if (self.host == 0) self.host = mach_host_self();

        out.mem_total = intByName("hw.memsize") orelse 0;

        // "In use" is active + wired + compressed: the pages the system cannot
        // hand out again without swapping. Free pages alone would count every
        // cached file as used, which is how memory monitors end up alarming.
        const page_size = intByName("hw.pagesize") orelse 4096;
        var vm: VmStats = std.mem.zeroes(VmStats);
        var vm_len = vm_info_count;
        if (host_statistics64(self.host, HOST_VM_INFO64, &vm, &vm_len) == 0) {
            const pages = @as(u64, vm.active_count) +
                @as(u64, vm.wire_count) +
                @as(u64, vm.compressor_page_count);
            out.mem_used = pages * page_size;
            if (out.mem_used > out.mem_total) out.mem_used = out.mem_total;
        }

        var cpu: CpuLoad = std.mem.zeroes(CpuLoad);
        var cpu_len = cpu_info_count;
        if (host_statistics(self.host, HOST_CPU_LOAD_INFO, &cpu, &cpu_len) == 0) {
            if (self.prev) |prev| out.cpu_percent = percentBetween(prev, cpu.cpu_ticks);
            self.prev = cpu.cpu_ticks;
        }
        return out;
    }
};

/// The engine's own host processes, matched by executable name. Guest-RAM
/// carriers are deliberately absent: on QEMU the VM's memory is the qemu
/// process, and on Virtualization.framework it is an XPC process no engine
/// labels. Counting either here would put the containers' guest memory in
/// "engine" as well, and the host bar subtracts containers separately.
pub const ENGINE_PROCESSES = [_][]const u8{
    "limactl",
    "com.docker.backend",
    "vpnkit",
    "gvproxy",
    "virtiofsd",
};

/// Null when `ps` cannot run: "unknown" must not render as zero bytes.
pub fn engineRss(io: std.Io, gpa: std.mem.Allocator) ?u64 {
    const result = std.process.run(gpa, io, .{
        .argv = &.{ "/bin/ps", "-axo", "rss=,comm=" },
    }) catch return null;
    defer gpa.free(result.stdout);
    defer gpa.free(result.stderr);
    if (!result.term.success()) return null;
    return parseEngineRss(result.stdout);
}

/// `ps -axo rss=,comm=` rows: KiB, then the executable. The name is the first
/// word's basename: `comm` is a path for ordinary processes, and some daemons
/// (the Lima SSH mux, for one) replace it with a title like "ssh: /… [mux]".
pub fn parseEngineRss(text: []const u8) u64 {
    var sum: u64 = 0;
    var lines = std.mem.splitScalar(u8, text, '\n');
    while (lines.next()) |line| {
        const trimmed = std.mem.trim(u8, line, " \r");
        if (trimmed.len == 0) continue;
        const space = std.mem.indexOfScalar(u8, trimmed, ' ') orelse continue;
        const kb = std.fmt.parseInt(u64, trimmed[0..space], 10) catch continue;
        const comm = std.mem.trim(u8, trimmed[space..], " ");
        const name = nameOf(comm);
        for (ENGINE_PROCESSES) |marker| {
            if (std.ascii.eqlIgnoreCase(name, marker)) {
                sum += kb * 1024;
                break;
            }
        }
    }
    return sum;
}

fn nameOf(comm: []const u8) []const u8 {
    const end = std.mem.indexOfScalar(u8, comm, ' ') orelse comm.len;
    return std.fs.path.basename(comm[0..end]);
}

test "cpu percent folds user, system and nice into busy" {
    const before = [4]u32{ 100, 50, 800, 50 };
    const after = [4]u32{ 300, 100, 1100, 50 };
    // 200 user + 50 system + 0 nice busy, 300 idle.
    try std.testing.expectApproxEqAbs(@as(f32, 45.45), percentBetween(before, after), 0.01);
}

test "cpu percent survives a counter wrap" {
    const before = [4]u32{ std.math.maxInt(u32) - 9, 0, 0, 0 };
    const after = [4]u32{ 10, 10, 80, 0 };
    // 20 user + 10 system busy, 80 idle.
    try std.testing.expectApproxEqAbs(@as(f32, 27.27), percentBetween(before, after), 0.01);
}

test "the vm statistics prefix is the REV1 count" {
    try std.testing.expectEqual(@as(mach_msg_type_number_t, 38), vm_info_count);
    try std.testing.expectEqual(@as(usize, 152), @sizeOf(VmStats));
}

test "engine rss sums the engine's host processes and ignores lookalikes" {
    const text =
        "  83008 /opt/homebrew/bin/limactl\n" ++
        " 264016 /System/Library/Frameworks/Virtualization.framework/com.apple.Virtualization.VirtualMachine\n" ++
        "   1472 /usr/bin/grep\n" ++
        " 102400 /usr/local/bin/qemu-system-aarch64\n" ++
        " 264000 /Applications/OrbStack.app/Contents/MacOS/OrbStack\n" ++
        "  2048 /Applications/Docker.app/Contents/MacOS/com.docker.backend\n" ++
        "  1024 /opt/homebrew/bin/vpnkit\n" ++
        "    12 not-a-number\n";
    // Guest-RAM carriers (the VZ XPC, qemu, OrbStack's VM) stay out: the host
    // bar counts container memory separately, so counting the VM's RAM here
    // would count the same bytes twice. Helpers are matched by name exactly.
    try std.testing.expectEqual(@as(u64, (83008 + 2048 + 1024) * 1024), parseEngineRss(text));
    try std.testing.expectEqual(@as(u64, 0), parseEngineRss(""));
}

test "a titled process is matched by the first word, not its arguments" {
    const text = "  4096 ssh: /Users/x/.lima/berth/ssh.sock [mux]\n  8192 limactl hostagent berth\n";
    // The SSH mux is not an engine process; the hostagent's title keeps limactl.
    try std.testing.expectEqual(@as(u64, 8192 * 1024), parseEngineRss(text));
}
