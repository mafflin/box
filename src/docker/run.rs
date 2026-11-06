//! Running the docker command

use std::io::Write;
use std::process::{Command, Output, Stdio};

use anyhow::{anyhow, bail};

use crate::Result;

pub const DOCKER: &str = "docker";

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
pub fn partial_output(args: &[&str]) -> Result<String> {
    let output = command(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| anyhow!("can't run docker: {e}"))?;
    if output.status.success() || !output.stdout.is_empty() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(error_message(args, &output))
}

/// Docker is installed and its daemon running
pub fn check() -> Result<()> {
    if !succeeds(&["--version"]) {
        bail!("docker is not installed");
    }
    if !succeeds(&["info"]) {
        bail!("docker is not running");
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
        .map_err(|e| anyhow!("can't run docker: {e}"))?;
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
        Some(line) => anyhow!(
            "{}",
            line.strip_prefix("Error response from daemon: ")
                .unwrap_or(line)
        ),
        None => failed(args),
    }
}

fn failed(args: &[&str]) -> anyhow::Error {
    anyhow!("docker {} failed", args.join(" "))
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

    use super::Docker;
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
                Some((_, None)) => anyhow::bail!("docker {} failed", args.join(" ")),
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
