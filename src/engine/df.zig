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
        return self.images + self.containers + self.volumes + self.build_cache;
    }
};

/// Bytes a prune could return from each store, in the same order.
pub const Reclaimable = struct {
    images: i64 = 0,
    containers: i64 = 0,
    volumes: i64 = 0,
    build_cache: i64 = 0,

    pub fn total(self: Reclaimable) i64 {
        return self.images + self.containers + self.volumes + self.build_cache;
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
            for (list) |i| out.images += @intCast(@min(i.Size, @as(u64, std.math.maxInt(i64))));
        }

        if (self.ContainerUsage) |u| {
            out.containers = u.TotalSize;
        } else if (self.Containers) |list| {
            for (list) |ct| {
                if (ct.SizeRw > 0) out.containers += ct.SizeRw;
            }
        }

        if (self.VolumeUsage) |u| {
            out.volumes = u.TotalSize;
        } else if (self.Volumes) |list| {
            for (list) |v| {
                if (v.UsageData) |d| {
                    // -1 means "not measured"; adding it would shrink the total.
                    if (d.Size > 0) out.volumes += d.Size;
                }
            }
        }

        if (self.BuildCacheUsage) |u| {
            out.build_cache = u.TotalSize;
        } else if (self.BuildCache) |list| {
            for (list) |b| {
                // Shared records are referenced by other cache entries and
                // are not storage this store owns on its own.
                if (!b.Shared and b.Size > 0) out.build_cache += b.Size;
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
                    used += uniqueImageBytes(image);
                }
                out.images = @max(0, layers - used);
            } else {
                for (list, 0..) |image, index| {
                    if (image.Containers != 0 or repeatedImageId(list, index)) continue;
                    out.images += uniqueImageBytes(image);
                }
            }
        }

        if (self.ContainerUsage) |u| {
            out.containers = u.Reclaimable;
        } else if (self.Containers) |list| {
            for (list) |ct| {
                // Paused and restarting containers are not prunable either.
                if (!ct.isUp() and ct.SizeRw > 0) out.containers += ct.SizeRw;
            }
        }

        if (self.VolumeUsage) |u| {
            out.volumes = u.Reclaimable;
        } else if (self.Volumes) |list| {
            for (list) |v| {
                if (v.UsageData) |d| {
                    if (d.RefCount == 0 and d.Size > 0) out.volumes += d.Size;
                }
            }
        }

        if (self.BuildCacheUsage) |u| {
            out.build_cache = u.Reclaimable;
        } else if (self.BuildCache) |list| {
            for (list) |b| {
                if (!b.Shared and !b.InUse and b.Size > 0) out.build_cache += b.Size;
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
};

/// The bytes an image holds that no other image shares. `SharedSize` is -1 on
/// older engines that did not measure it; treat that as zero rather than as a
/// negative amount of sharing.
fn uniqueImageBytes(image: api.Image) i64 {
    const size = @as(i64, @intCast(@min(image.Size, @as(u64, std.math.maxInt(i64)))));
    const shared = if (image.SharedSize > 0) image.SharedSize else 0;
    return @max(0, size - shared);
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
