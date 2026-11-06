//! `box list`, `prune` and `stop`

mod list;
mod prune;
mod stop;

pub use prune::{PruneTarget, prune};
pub use stop::stop;

use crate::cli::Command;
use crate::docker::{self, Docker, Terminal};
use crate::{Result, print};

/// Each container's image ID
const CONTAINER_IMAGE_FORMAT: &str = "{{.Image}}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListTarget {
    Containers,
    Images,
    Volumes,
}

impl ListTarget {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "containers" => Some(Self::Containers),
            "images" => Some(Self::Images),
            "volumes" => Some(Self::Volumes),
            _ => None,
        }
    }
}

/// Runs a command that needs docker
pub fn run(command: Command) -> Result<()> {
    docker::check()?;
    let docker = Terminal;
    match command {
        Command::List { target, extended } => {
            let table = match target {
                ListTarget::Containers => list::containers(&docker, extended)?,
                ListTarget::Images => list::images(&docker)?,
                ListTarget::Volumes => list::volumes(&docker)?,
            };
            print(&table);
        }
        Command::Prune(targets) => {
            prune(&docker, &targets)?;
        }
        Command::Stop => stop(&docker)?,
        Command::Dashboard | Command::Help(_) => unreachable!("not a docker command"),
    }
    Ok(())
}

/// `format` for every container, running or not
fn all_containers(docker: &impl Docker, format: &str) -> Result<Vec<String>> {
    let ids = docker.lines(&["ps", "-aq"])?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["inspect", "--format", format];
    args.extend(ids.iter().map(String::as_str));
    docker.lines(&args)
}
