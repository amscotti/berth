//! The Linux desktop's dark-mode preference. GNOME exposes it through
//! gsettings' color-scheme key; KDE and other desktops often leave the same
//! key unset, so GTK_THEME remains the fallback. The read side is bounded: one
//! deadline covers every pipe read, and a hung or trickling gsettings is
//! killed when the budget expires. Spawn and the final wait are outside that
//! deadline; gsettings exits promptly after writing its one line.
const std = @import("std");
const builtin = @import("builtin");

extern "c" fn getenv(name: [*:0]const u8) ?[*:0]u8;

/// Reads the desktop preference once for the theme the window opens with.
/// Returns false when neither signal is available, keeping GTK's light default.
pub fn prefersDark(gpa: std.mem.Allocator, io: std.Io) bool {
    if (builtin.os.tag != .linux) return false;
    if (queryColorScheme(gpa, io)) |dark| return dark;
    return gtkThemeIsDark();
}

fn queryColorScheme(gpa: std.mem.Allocator, io: std.Io) ?bool {
    // One deadline shared by every pipe read. Passing a duration instead
    // would give each read its own 250 ms window and let a trickling D-Bus
    // stretch it. Spawn and the final wait are not covered by this timeout.
    const budget: std.Io.Timeout = .{ .duration = .{
        .raw = .fromMilliseconds(250),
        .clock = .awake,
    } };
    const result = std.process.run(gpa, io, .{
        .argv = &.{ "gsettings", "get", "org.gnome.desktop.interface", "color-scheme" },
        .stdout_limit = .limited(256),
        .stderr_limit = .limited(256),
        .timeout = budget.toDeadline(io),
    }) catch return null;
    defer gpa.free(result.stdout);
    defer gpa.free(result.stderr);
    if (!result.term.success()) return null;
    return parseColorScheme(result.stdout);
}

/// `gsettings` prints a quoted enum such as `'prefer-dark'`. `default` means
/// the desktop has no explicit preference, so the GTK_THEME fallback still
/// gets a turn. Unknown values do the same rather than guessing.
pub fn parseColorScheme(text: []const u8) ?bool {
    const value = std.mem.trim(u8, text, " \t\r\n'\"");
    if (std.mem.eql(u8, value, "prefer-dark")) return true;
    if (std.mem.eql(u8, value, "prefer-light")) return false;
    return null;
}

fn gtkThemeIsDark() bool {
    const raw = getenv("GTK_THEME") orelse return false;
    const name = std.mem.span(raw);
    var i: usize = 0;
    while (i + 4 <= name.len) : (i += 1) {
        if (std.ascii.eqlIgnoreCase(name[i .. i + 4], "dark")) return true;
    }
    return false;
}

test "gsettings color-scheme values" {
    try std.testing.expectEqual(true, parseColorScheme("'prefer-dark'\n").?);
    try std.testing.expectEqual(false, parseColorScheme("'prefer-light'\n").?);
    // `default` is not a light preference: leave it to GTK_THEME.
    try std.testing.expectEqual(@as(?bool, null), parseColorScheme("'default'\n"));
    try std.testing.expectEqual(@as(?bool, null), parseColorScheme("'something-new'\n"));
    try std.testing.expectEqual(@as(?bool, null), parseColorScheme(""));
}
