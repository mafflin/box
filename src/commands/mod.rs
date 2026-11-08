//! `box list`, `prune` and `stop`

mod list;
mod prune;
mod stop;

pub use prune::{PruneTarget, prune};
pub use stop::stop;

use crate::cli::Command;
use crate::docker::{self, Docker, Terminal};
use crate::settings::Settings;
use crate::theme::Theme;
use crate::{Result, print};

/// Each container's image ID
const CONTAINER_IMAGE_FORMAT: &str = "{{.Image}}";
/// What `box list` prints, named on the command line like its variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ListTarget {
    Containers,
    Images,
    Volumes,
}

/// Runs a command that needs docker
pub fn run(command: Command) -> Result<()> {
    // Before docker, as for the dashboard, and only where it's used, so a broken file can't stop a prune
    let theme = if matches!(command, Command::List { .. }) {
        Settings::load()?.theme
    } else {
        Theme::default()
    };
    docker::check()?;
    let docker = Terminal;
    match command {
        Command::List { target, extended } => {
            let table = match target {
                ListTarget::Containers => list::containers(&docker, extended, &theme)?,
                ListTarget::Images => list::images(&docker, &theme)?,
                ListTarget::Volumes => list::volumes(&docker, &theme)?,
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
