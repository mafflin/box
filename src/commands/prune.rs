//! `box prune`, which the dashboard's prune dialog shares

use super::{CONTAINER_IMAGE_FORMAT, all_containers};
use crate::Result;
use crate::docker::Docker;
use crate::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneTarget {
    Containers,
    Volumes,
    Images,
}

impl PruneTarget {
    /// Containers first, as removing them leaves volumes and images dangling
    pub const ORDER: [Self; 3] = [Self::Containers, Self::Volumes, Self::Images];
    /// The states of the containers it removes: stopped, or created and never started
    pub const STOPPED_STATES: [&str; 2] = ["exited", "created"];

    /// `all` is every target
    pub fn from_name(name: &str) -> Option<Vec<Self>> {
        match name {
            "containers" => Some(vec![Self::Containers]),
            "volumes" => Some(vec![Self::Volumes]),
            "images" => Some(vec![Self::Images]),
            "all" => Some(Self::ORDER.to_vec()),
            _ => None,
        }
    }

    /// What it would remove, as docker lists them
    fn list(self, docker: &impl Docker) -> Result<Vec<String>> {
        match self {
            Self::Containers => {
                let filters: Vec<String> = Self::STOPPED_STATES
                    .iter()
                    .map(|state| format!("status={state}"))
                    .collect();
                let mut args = vec!["ps"];
                for filter in &filters {
                    args.extend(["--filter", filter]);
                }
                args.push("-q");
                docker.lines(&args)
            }
            Self::Volumes => docker.lines(&["volume", "ls", "--filter", "dangling=true", "-q"]),
            Self::Images => {
                docker.lines(&["images", "--filter", "dangling=true", "-q", "--no-trunc"])
            }
        }
    }

    fn remove(self) -> &'static [&'static str] {
        match self {
            Self::Containers => &["rm", "-v"],
            Self::Volumes => &["volume", "rm"],
            Self::Images => &["rmi"],
        }
    }

    /// Like `2 containers`
    pub fn count(self, count: usize) -> String {
        let noun = match self {
            Self::Containers => "container",
            Self::Volumes => "volume",
            Self::Images => "image",
        };
        format::plural(count, noun)
    }
}

/// Removes each target's items one by one, in the fixed order, stopping at the first failure; how many of each.
/// Dangling images that containers still use are left, as docker won't remove them.
pub fn prune(docker: &impl Docker, targets: &[PruneTarget]) -> Result<Vec<(PruneTarget, usize)>> {
    let mut removed = Vec::new();
    for target in PruneTarget::ORDER
        .into_iter()
        .filter(|target| targets.contains(target))
    {
        let mut items = target.list(docker)?;
        if target == PruneTarget::Images {
            let used = all_containers(docker, CONTAINER_IMAGE_FORMAT)?;
            items.retain(|image| !used.contains(image));
        }
        for item in &items {
            let mut args = target.remove().to_vec();
            args.push(item);
            docker.run(&args)?;
        }
        removed.push((target, items.len()));
    }
    Ok(removed)
}

#[cfg(test)]
mod tests;
