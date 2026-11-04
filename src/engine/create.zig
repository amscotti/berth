//! Volume and network creation: two POSTs with a name in the body. The
//! dialogs validate the name to a shell-free, JSON-safe charset before it
//! gets here, so the body needs no escaping.
const std = @import("std");
const api = @import("api.zig");

/// A resource name the engine accepts: one word of letters, digits, dot,
/// dash and underscore. Docker's volume rule wants at least two characters;
/// the same rule covers networks so the dialog has one sentence.
pub fn validName(name: []const u8) bool {
    if (name.len < 2 or name.len > 128) return false;
    for (name) |ch| {
        if (!(std.ascii.isAlphanumeric(ch) or ch == '_' or ch == '.' or ch == '-')) return false;
    }
    return std.ascii.isAlphanumeric(name[0]);
}

pub fn volume(client: *api.Client, name: []const u8) !void {
    var buf: [160]u8 = undefined;
    const body = try std.fmt.bufPrint(&buf, "{{\"Name\":\"{s}\"}}", .{name});
    // Same budget as a single action: a create on a healthy engine is
    // milliseconds, and the dialog must not hold the frame for longer.
    const response = try client.callOptsTimeout("POST", "/volumes/create", body, 1 << 20, false, 5, false);
    client.gpa.free(response);
}

pub fn network(client: *api.Client, name: []const u8) !void {
    var buf: [200]u8 = undefined;
    // CheckDuplicate turns a second create with the same name into the
    // engine's own 409 instead of a silently duplicated bridge network.
    const body = try std.fmt.bufPrint(&buf, "{{\"Name\":\"{s}\",\"CheckDuplicate\":true}}", .{name});
    const response = try client.callOptsTimeout("POST", "/networks/create", body, 1 << 20, false, 5, false);
    client.gpa.free(response);
}

test "names follow the engine's one-word rule" {
    try std.testing.expect(validName("data"));
    try std.testing.expect(validName("shop_default"));
    try std.testing.expect(validName("net-1.2"));
    try std.testing.expect(!validName(""));
    try std.testing.expect(!validName("a")); // volumes need two characters
    try std.testing.expect(!validName("-leading"));
    try std.testing.expect(!validName("two words"));
    try std.testing.expect(!validName("quote\"break"));
    try std.testing.expect(!validName("slash/name"));

    var longest: [128]u8 = undefined;
    @memset(&longest, 'x');
    try std.testing.expect(validName(&longest));
    var too_long: [129]u8 = undefined;
    @memset(&too_long, 'x');
    try std.testing.expect(!validName(&too_long));
}
