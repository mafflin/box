//! The command line, parsed with clap: options anywhere after the command, long ones abbreviated, short ones
//! combined, `--` ends them

use clap::error::ErrorKind;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};

use crate::commands::{ListTarget, PruneTarget};

/// How box calls itself in errors
pub const PROGRAM: &str = "box";

/// Usage, then the command's description, then its options
const HELP_TEMPLATE: &str = "{usage-heading} {usage}\n\n{before-help}{all-args}{after-help}";

const LIST: &str = "list";
const PRUNE: &str = "prune";

#[derive(Parser)]
#[command(
    name = PROGRAM,
    version,
    override_usage = "box [command] [options]",
    before_help = "Opens a dashboard of containers, images and volumes without a command.",
    after_help = "Run 'box command --help' for a command's options.",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print containers, images or volumes as a table
    #[command(
        override_usage = "box list WHAT [options]",
        before_help = "\
Prints WHAT as a table.

Targets:
  containers  Running containers
  images      Images and how many containers use them
  volumes     Volumes and how many containers use them"
    )]
    List(List),

    /// Remove stopped containers, dangling images or unused volumes
    #[command(
        override_usage = "box prune WHAT...",
        before_help = "\
Removes WHAT, containers first, then volumes, then images.

Targets:
  containers  Stopped containers
  images      Dangling images
  volumes     Volumes no container uses, named ones too
  all         All of them"
    )]
    Prune(Prune),

    /// Stop all running containers
    #[command(
        override_usage = "box stop",
        before_help = "Stops all running containers, use docker stop for one."
    )]
    Stop,
}

/// The targets are in the description, so the arguments are hidden
#[derive(Args)]
struct List {
    #[arg(value_name = "WHAT", hide = true)]
    what: Option<ListTarget>,

    /// Show containers with their IPs and mounts
    #[arg(short = 'x', long)]
    extended: bool,
}

#[derive(Args)]
struct Prune {
    #[arg(value_name = "WHAT", hide = true)]
    what: Vec<PruneWhat>,
}

#[derive(Clone, Copy, ValueEnum)]
enum PruneWhat {
    Containers,
    Images,
    Volumes,
    /// Every other target
    All,
}

impl PruneWhat {
    fn targets(self) -> &'static [PruneTarget] {
        match self {
            Self::Containers => &[PruneTarget::Containers],
            Self::Images => &[PruneTarget::Images],
            Self::Volumes => &[PruneTarget::Volumes],
            Self::All => &PruneTarget::ORDER,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// No arguments at all
    Dashboard,
    /// Box's help, or the named subcommand's
    Help(Option<&'static str>),
    List {
        target: ListTarget,
        extended: bool,
    },
    Prune(Vec<PruneTarget>),
    Stop,
}

/// With the settings every subcommand shares
fn command() -> clap::Command {
    with_shared_settings(Cli::command()).mut_subcommands(with_shared_settings)
}

fn with_shared_settings(command: clap::Command) -> clap::Command {
    command
        .help_template(HELP_TEMPLATE)
        .infer_long_args(true)
        .args_override_self(true)
}

/// Box's help, or the subcommand's, styled as clap styles `--help`; a closed pipe ends it quietly
pub fn print_help(subcommand: Option<&str>) {
    let mut command = command();
    let command = match subcommand {
        Some(name) => command.find_subcommand_mut(name).expect("a subcommand"),
        None => &mut command,
    };
    let _ = command.print_help();
}

/// The command, or clap's error, which also carries `--help` and `--version`
pub fn parse(args: &[String]) -> Result<Command, clap::Error> {
    if args.is_empty() {
        return Ok(Command::Dashboard);
    }

    let mut command = command();
    let matches = command
        .try_get_matches_from_mut(std::iter::once(PROGRAM.into()).chain(args.iter().cloned()))?;
    let cli = Cli::from_arg_matches(&matches)?;
    Ok(match cli.command {
        None => Command::Help(None),
        Some(Commands::List(List { what: None, .. })) => Command::Help(Some(LIST)),
        Some(Commands::List(List {
            what: Some(target),
            extended,
        })) => {
            if extended && target != ListTarget::Containers {
                let list = command.find_subcommand_mut(LIST).expect("list");
                return Err(list.error(
                    ErrorKind::ArgumentConflict,
                    "--extended only works with containers",
                ));
            }
            Command::List { target, extended }
        }
        Some(Commands::Prune(Prune { what })) if what.is_empty() => Command::Help(Some(PRUNE)),
        Some(Commands::Prune(Prune { what })) => {
            let chosen: Vec<PruneTarget> = what
                .iter()
                .flat_map(|what| what.targets())
                .copied()
                .collect();
            Command::Prune(
                PruneTarget::ORDER
                    .into_iter()
                    .filter(|target| chosen.contains(target))
                    .collect(),
            )
        }
        Some(Commands::Stop) => Command::Stop,
    })
}

#[cfg(test)]
mod tests;
