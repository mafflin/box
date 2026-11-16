use ratatui::crossterm::event::{KeyEventState, MouseButton, MouseEvent};

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

/// The keys that come of `events` read one by one, with nothing after them
fn forwarded(events: Vec<Event>) -> Vec<KeyCode> {
    let (stopped, _acknowledged) = mpsc::channel();
    let (actions, received) = mpsc::channel();
    let mut events = events.into_iter();
    let mut ended = false;
    forward(
        |_| match events.next() {
            Some(event) => Ok(Some(event)),
            // Once more after the last one, as a sequence waits for its rest
            None if !ended => {
                ended = true;
                Ok(None)
            }
            None => Err(io::Error::other("gone")),
        },
        &AtomicBool::new(false),
        &stopped,
        &actions,
    );
    received
        .try_iter()
        .filter_map(|action| match action {
            Action::Key(key) => Some(key.code),
            _ => None,
        })
        .collect()
}

/// Esc and then `text`, a key each, as a sequence comes when the terminal sent its Esc ahead
fn in_pieces(text: &str) -> Vec<Event> {
    let mut events = vec![Event::Key(KeyEvent::from(KeyCode::Esc))];
    for c in text.chars() {
        let modifiers = if c.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        events.push(Event::Key(KeyEvent::new(KeyCode::Char(c), modifiers)));
    }
    events
}

#[test]
fn a_sequence_in_pieces_is_its_key() {
    for (sequence, expected) in [
        ("[A", KeyCode::Up),
        ("[B", KeyCode::Down),
        ("OB", KeyCode::Down),
        ("[C", KeyCode::Right),
        ("[D", KeyCode::Left),
        ("[H", KeyCode::Home),
        ("[4~", KeyCode::End),
        ("[5~", KeyCode::PageUp),
        ("[6~", KeyCode::PageDown),
        ("[Z", KeyCode::BackTab),
        // Held with Ctrl or Shift
        ("[1;5B", KeyCode::Down),
        ("[1;2D", KeyCode::Left),
        ("[5;5~", KeyCode::PageUp),
        ("[1;5~", KeyCode::Home),
    ] {
        assert_eq!(forwarded(in_pieces(sequence)), [expected], "{sequence}");
    }
    // The keys after it are their own
    let mut events = in_pieces("[B");
    events.push(Event::Key(KeyEvent::from(KeyCode::Esc)));
    events.push(Event::Key(KeyEvent::from(KeyCode::Char('j'))));
    assert_eq!(
        forwarded(events),
        [KeyCode::Down, KeyCode::Esc, KeyCode::Char('j')]
    );
}

#[test]
fn esc_alone_is_esc() {
    assert_eq!(forwarded(in_pieces("")), [KeyCode::Esc]);
    assert_eq!(
        forwarded(in_pieces("q")),
        [KeyCode::Esc, KeyCode::Char('q')]
    );
    assert_eq!(
        forwarded(vec![
            Event::Key(KeyEvent::from(KeyCode::Esc)),
            Event::Key(KeyEvent::from(KeyCode::Esc)),
        ]),
        [KeyCode::Esc, KeyCode::Esc]
    );
}

#[test]
fn a_sequence_thats_no_key_is_dropped() {
    // The mouse moving, a key box doesn't know, and one that breaks off
    for sequence in ["[<35;10;5M", "[15~", "[15;5~", "OP", "[1;5"] {
        assert_eq!(forwarded(in_pieces(sequence)), [], "{sequence}");
    }
}

#[test]
fn esc_before_a_sequence_in_pieces_is_esc() {
    let mut events = in_pieces("");
    events.extend(in_pieces("[B"));
    assert_eq!(forwarded(events), [KeyCode::Esc, KeyCode::Down]);
    // A sequence that broke off, and then a whole one
    let mut events = in_pieces("[");
    events.extend(in_pieces("[A"));
    assert_eq!(forwarded(events), [KeyCode::Up]);
}

#[test]
fn a_key_with_ctrl_ends_a_sequence() {
    let interrupt = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let mut events = in_pieces("[");
    events.push(Event::Key(interrupt));
    let (stopped, _acknowledged) = mpsc::channel();
    let (actions, received) = mpsc::channel();
    let mut events = events.into_iter();
    forward(
        |_| events.next().map(Some).ok_or(io::Error::other("gone")),
        &AtomicBool::new(false),
        &stopped,
        &actions,
    );
    let keys: Vec<KeyEvent> = received
        .try_iter()
        .filter_map(|action| match action {
            Action::Key(key) => Some(key),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [interrupt]);
}

#[test]
fn an_esc_released_is_no_key() {
    let release = Event::Key(KeyEvent::new_with_kind_and_state(
        KeyCode::Esc,
        KeyModifiers::NONE,
        KeyEventKind::Release,
        KeyEventState::NONE,
    ));
    // Alone, as it comes when Esc was held longer than a sequence is waited for
    assert_eq!(forwarded(vec![release.clone()]), []);
    let mut events = in_pieces("");
    events.push(release);
    assert_eq!(forwarded(events), [KeyCode::Esc]);
}
