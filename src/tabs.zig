//! The views a window can show, and the filters within one. Kept out of
//! `app.zig` so that file stays inside the 600-line rule.
const std = @import("std");
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
            .stacks => "code",
            .disk => "internaldrive",
            .runtimes => "desktopcomputer",
            .settings => "pencil",
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
