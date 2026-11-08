//! How many containers are attached to each network, built once per snapshot.

use std::collections::BTreeMap;

use crate::dto::Container;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts(BTreeMap<String, usize>);

impl Counts {
    pub fn from_containers(containers: &[Container]) -> Self {
        let mut counts = BTreeMap::new();
        for container in containers {
            for network in container.network_settings.networks.keys() {
                *counts.entry(network.clone()).or_insert(0) += 1;
            }
        }
        Self(counts)
    }

    pub fn get(&self, network: &str) -> usize {
        self.0.get(network).copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{EndpointSettings, NetworkSettings};

    fn container(networks: &[&str]) -> Container {
        Container {
            network_settings: NetworkSettings {
                networks: networks
                    .iter()
                    .map(|name| ((*name).to_owned(), EndpointSettings::default()))
                    .collect(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn counts_each_network_once_per_container() {
        let counts =
            Counts::from_containers(&[container(&["bridge", "shop"]), container(&["bridge"])]);
        assert_eq!(counts.get("bridge"), 2);
        assert_eq!(counts.get("shop"), 1);
        assert_eq!(counts.get("none"), 0);
    }
}
