//! The command line, read the way Ruby's `OptionParser` reads one: options anywhere after the command, long ones
//! abbreviated, short ones combined, `--` ends them

use std::fmt::Write;

use crate::commands::{ListTarget, PruneTarget};

const BOX_HELP: &str = "\
Usage: box [command] [options]

Opens a dashboard of containers, images and volumes without a command.

Commands:
  list   Print containers, images or volumes as a table
  prune  Remove stopped containers, dangling images or unused volumes
  stop   Stop all running containers
";

const LIST_HELP: &str = "\
Usage: box list WHAT [options]

Prints WHAT as a table.

Targets:
  containers  Running containers
  images      Images and how many containers use them
  volumes     Volumes and how many containers use them
";

const PRUNE_HELP: &str = "\
Usage: box prune WHAT...

Removes WHAT, containers first, then volumes, then images.

Targets:
  containers  Stopped containers
  images      Dangling images
  volumes     Volumes no container uses, named ones too
  all         All of them
";

const STOP_HELP: &str = "\
Usage: box stop

Stops all running containers, use docker stop for one.
";

/// Help's column of option names, like `-h, --help`
const OPTION_NAMES_WIDTH: usize = 15;

const BOX_FOOTER: &str = "Run 'box command --help' for a command's options.";

struct Opt {
    short: char,
    long: &'static str,
    description: &'static str,
}

const HELP: Opt = Opt {
    short: 'h',
    long: "help",
    description: "Print help",
};

const EXTENDED: Opt = Opt {
    short: 'x',
    long: "extended",
    description: "Show containers with their IPs and mounts",
};

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// No arguments at all
    Dashboard,
    Help(String),
    List {
        target: ListTarget,
        extended: bool,
    },
    Prune(Vec<PruneTarget>),
    Stop,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UsageError {
    pub message: String,
    /// Whose help to point to, box's when `None`
    pub command: Option<&'static str>,
}

impl UsageError {
    fn new(message: impl Into<String>, command: Option<&'static str>) -> Self {
        Self {
            message: message.into(),
            command,
        }
    }

    /// `box: ...` and where to find the usage
    pub fn report(&self) -> String {
        let help = self
            .command
            .map_or_else(|| "box".to_string(), |command| format!("box {command}"));
        format!("box: {}\nTry '{help} --help' for usage.", self.message)
    }
}

/// Box's help, which `box` prints when it can't open the dashboard
pub fn box_help() -> String {
    help(BOX_HELP, &[HELP], Some(BOX_FOOTER))
}

pub fn parse(args: &[String]) -> Result<Command, UsageError> {
    if args.is_empty() {
        return Ok(Command::Dashboard);
    }

    // Box's options come before the command
    let mut rest = args;
    while let Some((arg, after)) = rest.split_first() {
        if arg == "--" {
            rest = after;
            break;
        }
        if !is_option(arg) {
            break;
        }
        let (flags, error) = options(arg, &[HELP]);
        if flags.contains(&'h') {
            return Ok(Command::Help(box_help()));
        }
        if let Some(message) = error {
            return Err(UsageError::new(message, None));
        }
        rest = after;
    }

    let Some((command, args)) = rest.split_first() else {
        return Ok(Command::Help(box_help()));
    };
    match command.as_str() {
        "list" => list(args),
        "prune" => prune(args),
        "stop" => stop(args),
        _ => Err(UsageError::new(format!("unknown command: {command}"), None)),
    }
}

fn list(args: &[String]) -> Result<Command, UsageError> {
    let error = |message: String| UsageError::new(message, Some("list"));
    let Some(Scanned { targets, flags }) = scan(args, &[EXTENDED, HELP]).map_err(error)? else {
        return Ok(Command::Help(help(LIST_HELP, &[EXTENDED, HELP], None)));
    };
    let Some(first) = targets.first() else {
        return Ok(Command::Help(help(LIST_HELP, &[EXTENDED, HELP], None)));
    };
    let target =
        ListTarget::from_name(first).ok_or_else(|| error(format!("unknown target: {first}")))?;
    if let Some(extra) = targets.get(1) {
        return Err(error(format!("unexpected argument: {extra}")));
    }
    let extended = flags.contains(&'x');
    if extended && target != ListTarget::Containers {
        return Err(error("--extended only works with containers".to_string()));
    }
    Ok(Command::List { target, extended })
}

fn prune(args: &[String]) -> Result<Command, UsageError> {
    let error = |message: String| UsageError::new(message, Some("prune"));
    let Some(Scanned { targets, .. }) = scan(args, &[HELP]).map_err(error)? else {
        return Ok(Command::Help(help(PRUNE_HELP, &[HELP], None)));
    };
    if targets.is_empty() {
        return Ok(Command::Help(help(PRUNE_HELP, &[HELP], None)));
    }
    let mut chosen = Vec::new();
    for target in &targets {
        let targets = PruneTarget::from_name(target)
            .ok_or_else(|| error(format!("unknown target: {target}")))?;
        chosen.extend(targets);
    }
    Ok(Command::Prune(
        PruneTarget::ORDER
            .into_iter()
            .filter(|target| chosen.contains(target))
            .collect(),
    ))
}

fn stop(args: &[String]) -> Result<Command, UsageError> {
    let error = |message: String| UsageError::new(message, Some("stop"));
    let Some(Scanned { targets, .. }) = scan(args, &[HELP]).map_err(error)? else {
        return Ok(Command::Help(help(STOP_HELP, &[HELP], None)));
    };
    if let Some(extra) = targets.first() {
        return Err(error(format!("unexpected argument: {extra}")));
    }
    Ok(Command::Stop)
}

struct Scanned {
    targets: Vec<String>,
    flags: Vec<char>,
}

/// The arguments and the options among them, `None` for help, which wins over what follows it
fn scan(args: &[String], known: &[Opt]) -> Result<Option<Scanned>, String> {
    let mut targets = Vec::new();
    let mut flags = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            targets.extend(args.cloned());
            break;
        }
        if !is_option(arg) {
            targets.push(arg.clone());
            continue;
        }
        let (matched, error) = options(arg, known);
        for flag in matched {
            if flag == 'h' {
                return Ok(None);
            }
            flags.push(flag);
        }
        if let Some(message) = error {
            return Err(message);
        }
    }
    Ok(Some(Scanned { targets, flags }))
}

fn is_option(arg: &str) -> bool {
    arg.starts_with('-') && arg != "-"
}

/// The flags an option argument sets, in order, and the error that stopped reading it
fn options(arg: &str, known: &[Opt]) -> (Vec<char>, Option<String>) {
    if let Some(long) = arg.strip_prefix("--") {
        let (name, value) = match long.split_once('=') {
            Some((name, _)) => (name, true),
            None => (long, false),
        };
        let exact = known.iter().find(|opt| opt.long == name);
        let prefixed: Vec<&Opt> = known
            .iter()
            .filter(|opt| opt.long.starts_with(name))
            .collect();
        let opt = match (exact, prefixed.as_slice()) {
            (Some(opt), _) | (None, &[opt]) => opt,
            (None, []) => return (Vec::new(), Some(format!("invalid option: {arg}"))),
            (None, _) => return (Vec::new(), Some(format!("ambiguous option: {arg}"))),
        };
        if value {
            return (Vec::new(), Some(format!("needless argument: {arg}")));
        }
        return (vec![opt.short], None);
    }

    let mut flags = Vec::new();
    for short in arg.chars().skip(1) {
        match known.iter().find(|opt| opt.short == short) {
            Some(opt) => flags.push(opt.short),
            None => return (flags, Some(format!("invalid option: -{short}"))),
        }
    }
    (flags, None)
}

/// Like `OptionParser`'s: the text, the options' summary, then the footer
fn help(text: &str, opts: &[Opt], footer: Option<&str>) -> String {
    let mut help = format!("{text}\nOptions:\n");
    for opt in opts {
        let names = format!("-{}, --{}", opt.short, opt.long);
        let _ = writeln!(help, "  {names:<OPTION_NAMES_WIDTH$} {}", opt.description);
    }
    if let Some(footer) = footer {
        let _ = write!(help, "\n{footer}\n");
    }
    help
}

#[cfg(test)]
mod tests;
