//! Prune routes. Most are shared, but the image filters and build-cache route
//! are not identical between Docker and Podman. A route Podman does not have
//! must be skipped, not retried until it succeeds.

use crate::discovery::Dialect;

/// What a cleanup run may remove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Containers,
    Images,
    Volumes,
    Networks,
    BuildCache,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Containers => "stopped containers",
            Kind::Images => "unused images",
            Kind::Volumes => "unused volumes",
            Kind::Networks => "unused networks",
            Kind::BuildCache => "build cache",
        }
    }
}

const CONTAINER_PATHS: &[&str] = &["/containers/prune"];
const DOCKER_IMAGE_PATHS: &[&str] =
    &["/images/prune?filters=%7B%22dangling%22%3A%7B%22false%22%3Atrue%7D%7D"];
const PODMAN_IMAGE_PATHS: &[&str] = &[
    "/images/prune?filters=%7B%22dangling%22%3A%7B%22true%22%3Atrue%7D%7D",
    "/images/prune?filters=%7B%22dangling%22%3A%7B%22false%22%3Atrue%7D%7D",
];
const DOCKER_VOLUME_PATHS: &[&str] =
    &["/volumes/prune?filters=%7B%22all%22%3A%7B%22true%22%3Atrue%7D%7D"];
const PODMAN_VOLUME_PATHS: &[&str] = &["/volumes/prune"];
const NETWORK_PATHS: &[&str] = &["/networks/prune"];
const BUILD_CACHE_PATHS: &[&str] = &["/build/prune"];

pub fn paths_for(kind: Kind, dialect: Dialect) -> &'static [&'static str] {
    match (kind, dialect) {
        (Kind::Containers, _) => CONTAINER_PATHS,
        (Kind::Images, Dialect::Docker) => DOCKER_IMAGE_PATHS,
        (Kind::Images, Dialect::Podman) => PODMAN_IMAGE_PATHS,
        (Kind::Volumes, Dialect::Docker) => DOCKER_VOLUME_PATHS,
        (Kind::Volumes, Dialect::Podman) => PODMAN_VOLUME_PATHS,
        (Kind::Networks, _) => NETWORK_PATHS,
        (Kind::BuildCache, _) => BUILD_CACHE_PATHS,
    }
}

/// Podman has no build-cache prune route; asking would turn a safe cleanup into
/// a failure.
pub fn supported(kind: Kind, dialect: Dialect) -> bool {
    !(kind == Kind::BuildCache && dialect == Dialect::Podman)
}

/// The stored build-cache preference is kept for Docker; only the effective
/// value is false while a Podman dialect is active.
pub fn build_cache_enabled(selected: bool, dialect: Dialect) -> bool {
    selected && supported(Kind::BuildCache, dialect)
}

/// The ordered cleanup plan. Containers first, then their leavings.
pub fn plan(
    containers: bool,
    images: bool,
    volumes: bool,
    networks: bool,
    build_cache: bool,
    dialect: Dialect,
) -> Vec<(Kind, &'static [&'static str])> {
    let mut steps = Vec::new();
    for (on, kind) in [
        (containers, Kind::Containers),
        (images, Kind::Images),
        (volumes, Kind::Volumes),
        (networks, Kind::Networks),
        (build_cache, Kind::BuildCache),
    ] {
        if on && supported(kind, dialect) {
            steps.push((kind, paths_for(kind, dialect)));
        }
    }
    steps
}

/// Add a reclaimed byte count, saturating.
pub fn add_reclaimed(total: &mut u64, freed: u64) {
    *total = total.saturating_add(freed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_and_podman_image_routes_differ() {
        assert_eq!(paths_for(Kind::Images, Dialect::Docker).len(), 1);
        assert_eq!(paths_for(Kind::Images, Dialect::Podman).len(), 2);
        assert!(paths_for(Kind::Images, Dialect::Podman)[0].contains("dangling"));
    }

    #[test]
    fn podman_has_no_build_cache_route() {
        assert!(supported(Kind::BuildCache, Dialect::Docker));
        assert!(!supported(Kind::BuildCache, Dialect::Podman));
        assert!(!build_cache_enabled(true, Dialect::Podman));
        assert!(build_cache_enabled(true, Dialect::Docker));
        let plan = plan(true, true, true, true, true, Dialect::Podman);
        assert!(!plan.iter().any(|(kind, _)| *kind == Kind::BuildCache));
    }

    #[test]
    fn the_plan_orders_containers_first() {
        let plan = plan(true, true, false, false, false, Dialect::Docker);
        assert_eq!(plan[0].0, Kind::Containers);
        assert_eq!(plan[1].0, Kind::Images);
        assert_eq!(plan.len(), 2);
    }

    #[test]
    fn empty_selection_produces_no_steps() {
        assert!(plan(false, false, false, false, false, Dialect::Docker).is_empty());
    }

    #[test]
    fn reclaim_saturates() {
        let mut total = u64::MAX - 1;
        add_reclaimed(&mut total, 10);
        assert_eq!(total, u64::MAX);
    }
}
