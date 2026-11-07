//! Running the docker command

use std::io::Write;
use std::process::{Command, Output, Stdio};

use anyhow::{anyhow, bail};

use crate::Result;

pub const DOCKER: &str = "docker";
/// Before most of docker's errors, which box leaves out
const DAEMON_ERROR: &str = "Error response from daemon: ";

/// Removes a container with its anonymous volumes, as `box prune` and the dashboard do
pub const REMOVE_CONTAINER: [&str; 2] = ["rm", "-v"];
/// What lets `REMOVE_CONTAINER` remove a running one
pub const FORCE: &str = "-f";
pub const REMOVE_VOLUME: [&str; 2] = ["volume", "rm"];
pub const REMOVE_IMAGE: [&str; 1] = ["rmi"];
/// Every volume's name
pub const LIST_VOLUMES: [&str; 3] = ["volume", "ls", "-q"];
pub const INSPECT_VOLUMES: [&str; 2] = ["volume", "inspect"];

/// Runs docker, which `box list`, `prune` and `stop` share with the dashboard
pub trait Docker {
    /// Stdout's lines, an error when docker fails
    fn lines(&self, args: &[&str]) -> Result<Vec<String>>;

    /// An error when docker fails
    fn run(&self, args: &[&str]) -> Result<()>;
}

/// For the command line: docker's own output and errors show as they come
pub struct Terminal;

impl Docker for Terminal {
    fn lines(&self, args: &[&str]) -> Result<Vec<String>> {
        let output = command(args)
            .stderr(Stdio::inherit())
            .output()
            .map_err(|_| failed(args))?;
        if !output.status.success() {
            return Err(failed(args));
        }
        Ok(lines(&output))
    }

    fn run(&self, args: &[&str]) -> Result<()> {
        // Box's output comes before docker's
        let _ = std::io::stdout().flush();
        match command(args).status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(failed(args)),
        }
    }
}

/// For the dashboard: nothing reaches the terminal, errors carry docker's message
pub struct Captured;

impl Docker for Captured {
    fn lines(&self, args: &[&str]) -> Result<Vec<String>> {
        let output = captured(args)?;
        Ok(lines(&output))
    }

    fn run(&self, args: &[&str]) -> Result<()> {
        captured(args).map(|_| ())
    }
}

/// Stdout even when docker fails, as long as it printed something, like inspect for a container that's gone
fn partial_output(args: &[&str]) -> Result<String> {
    let output = command(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| anyhow!(cant_run(&e)))?;
    if output.status.success() || !output.stdout.is_empty() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(error_message(args, &output))
}

/// `command` followed by every ID or name, as `partial_output` gives it; empty for none
pub fn inspect_all(command: &[&str], ids: &[String]) -> Result<String> {
    if ids.is_empty() {
        return Ok(String::new());
    }
    let mut args = command.to_vec();
    args.extend(ids.iter().map(String::as_str));
    partial_output(&args)
}

/// Arguments for a command run later, on another thread
pub fn args(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

/// A docker error without the `Error response from daemon: ` most start with
pub fn without_daemon_prefix(line: &str) -> &str {
    line.strip_prefix(DAEMON_ERROR).unwrap_or(line)
}

/// Why docker couldn't start, like when it isn't installed
pub fn cant_run(error: &std::io::Error) -> String {
    format!("can't run docker: {error}")
}

/// Docker is installed and its daemon running
pub fn check() -> Result<()> {
    if !succeeds(&["--version"]) {
        bail!("{DOCKER} is not installed");
    }
    if !succeeds(&["info"]) {
        bail!("{DOCKER} is not running");
    }
    Ok(())
}

pub fn command(args: &[&str]) -> Command {
    let mut command = Command::new(DOCKER);
    command.args(args);
    command
}

fn succeeds(args: &[&str]) -> bool {
    command(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn captured(args: &[&str]) -> Result<Output> {
    let output = command(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| anyhow!(cant_run(&e)))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(error_message(args, &output))
    }
}

/// Docker's first error line, like `Error response from daemon: ...`
fn error_message(args: &[&str], output: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    match stderr.lines().map(str::trim).find(|line| !line.is_empty()) {
        Some(line) => anyhow!("{}", without_daemon_prefix(line)),
        None => failed(args),
    }
}

fn failed(args: &[&str]) -> anyhow::Error {
    anyhow!("{DOCKER} {} failed", args.join(" "))
}

fn lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
pub mod fake {
    //! A docker that answers from a script and records what it ran

    use std::cell::RefCell;

    use super::{DOCKER, Docker};
    use crate::Result;

    #[derive(Default)]
    pub struct Fake {
        /// Argument prefixes and their output lines, `None` to fail; the longest matching prefix wins
        pub responses: Vec<(Vec<String>, Option<Vec<String>>)>,
        pub calls: RefCell<Vec<Vec<String>>>,
    }

    impl Fake {
        pub fn respond(&mut self, args: &[&str], lines: &[&str]) {
            self.responses.push((
                args.iter().map(ToString::to_string).collect(),
                Some(lines.iter().map(ToString::to_string).collect()),
            ));
        }

        pub fn fail(&mut self, args: &[&str]) {
            self.responses
                .push((args.iter().map(ToString::to_string).collect(), None));
        }

        fn answer(&self, args: &[&str]) -> Result<Vec<String>> {
            self.calls
                .borrow_mut()
                .push(args.iter().map(ToString::to_string).collect());
            let response = self
                .responses
                .iter()
                .filter(|(prefix, _)| {
                    prefix.len() <= args.len() && prefix.iter().zip(args).all(|(a, b)| a == b)
                })
                .max_by_key(|(prefix, _)| prefix.len());
            match response {
                Some((_, None)) => anyhow::bail!("{DOCKER} {} failed", args.join(" ")),
                Some((_, Some(lines))) => Ok(lines.clone()),
                None => Ok(Vec::new()),
            }
        }
    }

    impl Docker for Fake {
        fn lines(&self, args: &[&str]) -> Result<Vec<String>> {
            self.answer(args)
        }

        fn run(&self, args: &[&str]) -> Result<()> {
            self.answer(args).map(|_| ())
        }
    }
}
