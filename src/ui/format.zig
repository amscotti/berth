//! The one place numbers and times become text.
//!
//! Every view calls these, so a byte count reads the same on the Overview as
//! in a container row, and a duration never becomes `2h3m4.5s` in one place
//! and `2 hours` in another.
//!
//! Strings are allocated in the frame's arena by `Context.print`: the frame is
//! painted after the view function returns, so text must outlive the stack.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;

const UNITS = [_][]const u8{ "B", "KiB", "MiB", "GiB", "TiB", "PiB" };

/// `1.4 GiB`, `812 KiB`, `0 B`. Binary units, one decimal above a kilobyte.
pub fn bytes(c: *ui.Context, n: anytype) []const u8 {
    const v: f64 = @floatFromInt(@max(@as(@TypeOf(n), 0), n));
    var value = v;
    var unit: usize = 0;
    while (value >= 1024 and unit + 1 < UNITS.len) : (unit += 1) value /= 1024;
    if (unit == 0) return c.print("{d} B", .{@as(u64, @intFromFloat(value))});
    if (value >= 100) return c.print("{d:.0} {s}", .{ value, UNITS[unit] });
    return c.print("{d:.1} {s}", .{ value, UNITS[unit] });
}

/// The same figure into a caller's buffer, for places without a frame context
/// (a toast built in a callback, for instance).
pub fn bytesBuf(buf: []u8, n: u64) []const u8 {
    var value: f64 = @floatFromInt(n);
    var unit: usize = 0;
    while (value >= 1024 and unit + 1 < UNITS.len) : (unit += 1) value /= 1024;
    if (unit == 0) return std.fmt.bufPrint(buf, "{d} B", .{n}) catch "?";
    if (value >= 100) return std.fmt.bufPrint(buf, "{d:.0} {s}", .{ value, UNITS[unit] }) catch "?";
    return std.fmt.bufPrint(buf, "{d:.1} {s}", .{ value, UNITS[unit] }) catch "?";
}

/// A signed byte figure, for reclaimable totals.
pub fn bytesSigned(c: *ui.Context, n: i64) []const u8 {
    if (n < 0) {
        // `-minInt` is not representable as i64; take the magnitude in u64.
        const magnitude: u64 = if (n == std.math.minInt(i64))
            @as(u64, std.math.maxInt(i64)) + 1
        else
            @intCast(-n);
        return c.print("-{s}", .{bytes(c, magnitude)});
    }
    return bytes(c, @as(u64, @intCast(n)));
}

/// Percentages keep one decimal; CPU may exceed 100% on several cores, which
/// is correct rather than clamped.
pub fn percent(c: *ui.Context, v: f64) []const u8 {
    if (v >= 100) return c.print("{d:.0}%", .{v});
    return c.print("{d:.1}%", .{v});
}

/// `12s`, `4m 12s`, `3h 04m`, `2d 06h` — precise where it matters, and with
/// zero trailing fields dropped so a table never says `15h 00m`.
pub fn duration(c: *ui.Context, seconds: i64) []const u8 {
    const s = @max(@as(i64, 0), seconds);
    if (s < 60) return c.print("{d}s", .{s});
    if (s < 3600) {
        const rem = @mod(s, 60);
        if (rem == 0) return c.print("{d}m", .{@divTrunc(s, 60)});
        return c.print("{d}m {d:0>2}s", .{ @divTrunc(s, 60), rem });
    }
    if (s < 86400) {
        const rem = @divTrunc(@mod(s, 3600), 60);
        if (rem == 0) return c.print("{d}h", .{@divTrunc(s, 3600)});
        return c.print("{d}h {d:0>2}m", .{ @divTrunc(s, 3600), rem });
    }
    const rem = @divTrunc(@mod(s, 86400), 3600);
    if (rem == 0) return c.print("{d}d", .{@divTrunc(s, 86400)});
    return c.print("{d}d {d:0>2}h", .{ @divTrunc(s, 86400), rem });
}

/// How long ago, from a Unix timestamp in seconds.
pub fn ago(c: *ui.Context, unix_seconds: i64, now_seconds: i64) []const u8 {
    // Either endpoint can be the extreme an engine chooses to report. The
    // saturating difference keeps `now - created` from wrapping; duration
    // already clamps the negative end.
    return duration(c, now_seconds -| unix_seconds);
}

extern "c" fn time(t: ?*c_long) c_long;

/// Seconds since the epoch, from the system's wall clock.
pub fn now() i64 {
    return @intCast(time(null));
}

/// libc's broken-down local time. Docker hands us UTC, but a person reads the
/// clock on their desk: "updated 17:23" five hours off is worse than no time.
const Tm = extern struct {
    tm_sec: c_int,
    tm_min: c_int,
    tm_hour: c_int,
    tm_mday: c_int,
    tm_mon: c_int,
    tm_year: c_int,
    tm_wday: c_int,
    tm_yday: c_int,
    tm_isdst: c_int,
    tm_gmtoff: c_long,
    tm_zone: ?[*:0]const u8,
};

extern "c" fn localtime_r(timep: *const c_long, result: *Tm) ?*Tm;

fn localTm(unix_seconds: i64) ?Tm {
    var secs: c_long = @intCast(@max(@as(i64, 0), unix_seconds));
    var out: Tm = undefined;
    return if (localtime_r(&secs, &out) == null) null else out;
}

/// `14:32:07`, so a view can say "as of" without a date library.
pub fn clock(c: *ui.Context, unix_seconds: i64) []const u8 {
    return clockFrom(c, localTm(unix_seconds) orelse return "—");
}

/// The same clock into a caller's buffer, for code without a frame context.
pub fn clockBuf(buf: []u8, unix_seconds: i64) []const u8 {
    const tm = localTm(unix_seconds) orelse return "—";
    return std.fmt.bufPrint(buf, "{d:0>2}:{d:0>2}:{d:0>2}", .{
        @as(u32, @intCast(tm.tm_hour)),
        @as(u32, @intCast(tm.tm_min)),
        @as(u32, @intCast(tm.tm_sec)),
    }) catch "—";
}

fn clockFrom(c: *ui.Context, tm: Tm) []const u8 {
    return c.print("{d:0>2}:{d:0>2}:{d:0>2}", .{
        @as(u32, @intCast(tm.tm_hour)),
        @as(u32, @intCast(tm.tm_min)),
        @as(u32, @intCast(tm.tm_sec)),
    });
}

const MONTHS = [_][]const u8{ "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" };

/// When something started: `15:56` today, `Oct 28 15:56` otherwise. The
/// inspector already has "Up 2 hours" in the engine's status; this is the
/// fact that status cannot give — the wall-clock moment.
pub fn started(c: *ui.Context, unix_seconds: i64, now_seconds: i64) []const u8 {
    const tm = localTm(unix_seconds) orelse return "—";
    const now_tm = localTm(now_seconds) orelse tm;
    return stampFrom(c, tm, now_tm);
}

fn stampFrom(c: *ui.Context, tm: Tm, now_tm: Tm) []const u8 {
    const clock_text = c.print("{d:0>2}:{d:0>2}", .{
        @as(u32, @intCast(tm.tm_hour)),
        @as(u32, @intCast(tm.tm_min)),
    });
    if (tm.tm_year == now_tm.tm_year and tm.tm_mon == now_tm.tm_mon and tm.tm_mday == now_tm.tm_mday) return clock_text;
    return c.print("{s} {d} {s}", .{
        MONTHS[@intCast(tm.tm_mon)],
        @as(u32, @intCast(tm.tm_mday)),
        clock_text,
    });
}

// --- The same rules into a caller's buffer, for tests and non-UI callers. ---

pub fn bytesInto(buf: []u8, n: anytype) []const u8 {
    const v: f64 = @floatFromInt(@max(@as(@TypeOf(n), 0), n));
    var value = v;
    var unit: usize = 0;
    while (value >= 1024 and unit + 1 < UNITS.len) : (unit += 1) value /= 1024;
    if (unit == 0) return std.fmt.bufPrint(buf, "{d} B", .{@as(u64, @intFromFloat(value))}) catch "—";
    if (value >= 100) return std.fmt.bufPrint(buf, "{d:.0} {s}", .{ value, UNITS[unit] }) catch "—";
    return std.fmt.bufPrint(buf, "{d:.1} {s}", .{ value, UNITS[unit] }) catch "—";
}

pub fn durationInto(buf: []u8, seconds: i64) []const u8 {
    const s = @max(@as(i64, 0), seconds);
    if (s < 60) return std.fmt.bufPrint(buf, "{d}s", .{s}) catch "—";
    if (s < 3600) {
        const rem = @mod(s, 60);
        if (rem == 0) return std.fmt.bufPrint(buf, "{d}m", .{@divTrunc(s, 60)}) catch "—";
        return std.fmt.bufPrint(buf, "{d}m {d:0>2}s", .{ @divTrunc(s, 60), rem }) catch "—";
    }
    if (s < 86400) {
        const rem = @divTrunc(@mod(s, 3600), 60);
        if (rem == 0) return std.fmt.bufPrint(buf, "{d}h", .{@divTrunc(s, 3600)}) catch "—";
        return std.fmt.bufPrint(buf, "{d}h {d:0>2}m", .{ @divTrunc(s, 3600), rem }) catch "—";
    }
    const rem = @divTrunc(@mod(s, 86400), 3600);
    if (rem == 0) return std.fmt.bufPrint(buf, "{d}d", .{@divTrunc(s, 86400)}) catch "—";
    return std.fmt.bufPrint(buf, "{d}d {d:0>2}h", .{ @divTrunc(s, 86400), rem }) catch "—";
}

pub fn percentInto(buf: []u8, v: f64) []const u8 {
    if (v >= 100) return std.fmt.bufPrint(buf, "{d:.0}%", .{v}) catch "—";
    return std.fmt.bufPrint(buf, "{d:.1}%", .{v}) catch "—";
}

test "bytes read as binary units" {
    var b: [32]u8 = undefined;
    try std.testing.expectEqualStrings("0 B", bytesInto(&b, @as(u64, 0)));
    try std.testing.expectEqualStrings("512 B", bytesInto(&b, @as(u64, 512)));
    try std.testing.expectEqualStrings("1.0 KiB", bytesInto(&b, @as(u64, 1024)));
    try std.testing.expectEqualStrings("1.5 GiB", bytesInto(&b, @as(u64, 1610612736)));
    try std.testing.expectEqualStrings("2.0 TiB", bytesInto(&b, @as(u64, 2199023255552)));
}

test "durations stay short" {
    var b: [32]u8 = undefined;
    try std.testing.expectEqualStrings("0s", durationInto(&b, 0));
    try std.testing.expectEqualStrings("59s", durationInto(&b, 59));
    try std.testing.expectEqualStrings("4m 12s", durationInto(&b, 252));
    try std.testing.expectEqualStrings("3h 04m", durationInto(&b, 11040));
    try std.testing.expectEqualStrings("2d 06h", durationInto(&b, 194400));
    // A whole unit is not followed by a zero field.
    try std.testing.expectEqualStrings("1m", durationInto(&b, 60));
    try std.testing.expectEqualStrings("15h", durationInto(&b, 15 * 3600));
    try std.testing.expectEqualStrings("2d", durationInto(&b, 2 * 86400));
}

test "a start time is a clock today, a date before that" {
    var c: ui.Context = undefined;
    c.gpa = std.testing.allocator;
    c.text = undefined;
    c.arena = .init(std.testing.allocator);
    defer c.arena.deinit();

    const tm: Tm = .{
        .tm_sec = 20,
        .tm_min = 13,
        .tm_hour = 18,
        .tm_mday = 22,
        .tm_mon = 8, // September
        .tm_year = 126, // 2026
        .tm_wday = 0,
        .tm_yday = 0,
        .tm_isdst = 0,
        .tm_gmtoff = 0,
        .tm_zone = null,
    };
    var now_tm = tm;
    try std.testing.expectEqualStrings("18:13", stampFrom(&c, tm, now_tm));
    now_tm.tm_mday = 23;
    try std.testing.expectEqualStrings("Sep 22 18:13", stampFrom(&c, tm, now_tm));
}

test "percent keeps a decimal below a hundred" {
    var b: [32]u8 = undefined;
    try std.testing.expectEqualStrings("0.4%", percentInto(&b, 0.42));
    try std.testing.expectEqualStrings("142%", percentInto(&b, 142.4));
}

test "bytesBuf formats the same units without a frame context" {
    var b: [32]u8 = undefined;
    try std.testing.expectEqualStrings("0 B", bytesBuf(&b, 0));
    try std.testing.expectEqualStrings("1.5 KiB", bytesBuf(&b, 1536));
    try std.testing.expectEqualStrings("1.5 GiB", bytesBuf(&b, 1610612736));
}

test "percent switches to a whole number at one hundred" {
    var b: [32]u8 = undefined;
    try std.testing.expectEqualStrings("99.9%", percentInto(&b, 99.94));
    try std.testing.expectEqualStrings("100%", percentInto(&b, 100));
    try std.testing.expectEqualStrings("250%", percentInto(&b, 250));
}

test "a day and beyond keeps two fields" {
    var b: [32]u8 = undefined;
    try std.testing.expectEqualStrings("1d", durationInto(&b, 86400));
    try std.testing.expectEqualStrings("2d 06h", durationInto(&b, 194400));
    // The negative end of the clock is a zero age, not an overflow.
    try std.testing.expectEqualStrings("0s", durationInto(&b, std.math.minInt(i64)));
}

test "signed bytes handle the minimum integer" {
    var c: ui.Context = undefined;
    c.gpa = std.testing.allocator;
    c.text = undefined;
    c.arena = .init(std.testing.allocator);
    defer c.arena.deinit();

    try std.testing.expectEqualStrings("-8192 PiB", bytesSigned(&c, std.math.minInt(i64)));
    try std.testing.expectEqualStrings("8192 PiB", bytesSigned(&c, std.math.maxInt(i64)));
    try std.testing.expectEqualStrings("1.5 KiB", bytesSigned(&c, 1536));
}

test "an age at the ends of the clock saturates" {
    var c: ui.Context = undefined;
    c.gpa = std.testing.allocator;
    c.text = undefined;
    c.arena = .init(std.testing.allocator);
    defer c.arena.deinit();

    try std.testing.expectEqualStrings("50s", ago(&c, 50, 100));
    const huge = ago(&c, std.math.minInt(i64), std.math.maxInt(i64));
    try std.testing.expect(std.mem.indexOfScalar(u8, huge, 'd') != null);
    try std.testing.expectEqualStrings("0s", ago(&c, 200, 100));
}
