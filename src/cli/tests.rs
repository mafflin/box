use super::*;

fn parsed(args: &str) -> Result<Command, UsageError> {
    let args: Vec<String> = args.split_whitespace().map(ToString::to_string).collect();
    parse(&args)
}

fn error(args: &str) -> String {
    parsed(args).unwrap_err().report()
}

fn help_of(args: &str) -> String {
    match parsed(args) {
        Ok(Command::Help(help)) => help,
        other => panic!("{args}: {other:?}"),
    }
}

#[test]
fn commands() {
    assert_eq!(parsed(""), Ok(Command::Dashboard));
    assert_eq!(
        parsed("list containers"),
        Ok(Command::List {
            target: ListTarget::Containers,
            extended: false
        })
    );
    for extended in [
        "list -x containers",
        "list containers --extended",
        "list --e containers",
        "list containers --",
    ] {
        assert!(
            matches!(parsed(extended), Ok(Command::List { .. })),
            "{extended}"
        );
    }
    assert_eq!(
        parsed("prune images containers all"),
        Ok(Command::Prune(PruneTarget::ORDER.to_vec()))
    );
    assert_eq!(
        parsed("prune images volumes"),
        Ok(Command::Prune(vec![
            PruneTarget::Volumes,
            PruneTarget::Images
        ]))
    );
    assert_eq!(parsed("stop"), Ok(Command::Stop));
}

#[test]
fn helps() {
    let box_help = help_of("-h");
    assert!(box_help.starts_with("Usage: box [command] [options]\n"));
    assert!(box_help.ends_with(
        "\nOptions:\n  -h, --help      Print help\n\nRun 'box command --help' for a command's options.\n"
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
        "list --",
        "-- list",
        "list containers -h foo",
    ] {
        assert_eq!(help_of(args), list_help, "{args}");
    }
    assert!(help_of("prune").starts_with("Usage: box prune WHAT...\n"));
    assert!(help_of("prune -h -z").starts_with("Usage: box prune WHAT...\n"));
    assert!(help_of("stop --help").starts_with("Usage: box stop\n"));
}

#[test]
fn errors() {
    for (args, message) in [
        (
            "foo",
            "box: unknown command: foo\nTry 'box --help' for usage.",
        ),
        ("-", "box: unknown command: -\nTry 'box --help' for usage."),
        ("-z", "box: invalid option: -z\nTry 'box --help' for usage."),
        (
            "--help=1",
            "box: needless argument: --help=1\nTry 'box --help' for usage.",
        ),
        (
            "list foo",
            "box: unknown target: foo\nTry 'box list --help' for usage.",
        ),
        (
            "list containers images",
            "box: unexpected argument: images\nTry 'box list --help' for usage.",
        ),
        (
            "list images -x",
            "box: --extended only works with containers\nTry 'box list --help' for usage.",
        ),
        (
            "list --ext=1 containers",
            "box: needless argument: --ext=1\nTry 'box list --help' for usage.",
        ),
        (
            "list --extended= containers",
            "box: needless argument: --extended=\nTry 'box list --help' for usage.",
        ),
        (
            "list -x1 containers",
            "box: invalid option: -1\nTry 'box list --help' for usage.",
        ),
        (
            "list -xz",
            "box: invalid option: -z\nTry 'box list --help' for usage.",
        ),
        (
            "list ---x",
            "box: invalid option: ---x\nTry 'box list --help' for usage.",
        ),
        (
            "list --x",
            "box: invalid option: --x\nTry 'box list --help' for usage.",
        ),
        (
            "list -- -x",
            "box: unknown target: -x\nTry 'box list --help' for usage.",
        ),
        (
            "list -",
            "box: unknown target: -\nTry 'box list --help' for usage.",
        ),
        (
            "prune all -z",
            "box: invalid option: -z\nTry 'box prune --help' for usage.",
        ),
        (
            "prune -z -h",
            "box: invalid option: -z\nTry 'box prune --help' for usage.",
        ),
        (
            "prune foo all",
            "box: unknown target: foo\nTry 'box prune --help' for usage.",
        ),
        (
            "stop now",
            "box: unexpected argument: now\nTry 'box stop --help' for usage.",
        ),
    ] {
        assert_eq!(error(args), message, "{args}");
    }
}
