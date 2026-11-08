use clap::error::ErrorKind;

use super::*;

fn parsed(args: &str) -> Result<Command, clap::Error> {
    let args: Vec<String> = args.split_whitespace().map(ToString::to_string).collect();
    parse(&args)
}

fn error_kind(args: &str) -> ErrorKind {
    match parsed(args) {
        Err(e) => e.kind(),
        Ok(command) => panic!("{args}: {command:?}"),
    }
}

/// `--help`'s, which clap prints, or the one a command without a target asks `print_help` for
fn help_of(args: &str) -> String {
    match parsed(args) {
        Ok(Command::Help(subcommand)) => {
            let mut command = command();
            let command = match subcommand {
                Some(name) => command.find_subcommand_mut(name).expect("a subcommand"),
                None => &mut command,
            };
            command.render_help().to_string()
        }
        Err(e) if e.kind() == ErrorKind::DisplayHelp => e.render().to_string(),
        other => panic!("{args}: {other:?}"),
    }
}

#[test]
fn clap_definition() {
    command().debug_assert();
}

#[test]
fn commands() {
    assert_eq!(parsed("").unwrap(), Command::Dashboard);
    assert_eq!(
        parsed("list containers").unwrap(),
        Command::List {
            target: ListTarget::Containers,
            extended: false
        }
    );
    for extended in [
        "list -x containers",
        "list containers --extended",
        "list --e containers",
        "list containers -xx",
    ] {
        assert_eq!(
            parsed(extended).unwrap(),
            Command::List {
                target: ListTarget::Containers,
                extended: true
            },
            "{extended}"
        );
    }
    assert_eq!(
        parsed("prune images containers all").unwrap(),
        Command::Prune(PruneTarget::ORDER.to_vec())
    );
    assert_eq!(
        parsed("prune images volumes").unwrap(),
        Command::Prune(vec![PruneTarget::Volumes, PruneTarget::Images])
    );
    assert_eq!(parsed("stop").unwrap(), Command::Stop);
    assert_eq!(parsed("--").unwrap(), Command::Help(None));
    assert_eq!(parsed("list -x").unwrap(), Command::Help(Some("list")));
    assert_eq!(parsed("prune").unwrap(), Command::Help(Some("prune")));
}

#[test]
fn helps() {
    let box_help = help_of("-h");
    assert!(box_help.starts_with("Usage: box [command] [options]\n"));
    assert!(box_help.ends_with(
        "\nOptions:\n  -h, --help     Print help\n  -V, --version  Print version\n\nRun 'box command --help' for a command's options.\n"
    ));
    for args in ["--", "--he", "-hz", "-h list"] {
        assert_eq!(help_of(args), box_help, "{args}");
    }
    let list_help = help_of("list");
    assert!(list_help.ends_with(
        "  volumes     Volumes and how many containers use them\n\nOptions:\n  -x, --extended  Show containers with their IPs and mounts\n  -h, --help      Print help\n"
    ));
    for args in [
        "list -h",
        "list -xh",
        "list -x",
        "list --",
        "list containers -h foo",
    ] {
        assert_eq!(help_of(args), list_help, "{args}");
    }
    assert!(help_of("prune").starts_with("Usage: box prune WHAT...\n"));
    assert!(help_of("prune -h").starts_with("Usage: box prune WHAT...\n"));
    assert!(help_of("prune -h -z").starts_with("Usage: box prune WHAT...\n"));
    assert!(help_of("stop --help").starts_with("Usage: box stop\n"));
}

#[test]
fn version() {
    for args in ["-V", "--version", "--vers"] {
        match parsed(args) {
            Err(e) if e.kind() == ErrorKind::DisplayVersion => {
                assert_eq!(
                    e.render().to_string(),
                    format!("box {}\n", env!("CARGO_PKG_VERSION"))
                );
            }
            other => panic!("{args}: {other:?}"),
        }
    }
    assert_eq!(error_kind("list -V"), ErrorKind::UnknownArgument);
}

#[test]
fn errors() {
    for (args, kind) in [
        ("foo", ErrorKind::InvalidSubcommand),
        ("-", ErrorKind::InvalidSubcommand),
        ("-z", ErrorKind::UnknownArgument),
        // After `--` nothing is a command
        ("-- list", ErrorKind::UnknownArgument),
        ("--help=1", ErrorKind::TooManyValues),
        ("list foo", ErrorKind::InvalidValue),
        ("list containers images", ErrorKind::UnknownArgument),
        ("list --ext=1 containers", ErrorKind::TooManyValues),
        ("list --extended= containers", ErrorKind::TooManyValues),
        ("list -x1 containers", ErrorKind::UnknownArgument),
        ("list -xz", ErrorKind::UnknownArgument),
        ("list --x", ErrorKind::UnknownArgument),
        ("list ---x", ErrorKind::UnknownArgument),
        ("list -- -x", ErrorKind::InvalidValue),
        ("list -", ErrorKind::InvalidValue),
        ("prune all -z", ErrorKind::UnknownArgument),
        ("prune -z -h", ErrorKind::UnknownArgument),
        ("prune foo all", ErrorKind::InvalidValue),
        ("stop now", ErrorKind::UnknownArgument),
    ] {
        assert_eq!(error_kind(args), kind, "{args}");
    }
    let conflict = parsed("list images -x").unwrap_err();
    assert_eq!(conflict.kind(), ErrorKind::ArgumentConflict);
    assert!(
        conflict
            .render()
            .to_string()
            .starts_with("error: --extended only works with containers\n"),
        "{conflict}"
    );
}
