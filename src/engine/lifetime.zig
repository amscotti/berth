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
    /// `docker run …` reproducing this container, from the same inspect.
    /// Null when the inspect could not be read; freed by the caller's state.
    run_cmd: ?[]u8 = null,
};

/// The same inspect, read for the facts the pane shows and for a runnable
/// `docker run` line. Built while the parsed document is alive; nothing here
/// survives the arena but what is copied out.
pub fn fromInspect(gpa: std.mem.Allocator, raw: []const u8) Lifetime {
    var out: Lifetime = .{};
    const Mount = struct {
        Type: []const u8 = "",
        Name: []const u8 = "",
        Source: []const u8 = "",
        Destination: []const u8 = "",
        RW: bool = true,
    };
    const Doc = struct {
        Name: []const u8 = "",
        Config: struct {
            Image: []const u8 = "",
            Env: ?[]const []const u8 = null,
        } = .{},
        HostConfig: struct {
            PortBindings: ?std.json.Value = null,
        } = .{},
        Mounts: ?[]const Mount = null,
        State: struct {
            StartedAt: []const u8 = "",
            RestartCount: i64 = 0,
        } = .{},
    };
    var parsed = std.json.parseFromSlice(Doc, gpa, raw, .{ .ignore_unknown_fields = true, .allocate = .alloc_always }) catch return out;
    defer parsed.deinit();
    const doc = parsed.value;
    out.restarts = @intCast(@max(@as(i64, 0), doc.State.RestartCount));
    out.started_at = rfc3339(doc.State.StartedAt);
    out.run_cmd = runCommand(gpa, doc) catch null;
    return out;
}

/// `docker run -d --name X … image` — the flags a person actually reuses.
/// Compose labels and networks are left out: a compose service is reproduced
/// by `docker compose up`, not by pasting its labels. PATH is filtered from
/// the environment because the image contributes it; everything the user
/// asked for is kept. Fails (returns null) only when memory runs out.
fn runCommand(gpa: std.mem.Allocator, doc: anytype) !?[]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    const w = &out;

    try w.appendSlice(gpa, "docker run -d");
    const name = if (doc.Name.len > 0 and doc.Name[0] == '/') doc.Name[1..] else doc.Name;
    if (name.len > 0) {
        try w.appendSlice(gpa, " --name ");
        try appendQuoted(gpa, w, name);
    }

    // Port bindings: the key is "containerPort/proto" and the value is every
    // host binding for it. A binding on a specific address keeps that address
    // — widening a loopback bind to every interface is not a copy.
    if (doc.HostConfig.PortBindings) |bindings| {
        if (bindings == .object) {
            var it = bindings.object.iterator();
            while (it.next()) |entry| {
                var key = entry.key_ptr.*;
                var proto: []const u8 = "tcp";
                if (std.mem.indexOfScalar(u8, key, '/')) |slash| {
                    proto = key[slash + 1 ..];
                    key = key[0..slash];
                }
                const hosts = entry.value_ptr.*;
                if (hosts != .array) continue;
                for (hosts.array.items) |binding| {
                    if (binding != .object) continue;
                    const port = binding.object.get("HostPort") orelse continue;
                    if (port != .string or port.string.len == 0) continue;
                    try w.appendSlice(gpa, " -p ");
                    const ip_value = binding.object.get("HostIp");
                    if (ip_value != null and ip_value.? == .string and ip_value.?.string.len > 0 and
                        !std.mem.eql(u8, ip_value.?.string, "0.0.0.0") and !std.mem.eql(u8, ip_value.?.string, "::"))
                    {
                        const ip = ip_value.?.string;
                        if (std.mem.indexOfScalar(u8, ip, ':') != null) {
                            try w.append(gpa, '[');
                            try w.appendSlice(gpa, ip);
                            try w.append(gpa, ']');
                        } else {
                            try w.appendSlice(gpa, ip);
                        }
                        try w.append(gpa, ':');
                    }
                    try w.appendSlice(gpa, port.string);
                    try w.append(gpa, ':');
                    try w.appendSlice(gpa, key);
                    if (!std.mem.eql(u8, proto, "tcp")) {
                        try w.append(gpa, '/');
                        try w.appendSlice(gpa, proto);
                    }
                }
            }
        }
    }

    if (doc.Config.Env) |env| {
        for (env) |pair| {
            if (std.mem.startsWith(u8, pair, "PATH=")) continue;
            try w.appendSlice(gpa, " -e ");
            try appendQuoted(gpa, w, pair);
        }
    }

    if (doc.Mounts) |mounts| {
        for (mounts) |m| {
            // tmpfs has no -v spelling and an anonymous volume has no name to
            // reference. A bind with a ':' in either path cannot be written as
            // `-v` (docker splits on it) — skipped rather than written wrong.
            if (std.mem.eql(u8, m.Type, "bind")) {
                if (m.Source.len == 0 or m.Destination.len == 0) continue;
                if (std.mem.indexOfScalar(u8, m.Source, ':') != null or
                    std.mem.indexOfScalar(u8, m.Destination, ':') != null) continue;
                try w.appendSlice(gpa, " -v ");
                try appendQuoted(gpa, w, m.Source);
                try w.append(gpa, ':');
                try appendQuoted(gpa, w, m.Destination);
                if (!m.RW) try w.appendSlice(gpa, ":ro");
            } else if (std.mem.eql(u8, m.Type, "volume")) {
                if (m.Name.len == 0 or m.Destination.len == 0) continue;
                if (std.mem.indexOfScalar(u8, m.Destination, ':') != null) continue;
                try w.appendSlice(gpa, " -v ");
                try appendQuoted(gpa, w, m.Name);
                try w.append(gpa, ':');
                try appendQuoted(gpa, w, m.Destination);
                if (!m.RW) try w.appendSlice(gpa, ":ro");
            }
        }
    }

    try w.appendSlice(gpa, " ");
    try appendQuoted(gpa, w, doc.Config.Image);
    return try out.toOwnedSlice(gpa);
}

/// Words are quoted unless every byte is shell-safe: quoting only on a space
/// lets a lone apostrophe or a tab through unquoted, which a shell would read
/// as two words — for an `-e` value that silently swaps the image.
fn appendQuoted(gpa: std.mem.Allocator, w: *std.ArrayList(u8), text: []const u8) !void {
    const safe = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_@%+=:,./-";
    var needs_quotes = false;
    for (text) |ch| {
        if (std.mem.indexOfScalar(u8, safe, ch) == null) {
            needs_quotes = true;
            break;
        }
    }
    if (!needs_quotes) {
        try w.appendSlice(gpa, text);
        return;
    }
    try w.append(gpa, '\'');
    for (text) |ch| {
        if (ch == '\'') {
            // Close the quote, escape the apostrophe, reopen.
            try w.appendSlice(gpa, "'\\''");
            continue;
        }
        try w.append(gpa, ch);
    }
    try w.append(gpa, '\'');
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
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqual(@as(u32, 3), life.restarts);
    try std.testing.expect(life.started_at != null);
}

test "the run command reproduces ports, env and mounts, and skips the noise" {
    const doc =
        \\{"Name":"/shop-web-1","Config":{"Image":"nginx:alpine","Env":["PATH=/usr/bin","TOKEN=abc def","MODE=prod","APOSTROPHE=it's"]},
        \\ "HostConfig":{"PortBindings":{"80/tcp":[{"HostIp":"","HostPort":"8080"},{"HostIp":"127.0.0.1","HostPort":"8081"}],
        \\                                "53/udp":[{"HostIp":"","HostPort":"1053"}],
        \\                                "53/sctp":[{"HostIp":"","HostPort":"1054"}],
        \\                                "9000/tcp":[{"HostIp":"","HostPort":""}]}},
        \\ "Mounts":[{"Type":"bind","Source":"/Users/x/site","Destination":"/usr/share/nginx/html","RW":false},
        \\            {"Type":"volume","Name":"shop-data","Destination":"/data"},
        \\            {"Type":"bind","Source":"/srv/a:b","Destination":"/wrong"},
        \\            {"Type":"tmpfs","Destination":"/tmp"}],
        \\ "State":{"StartedAt":"2026-09-22T18:13:20Z","RestartCount":1}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    const cmd = life.run_cmd.?;
    defer std.testing.allocator.free(cmd);
    try std.testing.expectEqualStrings(
        "docker run -d --name shop-web-1 -p 8080:80 -p 127.0.0.1:8081:80 -p 1053:53/udp -p 1054:53/sctp -e 'TOKEN=abc def' -e MODE=prod -e 'APOSTROPHE=it'\\''s' -v /Users/x/site:/usr/share/nginx/html:ro -v shop-data:/data nginx:alpine",
        cmd,
    );
    try std.testing.expectEqual(@as(u32, 1), life.restarts);
}

test "a bare container copies as one line with no empty flags" {
    const doc =
        \\{"Name":"/solo","Config":{"Image":"alpine"},"State":{"StartedAt":"","RestartCount":0}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqualStrings("docker run -d --name solo alpine", life.run_cmd.?);
    try std.testing.expect(life.started_at == null);
}
