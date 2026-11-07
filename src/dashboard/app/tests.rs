use super::*;
use crate::dashboard::sample;

fn press(app: &mut App, keys: &str) -> Vec<Effect> {
    let mut effects = Vec::new();
    for c in keys.chars() {
        let code = match c {
            '↓' => KeyCode::Down,
            '↑' => KeyCode::Up,
            '⏎' => KeyCode::Enter,
            '⎋' => KeyCode::Esc,
            '⇥' => KeyCode::Tab,
            c => KeyCode::Char(c),
        };
        effects.extend(app.update(Action::Key(KeyEvent::from(code)), Instant::now()));
    }
    effects
}

fn selected_name(app: &mut App) -> String {
    let (rows, position) = app.selection();
    app.name_of(app.view, rows[position.unwrap()])
}

#[test]
fn sorts_running_containers_first_by_start() {
    let mut app = sample::app();
    let names = |app: &App| -> Vec<String> {
        app.rows(View::Containers)
            .iter()
            .map(|&i| app.containers[i].name.clone())
            .collect()
    };
    assert_eq!(
        app.sort[0],
        Sort {
            key: SortKey::Started,
            descending: true
        }
    );
    // The stopped worker started after db and web, but stays below them
    assert_eq!(names(&app), ["api", "console", "db", "web", "worker"]);
    press(&mut app, "T");
    assert_eq!(names(&app), ["web", "db", "console", "api", "worker"]);
    press(&mut app, "P");
    assert_eq!(names(&app), ["api", "db", "web", "console", "worker"]);
    press(&mut app, "N");
    assert_eq!(names(&app), ["api", "console", "db", "web", "worker"]);
    press(&mut app, "N");
    assert_eq!(
        app.sort[0],
        Sort {
            key: SortKey::Name,
            descending: true
        }
    );
    press(&mut app, "S");
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "containers can't be sorted by size"
    );

    press(&mut app, "T");
    // Stopped ones by when they stopped, as their status shows
    let mut snapshot = sample::snapshot();
    let worker = snapshot.containers.last().unwrap().clone();
    snapshot.containers.push(Container {
        id: "cron-id".into(),
        name: "cron".into(),
        status: "Exited (1) 1 minute ago".into(),
        started_at: worker.started_at - 240 * 3600,
        finished_at: worker.finished_at + 3 * 3600 - 60,
        ..worker
    });
    app.update(Action::Snapshot(Ok(snapshot)), Instant::now());
    assert_eq!(
        names(&app),
        ["api", "console", "db", "web", "cron", "worker"]
    );
}

#[test]
fn selection_follows_the_item_when_rows_move() {
    let mut app = sample::app();
    press(&mut app, "Pj");
    assert_eq!(selected_name(&mut app), "db");
    // db overtakes api
    let db = app
        .containers
        .iter()
        .find(|c| c.name == "db")
        .unwrap()
        .id
        .clone();
    app.update(
        Action::Stats(Stats {
            id: db,
            cpu: 99.0,
            ..Stats::default()
        }),
        Instant::now(),
    );
    assert_eq!(app.rows(View::Containers)[0], 1);
    assert_eq!(selected_name(&mut app), "db");
    press(&mut app, "G");
    assert_eq!(selected_name(&mut app), "worker");
}

#[test]
fn filters_and_hides_stopped_containers() {
    let mut app = sample::app();
    press(&mut app, "/post");
    assert!(app.filtering);
    assert_eq!(app.rows(View::Containers).len(), 1);
    press(&mut app, "⏎");
    assert!(!app.filtering);
    press(&mut app, "⎋a");
    assert_eq!(app.rows(View::Containers).len(), 4);
    press(&mut app, "⇥");
    assert_eq!(app.view, View::Images);
}

#[test]
fn opens_a_shell_in_running_containers() {
    let mut app = sample::app();
    assert_eq!(selected_name(&mut app), "api");
    let shell = Effect::Shell {
        id: "api-id".into(),
        name: "api".into(),
    };
    assert_eq!(press(&mut app, "e"), [shell]);

    let failed = |message: &str| Action::ShellFailed {
        name: "api".into(),
        message: message.into(),
    };
    let missing = r#"OCI runtime exec failed: exec: "sh": executable file not found in $PATH"#;
    assert_eq!(app.update(failed(missing), Instant::now()), []);
    assert_eq!(app.toast.as_ref().unwrap().text, "no shell in api");
    let stopped = "container api-id is not running";
    app.update(failed(stopped), Instant::now());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        format!("can't open a shell in api: {stopped}")
    );

    // Not while it's being stopped
    press(&mut app, "sy");
    assert_eq!(press(&mut app, "e"), []);

    press(&mut app, "G");
    assert_eq!(selected_name(&mut app), "worker");
    assert_eq!(press(&mut app, "e"), []);
    assert_eq!(app.toast.as_ref().unwrap().text, "worker is not running");
    assert_eq!(press(&mut app, "2e"), []);
}

#[test]
fn filters_images_by_the_platform_shown() {
    let mut app = sample::app();
    press(&mut app, "2/amd64⏎");
    assert_eq!(app.rows(View::Images).len(), 1);
    press(&mut app, "⎋/arm64⏎");
    assert_eq!(app.rows(View::Images), Vec::<usize>::new());
}

#[test]
fn stops_after_confirming() {
    let mut app = sample::app();
    assert_eq!(press(&mut app, "s"), []);
    assert!(matches!(app.modal, Some(Modal::Confirm(ref c)) if c.command == "docker stop api-id"));
    let effects = press(&mut app, "y");
    let [Effect::Start(Task::Run { args, .. })] = &effects[..] else {
        panic!("{effects:?}");
    };
    assert_eq!(args, &["stop", "api-id"]);
    let api = app.containers[0].clone();
    assert_eq!(app.pending(&api), Some(Pending::Stopping));
    // Pressing again does nothing until docker answers
    assert_eq!(press(&mut app, "s"), []);
    assert!(app.modal.is_none());

    let task = Task::Run {
        args: vec![],
        container: Some((api.id.clone(), Pending::Stopping)),
        done: String::new(),
    };
    let effects = app.update(
        Action::Done {
            task,
            result: Ok("stopped api".into()),
        },
        Instant::now(),
    );
    assert_eq!(effects, [Effect::Refresh]);
    assert_eq!(app.pending(&api), Some(Pending::Stopping));
    app.update(Action::Snapshot(Ok(sample::snapshot())), Instant::now());
    assert_eq!(app.pending(&api), None);
}

#[test]
fn acts_on_a_container_by_its_id() {
    let mut app = sample::app();
    let id = format!("3f2a1b4c5d6e{}", "0".repeat(52));
    app.containers[0].id.clone_from(&id);
    press(&mut app, "d");
    assert!(
        matches!(app.modal, Some(Modal::Confirm(ref c)) if c.command == "docker rm -f -v 3f2a1b4c5d6e")
    );
    let effects = press(&mut app, "y");
    let [Effect::Start(Task::Run { args, .. })] = &effects[..] else {
        panic!("{effects:?}");
    };
    assert_eq!(args, &["rm", "-f", "-v", id.as_str()]);
}

#[test]
fn removes_unused_images_only() {
    let mut app = sample::app();
    press(&mut app, "2");
    assert_eq!(selected_name(&mut app), "<none>");
    press(&mut app, "d");
    assert!(
        matches!(app.modal, Some(Modal::Confirm(ref c)) if c.command == "docker rmi 0123456789ab")
    );
    press(&mut app, "n");
    press(&mut app, "jd");
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "acme/api:dev is used by 2 containers"
    );
    assert!(app.modal.is_none());
}

#[test]
fn no_logs_for_interactive_containers() {
    let mut app = sample::app();
    press(&mut app, "N");
    press(&mut app, "j");
    assert_eq!(selected_name(&mut app), "console");
    assert_eq!(press(&mut app, "l"), []);
    assert!(app.logs.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "console is interactive (-it), logs are not shown"
    );

    press(&mut app, "k");
    let effects = press(&mut app, "l");
    assert!(matches!(
        &effects[..],
        [Effect::OpenLogs { session: 1, .. }]
    ));
    // It's recreated as interactive
    let mut snapshot = sample::snapshot();
    snapshot.containers[0].tty = true;
    let effects = app.update(Action::Snapshot(Ok(snapshot)), Instant::now());
    assert_eq!(effects, [Effect::CloseLogs { session: 1 }]);
    assert!(app.logs.is_none());
}

#[test]
fn logs_follow_and_scroll() {
    let mut app = sample::app();
    app.log_page = 3;
    press(&mut app, "l");
    for i in 0..10 {
        let line = LogLine::parse(format!("2026-11-06T13:08:{i:02}Z line {i}").as_bytes());
        app.update(Action::Log { session: 1, line }, Instant::now());
    }
    app.update(
        Action::Log {
            session: 2,
            line: LogLine::parse(b"other"),
        },
        Instant::now(),
    );
    let logs = app.logs.as_ref().unwrap();
    assert_eq!(logs.lines.len(), 10);
    assert_eq!(logs.view, None);
    press(&mut app, "kk");
    assert_eq!(app.logs.as_ref().unwrap().view, Some(8));
    press(&mut app, "g");
    assert_eq!(app.logs.as_ref().unwrap().view, Some(3));
    press(&mut app, "G ");
    assert_eq!(app.logs.as_ref().unwrap().view, Some(10));
    assert_eq!(press(&mut app, "⎋"), [Effect::CloseLogs { session: 1 }]);
}

#[test]
fn prunes_the_chosen_targets() {
    let mut app = sample::app();
    press(&mut app, "p");
    press(&mut app, "j ");
    assert_eq!(
        press(&mut app, "⏎"),
        [Effect::Start(Task::Prune(vec![
            PruneTarget::Containers,
            PruneTarget::Images
        ]))]
    );
    assert_eq!(app.prunable(), [(1, 0), (1, 0), (1, 412_000_000)]);
    // Docker won't remove a dangling image a container uses
    let dangling = app
        .images
        .iter()
        .find(|image| image.is_dangling())
        .unwrap()
        .id
        .clone();
    app.containers[0].image_id = dangling;
    assert_eq!(app.prunable()[2], (0, 0));
}

#[test]
fn orders_log_lines_by_time() {
    let mut app = sample::app();
    press(&mut app, "l");
    for raw in [
        "2026-11-06T13:00:01Z a",
        "2026-11-06T13:00:03Z c",
        "2026-11-06T13:00:02Z b",
        "no time",
    ] {
        app.update(
            Action::Log {
                session: 1,
                line: LogLine::parse(raw.as_bytes()),
            },
            Instant::now(),
        );
    }
    let texts: Vec<&str> = app
        .logs
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(texts, ["a", "b", "c", "no time"]);
}

#[test]
fn log_lines() {
    let line = LogLine::parse(b"2026-11-06T13:08:42.123456789Z \x1b[31mERROR\x1b[0m boom\r");
    assert_eq!(line.text, "ERROR boom");
    assert_eq!(line.level, Level::Error);
    assert_eq!(line.time.len(), 8);
    let line = LogLine::parse(b"no timestamp level=warn");
    assert_eq!(
        (line.time.as_str(), line.text.as_str(), line.level),
        ("", "no timestamp level=warn", Level::Warning)
    );
    assert_eq!(LogLine::parse(b"0 errors").level, Level::Info);
    // In capitals or lowercase, every level word; mixed case is prose
    assert_eq!(LogLine::parse(b"critical: disk full").level, Level::Error);
    assert_eq!(LogLine::parse(b"[trace] tick").level, Level::Debug);
    assert_eq!(LogLine::parse(b"Error handling is on").level, Level::Info);
}

#[test]
fn disconnects_and_reconnects() {
    let mut app = sample::app();
    app.update(
        Action::Snapshot(Err("Cannot connect to the Docker daemon".into())),
        Instant::now(),
    );
    assert!(app.disconnected.is_some());
    assert_eq!(app.containers.len(), 5);
    app.update(Action::Snapshot(Ok(sample::snapshot())), Instant::now());
    assert!(app.disconnected.is_none());
}
