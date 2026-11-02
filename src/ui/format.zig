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
    return c.print("{d:.1} {s}", .{ value, UNITS[unit] });
}

/// The same figure into a caller's buffer, for places without a frame context
/// (a toast built in a callback, for instance).
pub fn bytesBuf(buf: []u8, n: u64) []const u8 {
    var value: f64 = @floatFromInt(n);
    var unit: usize = 0;
    while (value >= 1024 and unit + 1 < UNITS.len) : (unit += 1) value /= 1024;
    if (unit == 0) return std.fmt.bufPrint(buf, "{d} B", .{n}) catch "?";
    return std.fmt.bufPrint(buf, "{d:.1} {s}", .{ value, UNITS[unit] }) catch "?";
}

/// A signed byte figure, for reclaimable totals.
pub fn bytesSigned(c: *ui.Context, n: i64) []const u8 {
    if (n < 0) return c.print("-{s}", .{bytes(c, @as(u64, @intCast(-n)))});
    return bytes(c, @as(u64, @intCast(n)));
}

/// Percentages keep one decimal; CPU may exceed 100% on several cores, which
/// is correct rather than clamped.
pub fn percent(c: *ui.Context, v: f64) []const u8 {
    if (v >= 100) return c.print("{d:.0}%", .{v});
    return c.print("{d:.1}%", .{v});
}

/// `12s`, `4m 12s`, `3h 04m`, `2d 06h` — precise where it matters, coarse
/// where it would only add noise.
pub fn duration(c: *ui.Context, seconds: i64) []const u8 {
    const s = @max(@as(i64, 0), seconds);
    if (s < 60) return c.print("{d}s", .{s});
    if (s < 3600) return c.print("{d}m {d:0>2}s", .{ @divTrunc(s, 60), @mod(s, 60) });
    if (s < 86400) return c.print("{d}h {d:0>2}m", .{ @divTrunc(s, 3600), @divTrunc(@mod(s, 3600), 60) });
    return c.print("{d}d {d:0>2}h", .{ @divTrunc(s, 86400), @divTrunc(@mod(s, 86400), 3600) });
}

/// How long ago, from a Unix timestamp in seconds.
pub fn ago(c: *ui.Context, unix_seconds: i64, now_seconds: i64) []const u8 {
    return duration(c, now_seconds - unix_seconds);
}

extern "c" fn time(t: ?*c_long) c_long;

/// Seconds since the epoch, from the system's wall clock.
pub fn now() i64 {
    return @intCast(time(null));
}

/// `14:32:07`, so a view can say "as of" without a date library.
pub fn clock(c: *ui.Context, unix_seconds: i64) []const u8 {
    const epoch = std.time.epoch.EpochSeconds{ .secs = @intCast(@max(@as(i64, 0), unix_seconds)) };
    const ds = epoch.getDaySeconds();
    return c.print("{d:0>2}:{d:0>2}:{d:0>2}", .{
        ds.getHoursIntoDay(),
        ds.getMinutesIntoHour(),
        ds.getSecondsIntoMinute(),
    });
}

// --- The same rules into a caller's buffer, for tests and non-UI callers. ---

pub fn bytesInto(buf: []u8, n: anytype) []const u8 {
    const v: f64 = @floatFromInt(@max(@as(@TypeOf(n), 0), n));
    var value = v;
    var unit: usize = 0;
    while (value >= 1024 and unit + 1 < UNITS.len) : (unit += 1) value /= 1024;
    if (unit == 0) return std.fmt.bufPrint(buf, "{d} B", .{@as(u64, @intFromFloat(value))}) catch "—";
    return std.fmt.bufPrint(buf, "{d:.1} {s}", .{ value, UNITS[unit] }) catch "—";
}

pub fn durationInto(buf: []u8, seconds: i64) []const u8 {
    const s = @max(@as(i64, 0), seconds);
    if (s < 60) return std.fmt.bufPrint(buf, "{d}s", .{s}) catch "—";
    if (s < 3600) return std.fmt.bufPrint(buf, "{d}m {d:0>2}s", .{ @divTrunc(s, 60), @mod(s, 60) }) catch "—";
    if (s < 86400) return std.fmt.bufPrint(buf, "{d}h {d:0>2}m", .{ @divTrunc(s, 3600), @divTrunc(@mod(s, 3600), 60) }) catch "—";
    return std.fmt.bufPrint(buf, "{d}d {d:0>2}h", .{ @divTrunc(s, 86400), @divTrunc(@mod(s, 86400), 3600) }) catch "—";
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
    try std.testing.expectEqualStrings("1d 00h", durationInto(&b, 86400));
    try std.testing.expectEqualStrings("2d 06h", durationInto(&b, 194400));
}
