//! Compose grouping. Nothing is parsed from YAML: Compose puts the project,
//! service and working directory on every container it creates, so grouping is
//! a label lookup.

use std::collections::BTreeMap;

use crate::dto::Container;

/// One Compose project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub name: String,
    pub running: usize,
    pub total: usize,
    /// Index of the first member in the container list.
    pub first: usize,
    pub members: Vec<usize>,
}

pub fn summaries(containers: &[Container]) -> Vec<Summary> {
    let mut order = Vec::new();
    let mut groups: BTreeMap<String, Summary> = BTreeMap::new();
    for (index, container) in containers.iter().enumerate() {
        let Some(project) = container.project() else {
            continue;
        };
        let entry = groups.entry(project.to_owned()).or_insert_with(|| {
            order.push(project.to_owned());
            Summary {
                name: project.to_owned(),
                first: index,
                ..Default::default()
            }
        });
        entry.total += 1;
        if container.is_up() {
            entry.running += 1;
        }
        entry.members.push(index);
    }
    order
        .into_iter()
        .filter_map(|name| groups.remove(&name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(id: &str, project: Option<&str>, state: &str) -> Container {
        let mut container = Container {
            id: id.into(),
            state: state.into(),
            ..Default::default()
        };
        if let Some(project) = project {
            container
                .labels
                .insert("com.docker.compose.project".into(), project.into());
        }
        container
    }

    #[test]
    fn groups_by_project_in_first_seen_order() {
        let containers = [
            container("a", Some("shop"), "running"),
            container("b", None, "running"),
            container("c", Some("shop"), "exited"),
            container("d", Some("blog"), "running"),
        ];
        let summaries = summaries(&containers);
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "shop");
        assert_eq!(summaries[0].running, 1);
        assert_eq!(summaries[0].total, 2);
        assert_eq!(summaries[0].first, 0);
        assert_eq!(summaries[0].members, vec![0, 2]);
        assert_eq!(summaries[1].name, "blog");
    }

    #[test]
    fn no_compose_labels_means_no_projects() {
        assert!(summaries(&[container("a", None, "running")]).is_empty());
    }
}
