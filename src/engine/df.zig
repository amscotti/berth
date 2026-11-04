//! The `/system/df` document and the per-store figures derived from it. Kept
//! out of `api.zig` for the line rule; the API re-exports the type.
//!
//! API 1.52 added the four `*Usage` summaries. Older engines send only the
//! raw lists, so the same figures must be reconstructed from what those lists
//! do carry — otherwise a Docker 28/Podman-compat disk page reads all zeroes.
const std = @import("std");
const api = @import("api.zig");

/// Bytes each engine store holds, from either the new summary or the old lists.
pub const Bytes = struct {
    images: i64 = 0,
    containers: i64 = 0,
    volumes: i64 = 0,
    build_cache: i64 = 0,

    pub fn total(self: Bytes) i64 {
        return self.images +| self.containers +| self.volumes +| self.build_cache;
    }
};

/// Bytes a prune could return from each store, in the same order.
pub const Reclaimable = struct {
    images: i64 = 0,
    containers: i64 = 0,
    volumes: i64 = 0,
    build_cache: i64 = 0,

    pub fn total(self: Reclaimable) i64 {
        return self.images +| self.containers +| self.volumes +| self.build_cache;
    }
};

pub const Df = struct {
    LayersSize: u64 = 0,
    Images: ?[]api.Image = null,
    Containers: ?[]api.Container = null,
    Volumes: ?[]api.Volume = null,
    BuildCache: ?[]BuildCacheItem = null,
    ImageUsage: ?Usage = null,
    ContainerUsage: ?Usage = null,
    VolumeUsage: ?Usage = null,
    BuildCacheUsage: ?Usage = null,

    pub const Usage = struct {
        TotalCount: i64 = 0,
        ActiveCount: i64 = 0,
        TotalSize: i64 = 0,
        Reclaimable: i64 = 0,
    };

    pub const BuildCacheItem = struct {
        ID: []const u8 = "",
        Size: i64 = 0,
        InUse: bool = false,
        Shared: bool = false,
    };

    /// The four figures the rail, Overview, Disk view and prune dialog share.
    pub fn stores(self: Df) Bytes {
        var out: Bytes = .{};
        if (self.ImageUsage) |u| {
            out.images = u.TotalSize;
        } else if (self.LayersSize > 0) {
            // LayersSize counts each layer once; summing per-image Size would
            // count shared layers again.
            out.images = @intCast(@min(self.LayersSize, @as(u64, std.math.maxInt(i64))));
        } else if (self.Images) |list| {
            for (list) |i| out.images = out.images +| @as(i64, @intCast(@min(i.Size, @as(u64, std.math.maxInt(i64)))));
        }

        if (self.ContainerUsage) |u| {
            out.containers = u.TotalSize;
        } else if (self.Containers) |list| {
            for (list) |ct| {
                if (ct.SizeRw > 0) out.containers = out.containers +| ct.SizeRw;
            }
        }

        if (self.VolumeUsage) |u| {
            out.volumes = u.TotalSize;
        } else if (self.Volumes) |list| {
            for (list) |v| {
                if (v.UsageData) |d| {
                    // -1 means "not measured"; adding it would shrink the total.
                    if (d.Size > 0) out.volumes = out.volumes +| d.Size;
                }
            }
        }

        if (self.BuildCacheUsage) |u| {
            out.build_cache = u.TotalSize;
        } else if (self.BuildCache) |list| {
            for (list) |b| {
                // Shared records are referenced by other cache entries and
                // are not storage this store owns on its own.
                if (!b.Shared and b.Size > 0) out.build_cache = out.build_cache +| b.Size;
            }
        }
        return out;
    }

    /// What each store's prune could free. `Images[].Containers == 0`,
    /// `Volumes[].RefCount == 0` and `BuildCache[].InUse == false` are the
    /// same tests the engine applies; a missing measurement stays excluded
    /// rather than counted as free.
    pub fn reclaimableByStore(self: Df) Reclaimable {
        var out: Reclaimable = .{};
        if (self.ImageUsage) |u| {
            out.images = u.Reclaimable;
        } else if (self.Images) |list| {
            if (self.LayersSize > 0) {
                // Docker's legacy formula: everything in the layer store that
                // the still-used images do not uniquely hold.
                const layers = @as(i64, @intCast(@min(self.LayersSize, @as(u64, std.math.maxInt(i64)))));
                var used: i64 = 0;
                for (list, 0..) |image, index| {
                    if (image.Containers <= 0 or repeatedImageId(list, index)) continue;
                    used = used +| uniqueImageBytes(image);
                }
                out.images = @max(0, layers -| used);
            } else {
                for (list, 0..) |image, index| {
                    if (image.Containers != 0 or repeatedImageId(list, index)) continue;
                    out.images = out.images +| uniqueImageBytes(image);
                }
            }
        }

        if (self.ContainerUsage) |u| {
            out.containers = u.Reclaimable;
        } else if (self.Containers) |list| {
            for (list) |ct| {
                // Paused and restarting containers are not prunable either.
                if (!ct.isUp() and ct.SizeRw > 0) out.containers = out.containers +| ct.SizeRw;
            }
        }

        if (self.VolumeUsage) |u| {
            out.volumes = u.Reclaimable;
        } else if (self.Volumes) |list| {
            for (list) |v| {
                if (v.UsageData) |d| {
                    if (d.RefCount == 0 and d.Size > 0) out.volumes = out.volumes +| d.Size;
                }
            }
        }

        if (self.BuildCacheUsage) |u| {
            out.build_cache = u.Reclaimable;
        } else if (self.BuildCache) |list| {
            for (list) |b| {
                if (!b.Shared and !b.InUse and b.Size > 0) out.build_cache = out.build_cache +| b.Size;
            }
        }
        return out;
    }

    /// The image store alone, which several views show as its own figure.
    pub fn imageBytes(self: Df) i64 {
        return self.stores().images;
    }

    /// Everything the engine's stores hold, not only images: the figure the
    /// Overview's DISK card and its graph stand for.
    pub fn totalBytes(self: Df) i64 {
        return self.stores().total();
    }

    pub fn reclaimable(self: Df) i64 {
        return self.reclaimableByStore().total();
    }

    /// Each store as a share of the four that are drawn, clamped at zero the
    /// way the bars are. The fractions come from one place so a legend and its
    /// segment cannot disagree about either the colour or the width.
    pub fn shares(self: Df) Shares {
        const store = self.stores();
        const images = @max(0, store.images);
        const containers = @max(0, store.containers);
        const volumes = @max(0, store.volumes);
        const cache = @max(0, store.build_cache);
        const total = images +| containers +| volumes +| cache;
        if (total <= 0) return .{};
        const f: f32 = @floatFromInt(total);
        return .{
            .images = @as(f32, @floatFromInt(images)) / f,
            .containers = @as(f32, @floatFromInt(containers)) / f,
            .volumes = @as(f32, @floatFromInt(volumes)) / f,
            .build_cache = @as(f32, @floatFromInt(cache)) / f,
        };
    }
};

/// The four store shares, in the order the bars and legends draw them.
pub const Shares = struct {
    images: f32 = 0,
    containers: f32 = 0,
    volumes: f32 = 0,
    build_cache: f32 = 0,

    pub fn total(self: Shares) f32 {
        return self.images + self.containers + self.volumes + self.build_cache;
    }
};

/// The bytes an image holds that no other image shares. `SharedSize` is -1 on
/// older engines that did not measure it; treat that as zero rather than as a
/// negative amount of sharing.
fn uniqueImageBytes(image: api.Image) i64 {
    const size = @as(i64, @intCast(@min(image.Size, @as(u64, std.math.maxInt(i64)))));
    const shared = if (image.SharedSize > 0) image.SharedSize else 0;
    return @max(0, size -| shared);
}

/// Multi-tag images can appear more than once with the same Id; counting each
/// row would make both the used and reclaimable sums too large.
fn repeatedImageId(images: []const api.Image, at: usize) bool {
    const id = images[at].Id;
    if (id.len == 0) return false;
    for (images[0..at]) |earlier| {
        if (std.mem.eql(u8, earlier.Id, id)) return true;
    }
    return false;
}

test "store shares are proportional to the bytes each store holds" {
    const df = Df{
        .ImageUsage = .{ .TotalSize = 50 },
        .ContainerUsage = .{ .TotalSize = 30 },
        .VolumeUsage = .{ .TotalSize = 20 },
    };
    const shares = df.shares();
    try std.testing.expectApproxEqAbs(@as(f32, 0.5), shares.images, 0.0001);
    try std.testing.expectApproxEqAbs(@as(f32, 0.3), shares.containers, 0.0001);
    try std.testing.expectApproxEqAbs(@as(f32, 0.2), shares.volumes, 0.0001);
    try std.testing.expectEqual(@as(f32, 0), shares.build_cache);
    try std.testing.expectApproxEqAbs(@as(f32, 1), shares.total(), 0.0001);

    // No measured bytes means no segments, not four equal zero-width slivers.
    const empty: Df = .{};
    try std.testing.expectEqual(@as(f32, 0), empty.shares().total());
}

test "store totals saturate instead of overflowing on hostile figures" {
    var images = [_]api.Image{
        .{ .Size = std.math.maxInt(u64), .Containers = 0 },
        .{ .Size = std.math.maxInt(u64), .Containers = 0 },
    };
    var containers = [_]api.Container{
        .{ .SizeRw = std.math.maxInt(i64) },
        .{ .SizeRw = std.math.maxInt(i64) },
    };
    var volumes = [_]api.Volume{
        .{ .UsageData = .{ .Size = std.math.maxInt(i64), .RefCount = 0 } },
        .{ .UsageData = .{ .Size = std.math.maxInt(i64), .RefCount = 0 } },
    };
    var cache = [_]Df.BuildCacheItem{
        .{ .Size = std.math.maxInt(i64) },
        .{ .Size = std.math.maxInt(i64) },
    };
    const df = Df{
        .Images = &images,
        .Containers = &containers,
        .Volumes = &volumes,
        .BuildCache = &cache,
    };
    const stores = df.stores();
    try std.testing.expectEqual(std.math.maxInt(i64), stores.images);
    try std.testing.expectEqual(std.math.maxInt(i64), stores.containers);
    try std.testing.expectEqual(std.math.maxInt(i64), stores.volumes);
    try std.testing.expectEqual(std.math.maxInt(i64), stores.build_cache);
    try std.testing.expectEqual(std.math.maxInt(i64), stores.total());

    const free = df.reclaimableByStore();
    try std.testing.expectEqual(std.math.maxInt(i64), free.images);
    try std.testing.expectEqual(std.math.maxInt(i64), free.total());

    // A negative summary must stay negative, not wrap to a huge positive.
    const negative = Df{
        .ImageUsage = .{ .TotalSize = std.math.minInt(i64) },
        .ContainerUsage = .{ .TotalSize = std.math.minInt(i64) },
    };
    try std.testing.expectEqual(std.math.minInt(i64), negative.totalBytes());
}
