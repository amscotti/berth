//! How many containers run each image, built once per snapshot.

use crate::dto::{Container, Image};

/// For each image, in the order the engine listed them, how many containers
/// run it. The same rule decides it as the one a container's inspector uses
/// (`Container::uses_image`).
pub fn counts(containers: &[Container], images: &[Image]) -> Vec<usize> {
    images
        .iter()
        .map(|image| {
            containers
                .iter()
                .filter(|container| container.uses_image(image))
                .count()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(id: &str, tags: &[&str]) -> Image {
        Image {
            id: id.into(),
            repo_tags: Some(tags.iter().map(|tag| (*tag).to_string()).collect()),
            ..Default::default()
        }
    }

    fn container(image_id: &str, image: &str) -> Container {
        Container {
            image_id: image_id.into(),
            image: image.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_container_counts_for_its_image_however_it_names_it() {
        let images = [
            image("sha256:aa", &["nginx:alpine", "nginx:1.27"]),
            image("sha256:bb", &["alpine:3.22"]),
        ];
        let containers = [
            container("sha256:aa", "nginx:alpine"),
            // Named by the other tag, with no id to go by.
            container("", "nginx:1.27"),
            // Known by its id, under a name that is no tag of the image.
            container("sha256:aa", ""),
            container("sha256:bb", "alpine:3.22"),
        ];
        assert_eq!(counts(&containers, &images), [3, 1]);
    }

    #[test]
    fn an_image_nothing_runs_counts_zero_and_an_empty_engine_counts_nothing() {
        let images = [image("sha256:aa", &["nginx:alpine"])];
        assert_eq!(counts(&[], &images), [0]);
        assert_eq!(counts(&[container("sha256:bb", "alpine")], &images), [0]);
        let none: Vec<usize> = counts(&[container("sha256:bb", "alpine")], &[]);
        assert!(none.is_empty());
    }
}
