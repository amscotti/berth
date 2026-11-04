//! A container's lifetime facts that the list endpoint only tells in prose:
//! how long it has been up, and (from inspect) when it started and how many
//! times it restarted. Parsing the English `Status` is cheaper and fresher
//! than one inspect request per row per refresh.
const std = @import("std");
const inspect = @import("inspect.zig");

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
        // A hostile status can carry any i64 count. Saturate both the unit
        // multiplication and the running total so Debug/ReleaseSafe do not
        // trap and ReleaseFast does not wrap into a negative uptime.
        seconds = seconds +| n *| mult;
        any = true;
    }
    return if (any) seconds else null;
}

/// Started-at and restart count from the inspect document, for the detail
/// pane. "0001-01-01T00:00:00Z" means never started, which reads as null.
pub const Lifetime = struct {
    started_at: ?i64 = null,
    restarts: u32 = 0,
    /// `docker run …` reproducing this container, from the same inspect.
    /// Null when the inspect could not be read; freed by the caller's state.
    run_cmd: ?[]u8 = null,
    /// True when an environment value was replaced with `<redacted>` because
    /// its key looked like a credential; the pane mentions this in the toast.
    run_redacted: bool = false,
};

/// The same inspect, read for the facts the pane shows and for a runnable
/// `docker run` line. Built while the parsed document is alive; nothing here
/// survives the arena but what is copied out.
pub fn fromInspect(gpa: std.mem.Allocator, raw: []const u8) Lifetime {
    var out: Lifetime = .{};
    // Parsing and the detail pane's pretty-printer both recurse per level; an
    // engine-supplied document past the limit is not worth a crash.
    if (inspect.nestingExceeds(raw, inspect.max_depth)) return out;
    const Mount = struct {
        Type: []const u8 = "",
        Name: []const u8 = "",
        Source: []const u8 = "",
        Destination: []const u8 = "",
        RW: bool = true,
    };
    const Doc = struct {
        Name: []const u8 = "",
        /// Inspect puts the restart count beside State, not inside it.
        RestartCount: i64 = 0,
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
        } = .{},
    };
    var parsed = std.json.parseFromSlice(Doc, gpa, raw, .{ .ignore_unknown_fields = true, .allocate = .alloc_always }) catch return out;
    defer parsed.deinit();
    const doc = parsed.value;
    const restarts = @max(@as(i64, 0), doc.RestartCount);
    out.restarts = @intCast(@min(restarts, @as(i64, std.math.maxInt(u32))));
    out.started_at = rfc3339(doc.State.StartedAt);
    var redacted = false;
    out.run_cmd = runCommand(gpa, doc, &redacted) catch null;
    out.run_redacted = out.run_cmd != null and redacted;
    return out;
}

/// `docker run -d --name X … image` — the flags a person actually reuses.
/// Compose labels and networks are left out: a compose service is reproduced
/// by `docker compose up`, not by pasting its labels. Fails only on OOM.
fn runCommand(gpa: std.mem.Allocator, doc: anytype, redacted: *bool) !?[]u8 {
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
                // Everything below reaches a shell. Only fields with a shape
                // this command can reproduce are emitted; a hostile engine
                // cannot smuggle a second command through a port field.
                const container_port = portNumber(key) orelse continue;
                if (!validProto(proto)) continue;
                const hosts = entry.value_ptr.*;
                if (hosts != .array) continue;
                for (hosts.array.items) |binding| {
                    if (binding != .object) continue;
                    const port = binding.object.get("HostPort") orelse continue;
                    if (port != .string) continue;
                    const host_port = portNumber(port.string) orelse continue;
                    var ip: []const u8 = "";
                    if (binding.object.get("HostIp")) |value| {
                        // A malformed field is not permission to widen a
                        // specific bind to every interface; drop the binding.
                        if (value != .string) continue;
                        ip = value.string;
                    }
                    const wildcard = ip.len == 0 or std.mem.eql(u8, ip, "0.0.0.0") or std.mem.eql(u8, ip, "::");
                    if (!wildcard and !validIp(ip)) continue;

                    var mapping_buf: [96]u8 = undefined;
                    var mapping_writer = std.Io.Writer.fixed(&mapping_buf);
                    const mapping = &mapping_writer;
                    if (!wildcard) {
                        if (std.mem.indexOfScalar(u8, ip, ':') != null) {
                            mapping.print("[{s}]:", .{ip}) catch continue;
                        } else {
                            mapping.print("{s}:", .{ip}) catch continue;
                        }
                    }
                    mapping.print("{d}:{d}", .{ host_port, container_port }) catch continue;
                    if (!std.mem.eql(u8, proto, "tcp")) mapping.print("/{s}", .{proto}) catch continue;
                    try w.appendSlice(gpa, " -p ");
                    try appendQuoted(gpa, w, mapping.buffered());
                }
            }
        }
    }

    if (doc.Config.Env) |env| {
        for (env) |pair| {
            if (std.mem.startsWith(u8, pair, "PATH=")) continue;
            try w.appendSlice(gpa, " -e ");
            if (secretEnvKey(pair)) |key| {
                try appendRedactedEnv(gpa, w, key);
                redacted.* = true;
            } else {
                try appendQuoted(gpa, w, pair);
            }
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

/// Only decimal ports in the range a container can bind survive into a copied
/// command; anything else is dropped rather than reaching the shell.
fn portNumber(text: []const u8) ?u16 {
    if (text.len == 0) return null;
    for (text) |ch| {
        if (!std.ascii.isDigit(ch)) return null;
    }
    const value = std.fmt.parseInt(u32, text, 10) catch return null;
    if (value == 0 or value > std.math.maxInt(u16)) return null;
    return @intCast(value);
}

fn validProto(proto: []const u8) bool {
    return eql(proto, "tcp") or eql(proto, "udp") or eql(proto, "sctp");
}

/// Unbracketed IP literals only; hostnames, schemes and shell fragments fail.
fn validIp(text: []const u8) bool {
    _ = std.Io.net.IpAddress.parse(text, 0) catch return false;
    return true;
}

/// The key of an environment entry, when it names a credential or its value
/// embeds URL userinfo. Matching is token-based, so `MONKEY` is not `KEY`.
fn secretEnvKey(pair: []const u8) ?[]const u8 {
    const eq = std.mem.indexOfScalar(u8, pair, '=') orelse return null;
    const key = pair[0..eq];
    if (urlHasUserinfo(pair[eq + 1 ..])) return key;

    var i: usize = 0;
    while (i < key.len) {
        while (i < key.len and isKeySeparator(key[i])) i += 1;
        if (i >= key.len) break;
        var start = i;
        i += 1;
        while (i < key.len and !isKeySeparator(key[i])) : (i += 1) {
            if (camelBoundary(key, i)) {
                if (secretToken(key[start..i])) return key;
                start = i;
            }
        }
        if (secretToken(key[start..i])) return key;
    }
    return null;
}

fn isKeySeparator(ch: u8) bool {
    return ch == '_' or ch == '-' or ch == '.';
}

/// `apiKey`, `accessToken` and `APIKey` split before the capital that starts
/// the next word; `API_KEYS` is handled by the separators instead.
fn camelBoundary(key: []const u8, i: usize) bool {
    if (i == 0) return false;
    const prev = key[i - 1];
    const cur = key[i];
    if (!std.ascii.isUpper(cur)) return false;
    if (std.ascii.isLower(prev)) return true;
    return std.ascii.isUpper(prev) and i + 1 < key.len and std.ascii.isLower(key[i + 1]);
}

fn secretToken(token: []const u8) bool {
    const needles = [_][]const u8{
        "TOKEN",   "SECRET",     "PASSWORD",  "PASS",     "KEY",
        "AUTH",    "CREDENTIAL", "DSN",       "URL",      "URI",
        "PRIVATE", "PWD",        "SK",        "SESSION",  "COOKIE",
        "JWT",     "BEARER",     "SIGNATURE", "PASSCODE", "PASSPHRASE",
        "PASSWD",
    };
    for (needles) |needle| {
        if (std.ascii.eqlIgnoreCase(token, needle)) return true;
        // Plural forms keep the same protection: PASSWORDS, SECRETS, COOKIES,
        // API_KEYS, CREDENTIALS.
        if (token.len == needle.len + 1 and std.ascii.eqlIgnoreCase(token[0..needle.len], needle)) {
            const last = token[token.len - 1];
            if (last == 's' or last == 'S') return true;
        }
    }
    return false;
}

/// `scheme://user:password@host` is a credential even when the key is called
/// `GREETING`; a bare `@` or `://` alone is not.
fn urlHasUserinfo(value: []const u8) bool {
    const scheme = std.mem.indexOf(u8, value, "://") orelse return false;
    const at = std.mem.indexOfScalarPos(u8, value, scheme + 3, '@') orelse return false;
    return at > scheme + 3;
}

fn appendRedactedEnv(gpa: std.mem.Allocator, w: *std.ArrayList(u8), key: []const u8) !void {
    // Always quoted: `<redacted>` and hostile keys may contain shell syntax.
    try w.append(gpa, '\'');
    for (key) |ch| {
        if (ch == '\'') {
            try w.appendSlice(gpa, "'\\''");
            continue;
        }
        try w.append(gpa, ch);
    }
    try w.appendSlice(gpa, "=<redacted>'");
}

/// Words are quoted unless every byte is shell-safe: quoting only on a space
/// lets a lone apostrophe or tab split an `-e` value into another argument.
fn appendQuoted(gpa: std.mem.Allocator, w: *std.ArrayList(u8), text: []const u8) !void {
    if (text.len == 0) {
        // An empty word still has to occupy its argument slot. Appending
        // nothing turns `-e <empty>` into the next flag and shifts the command.
        try w.appendSlice(gpa, "''");
        return;
    }
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
    // An engine can put any i64 in the number slot; the multiplication and
    // the running total both saturate instead of trapping.
    try std.testing.expectEqual(@as(?i64, std.math.maxInt(i64)), upSeconds("Up 9223372036854775807 years"));

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
        \\{"RestartCount":3,"State":{"Status":"running","StartedAt":"2026-09-22T18:13:20Z"}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqual(@as(u32, 3), life.restarts);
    try std.testing.expect(life.started_at != null);
}

test "restart count comes from the top level, not from State" {
    const doc =
        \\{"RestartCount":5,"State":{"StartedAt":"2026-09-22T18:13:20Z"}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqual(@as(u32, 5), life.restarts);
}

test "an oversized restart count clamps instead of trapping" {
    const doc =
        \\{"RestartCount":5000000000,"State":{"StartedAt":"2026-09-22T18:13:20Z"}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqual(std.math.maxInt(u32), life.restarts);
}
