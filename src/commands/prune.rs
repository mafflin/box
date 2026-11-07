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

    fn list(self) -> &'static [&'static str] {
        match self {
            Self::Containers => &[
                "ps",
                "--filter",
                "status=exited",
                "--filter",
                "status=created",
                "-q",
            ],
            Self::Volumes => &["volume", "ls", "--filter", "dangling=true", "-q"],
            Self::Images => &["images", "--filter", "dangling=true", "-q", "--no-trunc"],
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
        let mut items = docker.lines(target.list())?;
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
