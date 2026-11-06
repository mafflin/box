mod cli;
mod colors;
mod commands;
mod dashboard;
mod docker;
mod format;
mod paths;
mod settings;
mod table;

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

    let result = match cli::parse(&args) {
        Err(error) => {
            eprintln!("{}", error.report());
            return ExitCode::FAILURE;
        }
        Ok(Command::Help(help)) => {
            print(&help);
            Ok(())
        }
        Ok(Command::Dashboard)
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() =>
        {
            dashboard::run()
        }
        Ok(Command::Dashboard) => {
            print(&cli::box_help());
            Ok(())
        }
        Ok(command) => commands::run(command),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // `:#` adds the causes
            eprintln!("box: {e:#}");
            ExitCode::FAILURE
        }
    }
}
