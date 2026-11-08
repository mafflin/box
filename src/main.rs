mod cli;
mod commands;
mod dashboard;
mod docker;
mod format;
mod paths;
mod settings;
mod table;
mod theme;

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

pub use anyhow::Result;

use cli::Command;

/// Ignores a closed pipe, like `head` leaves
pub fn print(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    // Clap prints help, the version and usage errors, and exits
    let command = cli::parse(&args).unwrap_or_else(|e| e.exit());
    let result = match command {
        Command::Help(subcommand) => {
            cli::print_help(subcommand);
            Ok(())
        }
        Command::Dashboard if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => {
            dashboard::run()
        }
        Command::Dashboard => {
            cli::print_help(None);
            Ok(())
        }
        command => commands::run(command),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // `:#` adds the causes
            eprintln!("{}: {e:#}", cli::PROGRAM);
            ExitCode::FAILURE
        }
    }
}
