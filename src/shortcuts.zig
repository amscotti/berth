//! Window-level keyboard shortcuts: the first nine views on digits, and the
//! current page's search field on F. Kept separate from the shell so the map
//! is testable without a window, and so a dialog can own Enter/Escape first.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const tabs = @import("tabs.zig");
const sheets = @import("sheets.zig");

/// One-based, matching the rail: 1 is Overview through 9 is Runtimes. Settings
/// deliberately has no digit; ten views do not fit a single row of keys.
pub fn tabForDigit(key: u21) ?tabs.Tab {
    if (key < '1' or key > '9') return null;
    return tabs.all[key - '1'];
}

pub fn isSearchKey(key: u21) bool {
    return key == 'f' or key == 'F';
}

/// Offered before the view paints, so a dialog that also handles these keys
/// never sees a tab switch behind it. A changed tab closes any open sheet:
/// its target no longer exists on the page being left.
pub fn handle(s: *app.State, c: *ui.Context) void {
    for (tabs.all[0..9], 1..10) |tab, n| {
        const digit: u21 = @intCast('0' + n);
        // `cmd` is Control off Apple's systems; the second check covers an
        // explicit Control press on a Mac, where native() leaves it alone.
        if (c.shortcut(.{ .key = digit, .cmd = true }) or c.shortcut(.{ .key = digit, .ctrl = true })) {
            switchTab(s, tab);
            return;
        }
    }
    if ((c.shortcut(.{ .key = 'f', .cmd = true }) or c.shortcut(.{ .key = 'f', .ctrl = true })) and !sheets.open(s)) {
        if (s.search.forTab(s.tab)) |buffer| {
            buffer.focus();
            s.redraw();
        }
    }
}

pub fn switchTab(s: *app.State, tab: tabs.Tab) void {
    if (s.tab == tab) return;
    // A row from one view has no meaning in another; the inspector follows
    // the same rule a rail click already enforces.
    if (s.selected != null) s.clearSelection();
    sheets.dismiss(s);
    s.tab = tab;
    s.redraw();
}

test "digits one through nine address the first nine views" {
    try std.testing.expectEqual(tabs.Tab.overview, tabForDigit('1').?);
    try std.testing.expectEqual(tabs.Tab.containers, tabForDigit('2').?);
    try std.testing.expectEqual(tabs.Tab.images, tabForDigit('3').?);
    try std.testing.expectEqual(tabs.Tab.volumes, tabForDigit('4').?);
    try std.testing.expectEqual(tabs.Tab.networks, tabForDigit('5').?);
    try std.testing.expectEqual(tabs.Tab.events, tabForDigit('6').?);
    try std.testing.expectEqual(tabs.Tab.stacks, tabForDigit('7').?);
    try std.testing.expectEqual(tabs.Tab.disk, tabForDigit('8').?);
    try std.testing.expectEqual(tabs.Tab.runtimes, tabForDigit('9').?);
    // Settings has no shortcut, and neither does a key outside the row.
    try std.testing.expect(tabForDigit('0') == null);
    try std.testing.expect(tabForDigit('a') == null);
}

test "F is the search key" {
    try std.testing.expect(isSearchKey('f'));
    try std.testing.expect(isSearchKey('F'));
    // Shortcut keys are lowered by the platform before the view sees them.
    try std.testing.expect(!isSearchKey('g'));
}
