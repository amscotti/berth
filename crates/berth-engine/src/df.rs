//! `/system/df` and the per-store figures derived from it.
//!
//! API 1.52 added the four `*Usage` summaries. Older engines send only the raw
//! lists, so the same figures must be reconstructed from what those lists
//! carry — otherwise Docker 28 / Podman compat reads all zeroes.
//!
//! Integer figures are signed because the engine uses -1 for "not measured".
//! Every sum saturates; a hostile document must not wrap into a tiny total.

use serde::{Deserialize, Serialize};

use crate::dto::{Image, UsageData, Volume};

/// Bytes each engine store holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bytes {
    pub images: i64,
    pub containers: i64,
    pub volumes: i64,
    pub build_cache: i64,
}

impl Bytes {
    pub fn total(self) -> i64 {
        self.images
            .saturating_add(self.containers)
            .saturating_add(self.volumes)
            .saturating_add(self.build_cache)
    }
}

/// Bytes a prune could return from each store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reclaimable {
    pub images: i64,
    pub containers: i64,
    pub volumes: i64,
    pub build_cache: i64,
}

impl Reclaimable {
    pub fn total(self) -> i64 {
        self.images
            .saturating_add(self.containers)
            .saturating_add(self.volumes)
            .saturating_add(self.build_cache)
    }
}

/// The clamped shares used by the disk bar. A store with a negative sentinel
/// contributes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shares {
    pub images: f32,
    pub containers: f32,
    pub volumes: f32,
    pub build_cache: f32,
}

impl Shares {
    pub fn total(self) -> f32 {
        self.images + self.containers + self.volumes + self.build_cache
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Usage {
    pub total_count: i64,
    pub active_count: i64,
    pub total_size: i64,
    pub reclaimable: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct BuildCacheItem {
    #[serde(rename = "ID")]
    pub id: String,
    pub size: i64,
    pub in_use: bool,
    pub shared: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ContainerUsage {
    #[serde(rename = "Id")]
    pub id: String,
    pub size_rw: i64,
    pub size_root_fs: i64,
    pub state: String,
}

/// The `/system/df` document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Df {
    pub layers_size: u64,
    pub images: Option<Vec<Image>>,
    pub containers: Option<Vec<ContainerUsage>>,
    pub volumes: Option<Vec<Volume>>,
    pub build_cache: Option<Vec<BuildCacheItem>>,
    pub image_usage: Option<Usage>,
    pub container_usage: Option<Usage>,
    pub volume_usage: Option<Usage>,
    pub build_cache_usage: Option<Usage>,
}

impl Df {
    /// The four figures the rail, overview, disk view and prune sheet share.
    pub fn stores(&self) -> Bytes {
        let images = if let Some(usage) = self.image_usage {
            usage.total_size
        } else if self.layers_size > 0 {
            // LayersSize counts each layer once; summing per-image Size would
            // count shared layers again.
            clamp_u64(self.layers_size)
        } else if let Some(images) = &self.images {
            images.iter().fold(0i64, |total, image| {
                total.saturating_add(clamp_u64(image.size))
            })
        } else {
            0
        };

        let containers = if let Some(usage) = self.container_usage {
            usage.total_size
        } else if let Some(containers) = &self.containers {
            containers
                .iter()
                .filter(|container| container.size_rw > 0)
                .fold(0i64, |total, container| {
                    total.saturating_add(container.size_rw)
                })
        } else {
            0
        };

        let volumes = if let Some(usage) = self.volume_usage {
            usage.total_size
        } else if let Some(volumes) = &self.volumes {
            volumes.iter().fold(0i64, |total, volume| {
                // -1 means "not measured"; adding it would shrink the total.
                match volume.usage_data {
                    Some(UsageData { size, .. }) if size > 0 => total.saturating_add(size),
                    _ => total,
                }
            })
        } else {
            0
        };

        let build_cache = if let Some(usage) = self.build_cache_usage {
            usage.total_size
        } else if let Some(cache) = &self.build_cache {
            cache
                .iter()
                .filter(|item| !item.shared && item.size > 0)
                .fold(0i64, |total, item| total.saturating_add(item.size))
        } else {
            0
        };

        Bytes {
            images,
            containers,
            volumes,
            build_cache,
        }
    }

    /// What each store's prune could free. The tests mirror the engine's own:
    /// an image nothing uses, a stopped container, an unreferenced volume, an
    /// unused cache record. A missing measurement is excluded, not counted.
    pub fn reclaimable_by_store(&self) -> Reclaimable {
        let images = if let Some(usage) = self.image_usage {
            usage.reclaimable
        } else if let Some(images) = &self.images {
            let mut seen = std::collections::HashSet::with_capacity(images.len());
            if self.layers_size > 0 {
                let layers = clamp_u64(self.layers_size);
                let used = images.iter().fold(0i64, |total, image| {
                    if !seen.insert(image.id.as_str()) || image.containers <= 0 {
                        total
                    } else {
                        total.saturating_add(unique_image_bytes(image))
                    }
                });
                (layers - used).max(0)
            } else {
                images.iter().fold(0i64, |total, image| {
                    if !seen.insert(image.id.as_str()) || image.containers != 0 {
                        total
                    } else {
                        total.saturating_add(unique_image_bytes(image))
                    }
                })
            }
        } else {
            0
        };

        let containers = if let Some(usage) = self.container_usage {
            usage.reclaimable
        } else if let Some(containers) = &self.containers {
            containers
                .iter()
                .filter(|container| {
                    !matches!(
                        container.state.as_str(),
                        "running" | "paused" | "restarting"
                    ) && container.size_rw > 0
                })
                .fold(0i64, |total, container| {
                    total.saturating_add(container.size_rw)
                })
        } else {
            0
        };

        let volumes = if let Some(usage) = self.volume_usage {
            usage.reclaimable
        } else if let Some(volumes) = &self.volumes {
            volumes
                .iter()
                .fold(0i64, |total, volume| match volume.usage_data {
                    Some(UsageData { size, ref_count }) if size > 0 && ref_count == 0 => {
                        total.saturating_add(size)
                    }
                    _ => total,
                })
        } else {
            0
        };

        let build_cache = if let Some(usage) = self.build_cache_usage {
            usage.reclaimable
        } else if let Some(cache) = &self.build_cache {
            cache
                .iter()
                .filter(|item| !item.in_use && !item.shared && item.size > 0)
                .fold(0i64, |total, item| total.saturating_add(item.size))
        } else {
            0
        };

        Reclaimable {
            images,
            containers,
            volumes,
            build_cache,
        }
    }

    pub fn total_bytes(&self) -> i64 {
        self.stores().total()
    }

    pub fn reclaimable(&self) -> i64 {
        self.reclaimable_by_store().total()
    }

    pub fn shares(&self) -> Shares {
        let stores = self.stores();
        let total = stores.total().max(0) as f32;
        if total <= 0.0 {
            return Shares::default();
        }
        Shares {
            images: stores.images.max(0) as f32 / total,
            containers: stores.containers.max(0) as f32 / total,
            volumes: stores.volumes.max(0) as f32 / total,
            build_cache: stores.build_cache.max(0) as f32 / total,
        }
    }
}

fn clamp_u64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn unique_image_bytes(image: &Image) -> i64 {
    let unique = if image.shared_size >= 0 {
        clamp_u64(image.size).saturating_sub(image.shared_size)
    } else {
        clamp_u64(image.size)
    };
    unique.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(id: &str, size: u64, shared: i64, containers: i64) -> Image {
        Image {
            id: id.to_owned(),
            size,
            shared_size: shared,
            containers,
            ..Default::default()
        }
    }

    #[test]
    fn new_usage_summaries_win() {
        let df = Df {
            image_usage: Some(Usage {
                total_size: 100,
                reclaimable: 40,
                ..Default::default()
            }),
            container_usage: Some(Usage {
                total_size: 50,
                reclaimable: 20,
                ..Default::default()
            }),
            volume_usage: Some(Usage {
                total_size: 30,
                reclaimable: 10,
                ..Default::default()
            }),
            build_cache_usage: Some(Usage {
                total_size: 20,
                reclaimable: 5,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(df.stores().images, 100);
        assert_eq!(df.reclaimable_by_store().images, 40);
        assert_eq!(df.total_bytes(), 200);
        assert_eq!(df.reclaimable(), 75);
    }

    #[test]
    fn old_lists_are_reconstructed() {
        let df = Df {
            layers_size: 500,
            images: Some(vec![
                image("sha256:a", 300, 0, 1),
                image("sha256:b", 200, 0, 0),
            ]),
            containers: Some(vec![
                ContainerUsage {
                    state: "exited".into(),
                    size_rw: 10,
                    ..Default::default()
                },
                ContainerUsage {
                    state: "running".into(),
                    size_rw: 99,
                    ..Default::default()
                },
            ]),
            volumes: Some(vec![
                Volume {
                    usage_data: Some(UsageData {
                        size: 7,
                        ref_count: 0,
                    }),
                    ..Default::default()
                },
                Volume {
                    usage_data: Some(UsageData {
                        size: 3,
                        ref_count: 2,
                    }),
                    ..Default::default()
                },
            ]),
            build_cache: Some(vec![
                BuildCacheItem {
                    size: 11,
                    in_use: false,
                    shared: false,
                    ..Default::default()
                },
                BuildCacheItem {
                    size: 50,
                    in_use: true,
                    shared: false,
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        let stores = df.stores();
        assert_eq!(stores.images, 500);
        assert_eq!(stores.containers, 109);
        assert_eq!(stores.volumes, 10);
        assert_eq!(stores.build_cache, 61);
        let free = df.reclaimable_by_store();
        assert_eq!(free.containers, 10);
        assert_eq!(free.volumes, 7);
        assert_eq!(free.build_cache, 11);
    }

    #[test]
    fn negative_sentinels_do_not_shrink_a_total() {
        let df = Df {
            volumes: Some(vec![
                Volume {
                    usage_data: Some(UsageData {
                        size: -1,
                        ref_count: 0,
                    }),
                    ..Default::default()
                },
                Volume {
                    usage_data: Some(UsageData {
                        size: 8,
                        ref_count: 0,
                    }),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        assert_eq!(df.stores().volumes, 8);
        assert_eq!(df.reclaimable_by_store().volumes, 8);
    }

    #[test]
    fn shares_sum_to_one_for_a_nonempty_store() {
        let df = Df {
            image_usage: Some(Usage {
                total_size: 50,
                ..Default::default()
            }),
            volume_usage: Some(Usage {
                total_size: 50,
                ..Default::default()
            }),
            ..Default::default()
        };
        let shares = df.shares();
        assert!((shares.total() - 1.0).abs() < 0.0001);
        assert!((shares.images - 0.5).abs() < 0.0001);
    }

    #[test]
    fn totals_saturate_instead_of_wrapping() {
        let df = Df {
            image_usage: Some(Usage {
                total_size: i64::MAX,
                ..Default::default()
            }),
            volume_usage: Some(Usage {
                total_size: i64::MAX,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(df.total_bytes(), i64::MAX);
    }
}
