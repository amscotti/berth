//! Which columns each table shows at a given content width, and the widths the
//! rows must use to agree with the header. One module so a breakpoint cannot
//! change the header without changing the cells under it.
const std = @import("std");
const widgets = @import("../widgets.zig");

pub const Density = enum { full, medium, dense, compact, tiny };

/// The tiers are named for what they keep, not for a device: the default
/// 1280-point window has room for every column; a small one gives up the
/// least-used figures first, and the inspector carries the ports when the
/// table cannot.
pub fn ofWidth(width: f32) Density {
    if (width >= 1040) return .full;
    if (width >= 832) return .medium;
    if (width >= 656) return .dense;
    if (width >= 510) return .compact;
    return .tiny;
}

pub const ContainerCols = struct {
    header: []const widgets.Col,
    /// True when a checkbox column was prepended for bulk selection.
    select: bool = false,
    check: f32 = 0,
    name: f32,
    state: f32,
    cpu: f32,
    mem: f32 = 0,
    net: f32 = 0,
    disk: f32 = 0,
    pids: f32 = 0,
    uptime: f32 = 0,
    project: f32 = 0,
    ports: f32 = 0,
    actions: f32,
};

const full_no_project = [_]widgets.Col{
    .{ .name = "NAME", .width = 170, .key = 1 },
    .{ .name = "STATUS", .width = 66, .key = 3 },
    .{ .name = "CPU", .width = 50, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 62, .align_end = true, .key = 5 },
    .{ .name = "NET", .width = 82, .key = 7, .gap_before = 6 },
    .{ .name = "DISK", .width = 82, .key = 8 },
    .{ .name = "PIDS", .width = 38, .align_end = true, .key = 9, .gap_before = 6 },
    .{ .name = "UPTIME", .width = 54, .key = 10 },
    .{ .name = "PORTS", .width = 140 },
    .{ .name = "ACTIONS", .width = 140, .flex_before = true },
};

const full_project = [_]widgets.Col{
    .{ .name = "NAME", .width = 170, .key = 1 },
    .{ .name = "STATUS", .width = 66, .key = 3 },
    .{ .name = "CPU", .width = 50, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 62, .align_end = true, .key = 5 },
    .{ .name = "NET", .width = 82, .key = 7, .gap_before = 6 },
    .{ .name = "DISK", .width = 82, .key = 8 },
    .{ .name = "PROJECT", .width = 90, .key = 6 },
    .{ .name = "PORTS", .width = 140 },
    .{ .name = "ACTIONS", .width = 140, .flex_before = true },
};

const medium_no_project = [_]widgets.Col{
    .{ .name = "NAME", .width = 156, .key = 1 },
    .{ .name = "STATUS", .width = 62, .key = 3 },
    .{ .name = "CPU", .width = 46, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 56, .align_end = true, .key = 5 },
    .{ .name = "NET", .width = 72, .key = 7, .gap_before = 6 },
    .{ .name = "DISK", .width = 72, .key = 8 },
    .{ .name = "PORTS", .width = 112 },
    .{ .name = "ACTIONS", .width = 136, .flex_before = true },
};

const medium_project = [_]widgets.Col{
    .{ .name = "NAME", .width = 150, .key = 1 },
    .{ .name = "STATUS", .width = 62, .key = 3 },
    .{ .name = "CPU", .width = 46, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 56, .align_end = true, .key = 5 },
    .{ .name = "NET", .width = 72, .key = 7, .gap_before = 6 },
    .{ .name = "PROJECT", .width = 78, .key = 6 },
    .{ .name = "PORTS", .width = 112 },
    .{ .name = "ACTIONS", .width = 136, .flex_before = true },
};

/// Net and disk go before the ports, which are the one thing a person clicks.
const dense_cols = [_]widgets.Col{
    .{ .name = "NAME", .width = 150, .key = 1 },
    .{ .name = "STATUS", .width = 64, .key = 3 },
    .{ .name = "CPU", .width = 48, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 60, .align_end = true, .key = 5 },
    .{ .name = "PORTS", .width = 110 },
    .{ .name = "ACTIONS", .width = 140, .flex_before = true },
};

const compact_cols = [_]widgets.Col{
    .{ .name = "NAME", .width = 124, .key = 1 },
    .{ .name = "STATUS", .width = 64, .key = 3 },
    .{ .name = "CPU", .width = 48, .align_end = true, .key = 4 },
    .{ .name = "MEM", .width = 60, .align_end = true, .key = 5 },
    .{ .name = "ACTIONS", .width = 140, .flex_before = true },
};

const tiny_cols = [_]widgets.Col{
    .{ .name = "NAME", .width = 132, .key = 1 },
    .{ .name = "STATUS", .width = 64, .key = 3 },
    .{ .name = "CPU", .width = 48, .align_end = true, .key = 4 },
    .{ .name = "ACTIONS", .width = 140, .flex_before = true },
};

pub fn containers(width: f32, any_project: bool, select: bool, out: *[11]widgets.Col) ContainerCols {
    var result: ContainerCols = switch (ofWidth(width)) {
        .full => if (any_project)
            .{ .header = &full_project, .name = 170, .state = 66, .cpu = 50, .mem = 62, .net = 82, .disk = 82, .project = 90, .ports = 140, .actions = 140 }
        else
            .{ .header = &full_no_project, .name = 170, .state = 66, .cpu = 50, .mem = 62, .net = 82, .disk = 82, .pids = 38, .uptime = 54, .ports = 140, .actions = 140 },
        .medium => if (any_project)
            .{ .header = &medium_project, .name = 150, .state = 62, .cpu = 46, .mem = 56, .net = 72, .project = 78, .ports = 112, .actions = 136 }
        else
            .{ .header = &medium_no_project, .name = 156, .state = 62, .cpu = 46, .mem = 56, .net = 72, .disk = 72, .ports = 112, .actions = 136 },
        .dense => .{ .header = &dense_cols, .name = 150, .state = 64, .cpu = 48, .mem = 60, .ports = 110, .actions = 140 },
        .compact => .{ .header = &compact_cols, .name = 124, .state = 64, .cpu = 48, .mem = 60, .actions = 140 },
        .tiny => .{ .header = &tiny_cols, .name = 132, .state = 64, .cpu = 48, .actions = 144 },
    };

    const src = result.header;
    @memcpy(out[0..src.len], src);
    // The name column takes the window's spare width up to a readable limit;
    // a wide table then reads as a table, not as columns with a hole in them.
    const grow = @min(@max(0, width - tableWidth(src)), 110);
    out[0].width += grow;
    result.name += grow;
    if (!select) {
        result.header = out[0..src.len];
        return result;
    }
    // The checkbox borrows the name column's width, so the table still fits
    // whatever window it was sized for; selection is a mode, not a wider row.
    var i = src.len;
    while (i > 0) {
        out[i] = out[i - 1];
        i -= 1;
    }
    out[1].width -= 44;
    result.name -= 44;
    out[0] = .{ .name = "", .width = 34 };
    result.select = true;
    result.check = 34;
    result.header = out[0 .. src.len + 1];
    return result;
}

pub const ImageCols = struct {
    header: []const widgets.Col,
    repo: f32,
    id: f32,
    size: f32,
    age: f32 = 0,
    status: f32 = 0,
    status_gap: f32 = 0,
};

const image_full = [_]widgets.Col{
    .{ .name = "REPOSITORY:TAG", .width = 300, .key = 1 },
    .{ .name = "ID", .width = 110, .key = 2 },
    .{ .name = "SIZE", .width = 100, .align_end = true, .key = 3 },
    .{ .name = "AGE", .width = 100, .align_end = true, .key = 4 },
    .{ .name = "STATUS", .width = 120, .key = 5, .gap_before = 10 },
};

const image_compact = [_]widgets.Col{
    .{ .name = "REPOSITORY:TAG", .width = 240, .key = 1 },
    .{ .name = "ID", .width = 100, .key = 2 },
    .{ .name = "SIZE", .width = 80, .align_end = true, .key = 3 },
};

pub fn images(width: f32, out: *[5]widgets.Col) ImageCols {
    var result: ImageCols = if (width >= 800)
        .{ .header = &image_full, .repo = 300, .id = 110, .size = 100, .age = 100, .status = 120, .status_gap = 10 }
    else
        .{ .header = &image_compact, .repo = 240, .id = 100, .size = 80 };
    const src = result.header;
    @memcpy(out[0..src.len], src);
    // The repository is what the eye lands on; let it take the spare width so
    // the row does not stop halfway across a wide window.
    const grow = @min(@max(0, width - tableWidth(src)), 220);
    out[0].width += grow;
    result.repo += grow;
    result.header = out[0..src.len];
    return result;
}

pub const VolumeCols = struct {
    header: []const widgets.Col,
    name: f32,
    driver: f32,
    size: f32 = 0,
    in_use: f32 = 0,
    mount: f32,
    mount_gap: f32 = 0,
};

const volume_full = [_]widgets.Col{
    .{ .name = "NAME", .width = 240, .key = 1 },
    .{ .name = "DRIVER", .width = 90, .key = 2 },
    .{ .name = "SIZE", .width = 90, .align_end = true, .key = 3 },
    .{ .name = "IN USE", .width = 80, .align_end = true, .key = 4 },
    .{ .name = "MOUNTPOINT", .width = 246, .key = 5, .gap_before = 12 },
};

const volume_compact = [_]widgets.Col{
    .{ .name = "NAME", .width = 170, .key = 1 },
    .{ .name = "DRIVER", .width = 70, .key = 2 },
    .{ .name = "MOUNTPOINT", .width = 166, .key = 5, .gap_before = 12 },
};

pub fn volumes(width: f32, out: *[5]widgets.Col) VolumeCols {
    var result: VolumeCols = if (width >= 824)
        .{ .header = &volume_full, .name = 240, .driver = 90, .size = 90, .in_use = 80, .mount = 246, .mount_gap = 12 }
    else
        .{ .header = &volume_compact, .name = 170, .driver = 70, .mount = 166, .mount_gap = 12 };
    const src = result.header;
    @memcpy(out[0..src.len], src);
    // Mountpoints are the longest text in the table; they take the spare room.
    const grow = @min(@max(0, width - tableWidth(src)), 260);
    out[src.len - 1].width += grow;
    result.mount += grow;
    result.header = out[0..src.len];
    return result;
}

/// The width a column set asks for, including the row padding and the gaps.
fn tableWidth(src: []const widgets.Col) f32 {
    var used: f32 = 24;
    var seen: usize = 0;
    for (src) |col| {
        if (col.skip) continue;
        if (seen > 0) used += 10;
        // A gap_before/flex_before spacer is an extra child in the row, so it
        // costs its own width and one more 10pt gap.
        if (col.gap_before > 0) used += 10;
        if (col.flex_before) used += 10;
        used += col.width + col.gap_before;
        seen += 1;
    }
    return used;
}

pub const NetworkCols = struct {
    header: []const widgets.Col,
    name: f32,
    driver: f32,
    scope: f32,
    attached: f32,
};

const network_full = [_]widgets.Col{
    .{ .name = "NAME", .width = 220, .key = 1 },
    .{ .name = "DRIVER", .width = 90, .key = 2 },
    .{ .name = "SCOPE", .width = 80, .key = 3 },
    .{ .name = "ATTACHED", .width = 80, .align_end = true, .key = 4 },
};

const network_compact = [_]widgets.Col{
    .{ .name = "NAME", .width = 170, .key = 1 },
    .{ .name = "DRIVER", .width = 70, .key = 2 },
    .{ .name = "SCOPE", .width = 60, .key = 3 },
    .{ .name = "ATTACHED", .width = 60, .align_end = true, .key = 4 },
};

pub fn networks(width: f32) NetworkCols {
    if (width >= 524) return .{ .header = &network_full, .name = 220, .driver = 90, .scope = 80, .attached = 80 };
    return .{ .header = &network_compact, .name = 180, .driver = 70, .scope = 60, .attached = 60 };
}

test "the default window gets every column; a small one gives up the least-used" {
    try std.testing.expectEqual(Density.full, ofWidth(1040));
    try std.testing.expectEqual(Density.medium, ofWidth(832));
    try std.testing.expectEqual(Density.dense, ofWidth(770));
    try std.testing.expectEqual(Density.compact, ofWidth(510));
    try std.testing.expectEqual(Density.tiny, ofWidth(472));

    var full_buf: [11]widgets.Col = undefined;
    const full = containers(1040, false, false, &full_buf);
    try std.testing.expectEqual(@as(f32, 140), full.ports);
    try std.testing.expectEqual(@as(f32, 38), full.pids);
    try std.testing.expectEqual(@as(f32, 170), full.name); // no spare at this width

    var tiny_buf: [11]widgets.Col = undefined;
    const tiny = containers(472, false, false, &tiny_buf);
    try std.testing.expectEqual(@as(f32, 0), tiny.mem);
    try std.testing.expectEqual(@as(f32, 0), tiny.ports);
    try std.testing.expectEqual(@as(f32, 156), tiny.name); // twenty-four spare points, capped at 110
}

test "every density fits the width that chose it" {
    const widths = [_]f32{ 1040, 832, 770, 510, 472 };
    for (widths) |width| {
        var buf: [11]widgets.Col = undefined;
        const cols = containers(width, false, false, &buf);
        try std.testing.expect(tableWidth(cols.header) <= width);
    }
    var buf: [11]widgets.Col = undefined;
    const project = containers(1040, true, false, &buf);
    try std.testing.expect(tableWidth(project.header) <= 1040);
}

test "selection mode borrows the name column's width, not the table's" {
    var buf: [11]widgets.Col = undefined;
    const cols = containers(1040, false, true, &buf);
    try std.testing.expectEqual(@as(usize, 11), cols.header.len);
    try std.testing.expectEqual(@as(f32, 34), cols.header[0].width);
    try std.testing.expectEqual(@as(f32, 126), cols.header[1].width); // the name lends 44 to the checkbox
    try std.testing.expect(tableWidth(cols.header) <= 1040);
}

test "the other tables fit the smallest pane" {
    var image_buf: [5]widgets.Col = undefined;
    const images_small = images(472, &image_buf);
    try std.testing.expect(tableWidth(images_small.header) <= 472);

    var volume_buf: [5]widgets.Col = undefined;
    const volumes_small = volumes(472, &volume_buf);
    try std.testing.expect(tableWidth(volumes_small.header) <= 472);

    const networks_small = networks(472);
    try std.testing.expect(tableWidth(networks_small.header) <= 472);
}
