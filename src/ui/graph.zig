//! Cell graphs: a grid of dots where each column is a slice of time and each
//! dot is a step of height. The technique is nanomon's (same framework, same
//! author) — dots are cheap absolute leaves, so a few hundred of them cost
//! less than one nested box per bar.
//!
//! Colours come from the window theme, not a private palette, so the graphs
//! follow light and dark like everything else.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;

const dot: f32 = 4.0;
const pitch: f32 = 6.0;
const inset: f32 = 8.0;

pub const Series = struct {
    values: []const f32,
    color: ui.Color,
};

pub const Options = struct {
    height: f32 = 64,
    /// Scales the graph to this value; null scales to the busiest column.
    scale: ?f32 = null,
    /// Draws a dotted baseline grid so an empty graph still reads as a chart.
    grid: bool = false,
};

/// A scale for a count graph: the whole fleet is the top of the chart, so one
/// running container is one unit rather than half the card. The container
/// count changes when the fleet does, not when a peak sample ages out.
pub fn countScale(total: usize) f32 {
    return @max(1, @as(f32, @floatFromInt(total)));
}

/// Rounds a peak up to the next 1/2/5 × 10^n ceiling. A fixed ceiling keeps a
/// flat series at one height while its oldest samples leave the ring; scaling
/// to `peak * 1.6` re-scaled the chart every time a spike fell off.
pub fn ceiling(peak: f32) f32 {
    if (!(peak > 0)) return 1;
    if (peak == std.math.inf(f32)) return peak;
    const magnitude = std.math.pow(f32, 10, std.math.floor(std.math.log10(peak)));
    const n = peak / magnitude;
    const step: f32 = if (n <= 1) 1 else if (n <= 2) 2 else if (n <= 5) 5 else 10;
    return @max(1, step * magnitude);
}

/// Draws `series` stacked in columns, newest on the right.
pub fn cellGraph(c: *ui.Context, series: []const Series, opts: Options) void {
    // `clip` is load-bearing: the dots are absolute leaves and a stale or
    // wider-than-the-card rect would otherwise paint them outside the well.
    const well = ui.div(c).wFull().h(opts.height).rounded(8)
        .bg(c.theme.background).border(c.theme.border).p(inset).clip();
    const it = well.interaction();
    defer well.end();
    // Without a laid-out rect there is no honest width to scale to; draw
    // nothing this frame rather than guessing 600 and overflowing a narrow card.
    const rect = it.rect orelse return;

    const inner_w = @max(@as(f32, 1), rect.w - 2 * inset);
    const inner_h = @max(@as(f32, 1), opts.height - 2 * inset);
    const cols: usize = @intFromFloat(@max(1, @floor((inner_w + pitch - dot) / pitch)));
    const rows: usize = @intFromFloat(@max(1, @floor((inner_h + pitch - dot) / pitch)));
    const x0 = (inner_w - (@as(f32, @floatFromInt(cols)) * pitch - (pitch - dot))) / 2;

    // Dot positions are relative to the well's content box.
    const base_y: f32 = 0;

    if (opts.grid) {
        var r: usize = 0;
        while (r < rows) : (r += 1) {
            if (r % 3 != 0) continue;
            var i: usize = 0;
            while (i < cols) : (i += 1) {
                const x = x0 + @as(f32, @floatFromInt(i)) * pitch;
                const y = base_y + @as(f32, @floatFromInt(r)) * pitch;
                _ = c.leaf(.{
                    .absolute = true,
                    .offset = .{ .x = x, .y = y },
                    .width = .{ .px = 1 },
                    .height = .{ .px = 1 },
                    .background = c.theme.track,
                }, .none, null);
            }
        }
    }

    const len = if (series.len > 0) series[0].values.len else 0;
    if (len == 0) return;

    // Each column is the mean of its slice; series stack within a column.
    var vals: [4][256]f32 = std.mem.zeroes([4][256]f32);
    const ncols = @min(cols, 256);
    var most: f32 = 0;
    for (0..ncols) |i| {
        const lo = len * i / cols;
        const hi = @max(lo + 1, len * (i + 1) / cols);
        const capped = @min(hi, len);
        if (capped <= lo) continue;
        var sum: f32 = 0;
        for (series[0..@min(series.len, 4)], 0..) |s, si| {
            if (lo >= s.values.len) continue;
            var v: f32 = 0;
            for (s.values[lo..@min(capped, s.values.len)]) |x| v += x;
            v /= @floatFromInt(@min(capped, s.values.len) - lo);
            vals[si][i] = v;
            sum += v;
        }
        most = @max(most, sum);
    }
    const top = opts.scale orelse @max(most * 1.15, 1e-6);
    const rf: f32 = @floatFromInt(rows);

    for (0..ncols) |i| {
        var cum: f32 = 0;
        var levels: [4]usize = @splat(0);
        for (0..@min(series.len, 4)) |si| {
            cum += vals[si][i];
            levels[si] = @intFromFloat(@min(rf, @round(cum / top * rf)));
        }
        const last = @min(series.len, 4) -| 1;
        if (cum > 0 and levels[last] == 0) levels[last] = 1;

        const x = x0 + @as(f32, @floatFromInt(i)) * pitch;
        for (0..rows) |r| {
            // The unlit grid has to be quiet: on a dark background a 45% track
            // is brighter than the data and the chart reads as noise.
            const dark_bg = (c.theme.background.r + c.theme.background.g + c.theme.background.b) < 1.5;
            var color = c.theme.track.alpha(if (dark_bg) 0.16 else 0.45);
            for (0..@min(series.len, 4)) |si| {
                if (r < levels[si]) {
                    color = series[si].color;
                    break;
                }
            }
            if (color.a == 0) continue;
            const y = base_y + (rf - 1 - @as(f32, @floatFromInt(r))) * pitch;
            _ = c.leaf(.{
                .absolute = true,
                .offset = .{ .x = x, .y = y },
                .width = .{ .px = dot },
                .height = .{ .px = dot },
                .radius = 1.2,
                .background = color,
            }, .none, null);
        }
    }
}

test "a count graph scales to the fleet, not to twice its busiest sample" {
    try std.testing.expectEqual(@as(f32, 40), countScale(40));
    try std.testing.expectEqual(@as(f32, 1), countScale(0));
}

test "a rounded ceiling keeps a flat series stable" {
    try std.testing.expectEqual(@as(f32, 1), ceiling(0));
    try std.testing.expectEqual(@as(f32, 5), ceiling(3.2));
    try std.testing.expectEqual(@as(f32, 10), ceiling(8));
    try std.testing.expectEqual(@as(f32, 10), ceiling(9.6));
    try std.testing.expectEqual(@as(f32, 10), ceiling(10));
    try std.testing.expectEqual(@as(f32, 20), ceiling(11));
    try std.testing.expectEqual(@as(f32, 2), ceiling(1.1));
}
