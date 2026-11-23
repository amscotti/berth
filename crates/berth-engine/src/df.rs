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

/// Bytes each engine store holds. A store is `None` when the engine didn't
/// measure it or doesn't have it: that is unknown, not zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bytes {
    pub images: Option<i64>,
    pub containers: Option<i64>,
    pub volumes: Option<i64>,
    pub build_cache: Option<i64>,
}

/// One of the engine's disk stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    Images,
    Containers,
    Volumes,
    BuildCache,
}

impl Store {
    pub const ALL: [Store; 4] = [
        Store::Images,
        Store::Containers,
        Store::Volumes,
        Store::BuildCache,
    ];

    /// The name the legends use.
    pub fn label(self) -> &'static str {
        match self {
            Store::Images => "images",
            Store::Containers => "containers",
            Store::Volumes => "volumes",
            Store::BuildCache => "build cache",
        }
    }
}

impl Bytes {
    pub fn get(self, store: Store) -> Option<i64> {
        match store {
            Store::Images => self.images,
            Store::Containers => self.containers,
            Store::Volumes => self.volumes,
            Store::BuildCache => self.build_cache,
        }
    }

    /// The sum of the measured stores, saturating.
    pub fn total(self) -> i64 {
        Store::ALL
            .into_iter()
            .filter_map(|store| self.get(store))
            .fold(0i64, |total, bytes| total.saturating_add(bytes))
    }

    /// True when the engine measured no store at all, so there is no figure to
    /// show, not even zero.
    pub fn is_unmeasured(self) -> bool {
        Store::ALL
            .into_iter()
            .all(|store| self.get(store).is_none())
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
    /// A store is `None` when the engine didn't measure it or doesn't have it:
    /// that is an unknown, not a zero.
    pub fn stores(&self) -> Bytes {
        let images = if let Some(usage) = self.image_usage {
            Some(usage.total_size)
        } else if self.layers_size > 0 {
            // LayersSize counts each layer once; summing per-image Size would
            // count shared layers again.
            Some(clamp_u64(self.layers_size))
        } else {
            self.images.as_ref().map(|images| {
                images.iter().fold(0i64, |total, image| {
                    total.saturating_add(clamp_u64(image.size))
                })
            })
        };

        let containers = match self.container_usage {
            Some(usage) => Some(usage.total_size),
            None => self.containers.as_ref().map(|containers| {
                containers
                    .iter()
                    .filter(|container| container.size_rw > 0)
                    .fold(0i64, |total, container| {
                        total.saturating_add(container.size_rw)
                    })
            }),
        };

        let volumes = match self.volume_usage {
            Some(usage) => Some(usage.total_size),
            None => self.volumes.as_ref().map(|volumes| {
                volumes.iter().fold(0i64, |total, volume| {
                    // -1 means "not measured"; adding it would shrink the total.
                    match volume.usage_data {
                        Some(UsageData { size, .. }) if size > 0 => total.saturating_add(size),
                        _ => total,
                    }
                })
            }),
        };

        let build_cache = match self.build_cache_usage {
            Some(usage) => Some(usage.total_size),
            None => self.build_cache.as_ref().map(|cache| {
                cache
                    .iter()
                    .filter(|item| !item.shared && item.size > 0)
                    .fold(0i64, |total, item| total.saturating_add(item.size))
            }),
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
        let measured = |store: Store| stores.get(store).unwrap_or(0).max(0) as f32;
        let total = measured(Store::Images)
            + measured(Store::Containers)
            + measured(Store::Volumes)
            + measured(Store::BuildCache);
        if total <= 0.0 {
            return Shares::default();
        }
        Shares {
            images: measured(Store::Images) / total,
            containers: measured(Store::Containers) / total,
            volumes: measured(Store::Volumes) / total,
            build_cache: measured(Store::BuildCache) / total,
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
        assert_eq!(df.stores().images, Some(100));
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
        assert_eq!(stores.images, Some(500));
        assert_eq!(stores.containers, Some(109));
        assert_eq!(stores.volumes, Some(10));
        assert_eq!(stores.build_cache, Some(61));
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
        assert_eq!(df.stores().volumes, Some(8));
        assert_eq!(df.reclaimable_by_store().volumes, 8);
    }

    #[test]
    fn an_unmeasured_store_is_unknown_not_zero() {
        let empty = Df::default();
        assert!(empty.stores().is_unmeasured());
        assert_eq!(empty.stores().get(Store::Volumes), None);

        // A measured empty store is a real zero.
        let measured = Df {
            volume_usage: Some(Usage {
                total_size: 0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let stores = measured.stores();
        assert!(!stores.is_unmeasured());
        assert_eq!(stores.get(Store::Volumes), Some(0));
        assert_eq!(stores.get(Store::Images), None);
        assert_eq!(stores.total(), 0);
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
