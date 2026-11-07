use ratatui::crossterm::event::{KeyEventState, KeyModifiers, MouseButton, MouseEvent};

use super::*;

fn mouse(kind: MouseEventKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
}

/// The key `event` comes as, if any
fn key(event: &Event) -> Option<KeyCode> {
    match action(event) {
        Some(Action::Key(key)) => Some(key.code),
        _ => None,
    }
}

#[test]
fn the_wheel_moves_as_the_arrows_do() {
    assert_eq!(key(&mouse(MouseEventKind::ScrollUp)), Some(KeyCode::Up));
    assert_eq!(key(&mouse(MouseEventKind::ScrollDown)), Some(KeyCode::Down));
    for other in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Moved,
        MouseEventKind::ScrollLeft,
    ] {
        assert!(action(&mouse(other)).is_none());
    }
}

#[test]
fn stops_reading_while_paused() {
    let paused = Arc::new(AtomicBool::new(true));
    let finished = Arc::new(AtomicBool::new(false));
    let (stopped, acknowledged) = mpsc::channel();
    let (actions, received) = mpsc::channel();
    let (reads, read) = mpsc::channel();
    let (flag, done) = (Arc::clone(&paused), Arc::clone(&finished));
    let reader = thread::spawn(move || {
        let mut keys = "ab".chars();
        forward(
            |timeout| {
                if done.load(Ordering::SeqCst) {
                    return Err(io::Error::other("gone"));
                }
                let _ = reads.send(());
                let key = keys.next();
                if key.is_none() {
                    thread::sleep(timeout);
                }
                Ok(key.map(|c| Event::Key(KeyEvent::from(KeyCode::Char(c)))))
            },
            &flag,
            &stopped,
            &actions,
        );
    });

    // Paused from the start: it says so once and reads nothing
    let wait = Duration::from_secs(1);
    acknowledged.recv_timeout(wait).unwrap();
    thread::sleep(POLL * 3);
    assert!(read.try_recv().is_err());
    assert!(acknowledged.try_recv().is_err());

    paused.store(false, Ordering::SeqCst);
    for expected in ['a', 'b'] {
        let Action::Key(key) = received.recv_timeout(wait).unwrap() else {
            panic!("not a key");
        };
        assert_eq!(key.code, KeyCode::Char(expected));
    }
    paused.store(true, Ordering::SeqCst);
    acknowledged.recv_timeout(wait).unwrap();

    finished.store(true, Ordering::SeqCst);
    paused.store(false, Ordering::SeqCst);
    reader.join().unwrap();
}

#[test]
fn docker_errors() {
    let error =
        b"\nError response from daemon: container abc is not running\nSee 'docker exec --help'.\n";
    // Without the prefix, as box shows docker's other errors
    assert_eq!(
        docker_error(error).as_deref(),
        Some("container abc is not running")
    );
    assert_eq!(
        docker_error(b"\x1b]0;title\x07oops\n").as_deref(),
        Some("oops")
    );
    assert_eq!(docker_error(b""), None);
    assert_eq!(docker_error(b"\n  \n"), None);
}

#[test]
fn keys_count_once() {
    let press = KeyEvent::from(KeyCode::Char('q'));
    let release = KeyEvent::new_with_kind_and_state(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
        KeyEventState::NONE,
    );
    assert_eq!(key(&Event::Key(press)), Some(KeyCode::Char('q')));
    assert!(action(&Event::Key(release)).is_none());
    assert!(matches!(
        action(&Event::Resize(80, 24)),
        Some(Action::Resize)
    ));
}
