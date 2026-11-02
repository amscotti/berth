//! The views a window can show, and the filters within one. Kept out of
//! `app.zig` so that file stays inside the 600-line rule.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const api = @import("engine/api.zig");

/// What the container list shows. Filters are per view and stay where the
/// user left them, but they are visible as chips — a filtered list must never
/// look like a short one.
pub const Filter = enum {
    all,
    running,
    stopped,

    pub fn label(self: Filter) []const u8 {
        return switch (self) {
            .all => "All",
            .running => "Running",
            .stopped => "Stopped",
        };
    }

    pub fn keeps(self: Filter, ct: *const api.Container) bool {
        return switch (self) {
            .all => true,
            .running => ct.isRunning(),
            .stopped => !ct.isRunning(),
        };
    }
};

pub const Tab = enum {
    overview,
    containers,
    images,
    volumes,
    networks,
    stacks,
    disk,
    runtimes,
    settings,

    pub fn label(self: Tab) []const u8 {
        return switch (self) {
            .overview => "Overview",
            .containers => "Containers",
            .images => "Images",
            .volumes => "Volumes",
            .networks => "Networks",
            .stacks => "Stacks",
            .disk => "Disk",
            .runtimes => "Runtimes",
            .settings => "Settings",
        };
    }

    /// The fixed icon vocabulary from the design contract: these are the only
    /// names the framework draws on both platforms.
    pub fn icon(self: Tab) []const u8 {
        return switch (self) {
            .overview => "cpu",
            .containers => "cube",
            .images => "doc",
            .volumes => "folder",
            .networks => "network",
            .stacks => "rectangle.on.rectangle",
            .disk => "internaldrive",
            .runtimes => "desktopcomputer",
            .settings => "pencil",
        };
    }
};


/// One search buffer per list view. They are separate so that focus, caret
/// and undo belong to the field on screen, and so switching tabs keeps each
/// view's query where the user left it. `TextBuffer` allocates on the C heap,
/// so `deinit` is called from the state's teardown.
pub const Search = struct {
    containers: ui.TextBuffer = .{},
    images: ui.TextBuffer = .{},
    volumes: ui.TextBuffer = .{},
    networks: ui.TextBuffer = .{},

    pub fn deinit(self: *Search) void {
        self.containers.deinit();
        self.images.deinit();
        self.volumes.deinit();
        self.networks.deinit();
    }

    /// The buffer of the view on screen, or null for the views that have no
    /// table to search.
    pub fn forTab(self: *Search, tab: Tab) ?*ui.TextBuffer {
        return switch (tab) {
            .containers => &self.containers,
            .images => &self.images,
            .volumes => &self.volumes,
            .networks => &self.networks,
            else => null,
        };
    }
};

/// A tab by its name, for `BERTH_TAB`. A plain table: reflection over an
/// enum's field names changed shape in 0.17 and buys nothing here.
pub fn tabFromName(name: []const u8) ?Tab {
    const pairs = [_]struct { n: []const u8, t: Tab }{
        .{ .n = "overview", .t = .overview }, .{ .n = "containers", .t = .containers },
        .{ .n = "images", .t = .images },     .{ .n = "volumes", .t = .volumes },
        .{ .n = "networks", .t = .networks }, .{ .n = "stacks", .t = .stacks },
        .{ .n = "disk", .t = .disk },         .{ .n = "runtimes", .t = .runtimes },
        .{ .n = "settings", .t = .settings },
    };
    for (pairs) |pair| {
        if (std.ascii.eqlIgnoreCase(name, pair.n)) return pair.t;
    }
    return null;
}

test "tab names parse case-insensitively and reject unknown ones" {
    try std.testing.expectEqual(Tab.containers, tabFromName("containers").?);
    try std.testing.expectEqual(Tab.containers, tabFromName("Containers").?);
    try std.testing.expectEqual(Tab.settings, tabFromName("SETTINGS").?);
    try std.testing.expect(tabFromName("events") == null);
    try std.testing.expect(tabFromName("") == null);
}

test "every tab has a label and an icon" {
    inline for (@typeInfo(Tab).@"enum".field_values) |value| {
        const tab: Tab = @enumFromInt(value);
        try std.testing.expect(tab.label().len > 0);
        try std.testing.expect(tab.icon().len > 0);
    }
}

test "filters keep exactly the containers they name" {
    const running: api.Container = .{ .State = "running" };
    const stopped: api.Container = .{ .State = "exited" };
    try std.testing.expect(Filter.all.keeps(&running));
    try std.testing.expect(Filter.all.keeps(&stopped));
    try std.testing.expect(Filter.running.keeps(&running));
    try std.testing.expect(!Filter.running.keeps(&stopped));
    try std.testing.expect(Filter.stopped.keeps(&stopped));
    try std.testing.expect(!Filter.stopped.keeps(&running));
}
