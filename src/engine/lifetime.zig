//! A container's lifetime facts that the list endpoint only tells in prose:
//! how long it has been up, and (from inspect) when it started and how many
//! times it restarted. Docker composes these server-side, in English, in the
//! human `Status` string — parsing it is cheaper and fresher than one inspect
//! request per row per refresh.
const std = @import("std");

fn eql(a: []const u8, b: []const u8) bool {
    return std.mem.eql(u8, a, b);
}

/// Seconds the container has been up, from a Status like "Up 2 hours (healthy)".
/// Null when it is not up ("Exited …", "Restarting …", "Created") — no value is
/// better than a wrong one.
pub fn upSeconds(status: []const u8) ?i64 {
    if (!std.mem.startsWith(u8, status, "Up")) return null;
    const rest = status[2..];
    if (rest.len == 0 or rest[0] != ' ') return null;

    // Docker capitalizes these: the phrase begins the status string.
    if (std.ascii.startsWithIgnoreCase(rest, " about a minute")) return 60;
    if (std.ascii.startsWithIgnoreCase(rest, " about an hour")) return 3600;
    if (std.ascii.startsWithIgnoreCase(rest, " less than a second")) return 0;

    var seconds: i64 = 0;
    var any = false;
    var it = std.mem.tokenizeScalar(u8, rest, ' ');
    while (it.next()) |token| {
        const n = std.fmt.parseInt(i64, token, 10) catch continue;
        const unit = it.next() orelse return null;
        const unit_trimmed = std.mem.trimEnd(u8, unit, ",)");
        const mult: i64 = if (eql(unit_trimmed, "second") or eql(unit_trimmed, "seconds")) 1 else if (eql(unit_trimmed, "minute") or eql(unit_trimmed, "minutes")) 60 else if (eql(unit_trimmed, "hour") or eql(unit_trimmed, "hours")) 3600 else if (eql(unit_trimmed, "day") or eql(unit_trimmed, "days")) 86400 else if (eql(unit_trimmed, "week") or eql(unit_trimmed, "weeks")) 7 * 86400 else if (eql(unit_trimmed, "month") or eql(unit_trimmed, "months")) 30 * 86400 else if (eql(unit_trimmed, "year") or eql(unit_trimmed, "years")) 365 * 86400 else return null;
        seconds += n * mult;
        any = true;
    }
    return if (any) seconds else null;
}

/// Started-at and restart count from the inspect document, for the detail
/// pane. `StartedAt` is RFC 3339 UTC; "0001-01-01T00:00:00Z" means never
/// started, which reads as null here.
pub const Lifetime = struct {
    started_at: ?i64 = null,
    restarts: u32 = 0,
};

pub fn fromInspect(gpa: std.mem.Allocator, raw: []const u8) Lifetime {
    var out: Lifetime = .{};
    const Doc = struct {
        State: struct {
            StartedAt: []const u8 = "",
            RestartCount: i64 = 0,
        } = .{},
    };
    var parsed = std.json.parseFromSlice(Doc, gpa, raw, .{ .ignore_unknown_fields = true }) catch return out;
    defer parsed.deinit();
    out.restarts = @intCast(@max(@as(i64, 0), parsed.value.State.RestartCount));
    out.started_at = rfc3339(parsed.value.State.StartedAt);
    return out;
}

/// RFC 3339 as Docker writes it: 2026-11-02T09:41:22.501745461Z. Fractional
/// seconds are dropped; the offset, when present, is applied.
pub fn rfc3339(text: []const u8) ?i64 {
    if (text.len < 19) return null;
    if (text[4] != '-' or text[7] != '-' or text[10] != 'T' or text[13] != ':' or text[16] != ':') return null;
    const year = std.fmt.parseInt(u32, text[0..4], 10) catch return null;
    const month = std.fmt.parseInt(u32, text[5..7], 10) catch return null;
    const day = std.fmt.parseInt(u32, text[8..10], 10) catch return null;
    const hour = std.fmt.parseInt(u32, text[11..13], 10) catch return null;
    const minute = std.fmt.parseInt(u32, text[14..16], 10) catch return null;
    const second = std.fmt.parseInt(u32, text[17..19], 10) catch return null;

    // Docker's zero value means "never started".
    if (year == 1 and month == 1 and day == 1 and hour == 0) return null;

    const days = daysFromCivil(year, month, day) catch return null;
    var epoch: i64 = days * 86400;
    epoch += @as(i64, hour) * 3600 + @as(i64, minute) * 60 + second;

    var rest = text[19..];
    if (rest.len > 0 and rest[0] == '.') {
        var i: usize = 1;
        while (i < rest.len and std.ascii.isDigit(rest[i])) i += 1;
        rest = rest[i..];
    }
    if (rest.len > 0 and (rest[0] == '+' or rest[0] == '-')) {
        const sign: i64 = if (rest[0] == '-') -1 else 1;
        if (rest.len >= 6 and rest[3] == ':') {
            const oh = std.fmt.parseInt(i64, rest[1..3], 10) catch 0;
            const om = std.fmt.parseInt(i64, rest[4..6], 10) catch 0;
            epoch -= sign * (oh * 3600 + om * 60);
        }
    }
    return epoch;
}

fn daysFromCivil(y_in: u32, m: u32, d: u32) !i64 {
    if (m < 1 or m > 12 or d < 1 or d > 31) return error.Invalid;
    const y: i64 = @intCast(y_in);
    // Howard Hinnant's civil-from-days, inverted; valid for the Gregorian
    // calendar over any range Docker can print.
    const yy: i64 = if (m <= 2) y - 1 else y;
    const era = @divFloor(yy, 400);
    const yoe = yy - era * 400;
    const mp: i64 = @intCast((m + 9) % 12);
    const doy = @divFloor(153 * mp + 2, 5) + @as(i64, @intCast(d)) - 1;
    const doe = yoe * 365 + @divFloor(yoe, 4) - @divFloor(yoe, 100) + doy;
    return era * 146097 + doe - 719468;
}

test "uptime reads from the English status the engine composes" {
    try std.testing.expectEqual(@as(?i64, 7200), upSeconds("Up 2 hours"));
    try std.testing.expectEqual(@as(?i64, 259200), upSeconds("Up 3 days (healthy)"));
    // Docker capitalizes its fuzzy phrases; the engine sends them that way.
    try std.testing.expectEqual(@as(?i64, 60), upSeconds("Up About a minute"));
    try std.testing.expectEqual(@as(?i64, 3600), upSeconds("Up About an hour"));
    try std.testing.expectEqual(@as(?i64, 0), upSeconds("Up Less than a second"));
    try std.testing.expectEqual(@as(?i64, 14 * 86400), upSeconds("Up 2 weeks"));
    try std.testing.expectEqual(@as(?i64, 5 * 7 * 86400), upSeconds("Up 5 weeks (healthy)"));
    try std.testing.expectEqual(@as(?i64, 60 * 86400), upSeconds("Up 2 months"));
    try std.testing.expectEqual(@as(?i64, 2 * 365 * 86400), upSeconds("Up 2 years"));
    try std.testing.expectEqual(@as(?i64, 3660), upSeconds("Up 1 hour 1 minute"));
    try std.testing.expectEqual(@as(?i64, 0), upSeconds("Up 0 seconds"));

    try std.testing.expect(upSeconds("Exited (0) 2 minutes ago") == null);
    try std.testing.expect(upSeconds("Restarting (1) 5 seconds ago") == null);
    try std.testing.expect(upSeconds("Created") == null);
}

test "rfc3339 parses docker timestamps and rejects the never-started zero" {
    try std.testing.expectEqual(@as(?i64, 1790100800), rfc3339("2026-09-22T18:13:20Z"));
    try std.testing.expect(rfc3339("2026-09-22T18:13:20.501745461Z") != null);
    try std.testing.expectEqual(@as(?i64, 1790093600), rfc3339("2026-09-22T18:13:20+02:00"));
    try std.testing.expectEqual(@as(?i64, 1790093600), rfc3339("2026-09-22T18:13:20.5+02:00"));
    try std.testing.expectEqual(@as(?i64, 1709164800), rfc3339("2024-02-29T00:00:00Z"));
    try std.testing.expect(rfc3339("0001-01-01T00:00:00Z") == null);
    try std.testing.expect(rfc3339("not a date") == null);
}

test "inspect carries restarts and a start time" {
    const doc =
        \\{"State":{"Status":"running","StartedAt":"2026-09-22T18:13:20Z","RestartCount":3}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    try std.testing.expectEqual(@as(u32, 3), life.restarts);
    try std.testing.expect(life.started_at != null);
}
