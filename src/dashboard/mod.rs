//! `box` without a command: a live view of containers, images and volumes

mod app;
mod collector;
mod keys;
#[cfg(test)]
mod sample;
mod ui;

use std::io::{self, stdout};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};

use crate::settings::Settings;
use crate::{Result, docker, format};
use app::{Action, App, Effect};
use collector::Collector;

/// How often old messages are checked for
const TICK: Duration = Duration::from_millis(250);
/// Updates handled before drawing again, so a burst of log lines draws once
const BATCH: usize = 1000;
/// How long the input thread waits for an event before checking whether it's paused
const POLL: Duration = Duration::from_millis(50);
/// How long a lone Esc waits for the rest of a key's escape sequence, which a slow terminal sends in pieces
const ESC_WAIT: Duration = Duration::from_millis(50);
/// What `e` runs in the container: bash, else sh
const SHELL: &str = "command -v bash >/dev/null && exec bash || exec sh";
/// The most of an error from docker that's kept
const MAX_ERROR_BYTES: usize = 200;

pub fn run() -> Result<()> {
    // Before taking over the screen, so the errors show
    let settings = Settings::load()?;
    docker::check()?;

    let theme = settings.theme;
    let (actions, updates) = mpsc::channel();
    let mut collector = Collector::start(actions.clone());
    let input = read_input(actions);

    let mut terminal = ratatui::init();
    capture_mouse();
    let result = execute!(std::io::stdout(), EnableMouseCapture)
        .map_err(Into::into)
        .and_then(|()| {
            event_loop(
                &mut terminal,
                &updates,
                &mut collector,
                &input,
                App::new(theme, Instant::now()),
            )
        });
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    collector.stop();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    updates: &Receiver<Action>,
    collector: &mut Collector,
    input: &Input,
    mut app: App,
) -> Result<()> {
    let mut dirty = true;
    loop {
        if dirty {
            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            dirty = false;
        }

        let mut batch = Vec::new();
        match updates.recv_timeout(TICK) {
            Ok(action) => batch.push(action),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        while batch.len() < BATCH
            && let Ok(action) = updates.try_recv()
        {
            batch.push(action);
        }

        let now = Instant::now();
        for action in batch {
            dirty = true;
            for effect in app.update(action, now) {
                match effect {
                    Effect::Start(task) => collector.start_task(task),
                    Effect::OpenLogs { session, id } => collector.open_logs(session, &id),
                    Effect::CloseLogs { session } => collector.close_logs(session),
                    Effect::Shell { id, name } => {
                        if let Some(message) = open_shell(terminal, input, &id)? {
                            app.update(Action::ShellFailed { name, message }, Instant::now());
                        }
                        // What was done in the shell, like killing its container
                        collector.refresh();
                    }
                    Effect::Refresh => collector.refresh(),
                    Effect::Quit => return Ok(()),
                }
            }
        }
        // A shell may have taken minutes
        dirty |= app.expire(Instant::now());
    }
}

/// The wheel comes to the dashboard instead of scrolling the terminal, as in htop; a panic hook
/// gives the mouse back, as ratatui's own one doesn't
fn capture_mouse() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        hook(info);
    }));
}

/// Hands the terminal to `docker exec -it` until the shell exits, then takes it back; docker's error, if it
/// couldn't open one. Raw mode stays on, so Ctrl-C goes to the shell and never to box, and the session stays
/// on the main screen, in the scrollback
fn open_shell(terminal: &mut DefaultTerminal, input: &Input, id: &str) -> Result<Option<String>> {
    input.pause();
    terminal.show_cursor()?;
    execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen)?;
    let ran = docker::command(&["exec", "-it", id, "sh", "-c", SHELL])
        .stderr(Stdio::piped())
        .spawn()
        .and_then(Child::wait_with_output);
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.clear()?;
    input.resume();
    Ok(match ran {
        Ok(output) if output.status.success() => None,
        Ok(output) => docker_error(&output.stderr),
        Err(e) => Some(docker::cant_run(&e)),
    })
}

/// The first line docker wrote to stderr. With `-t` the shell's output goes through the terminal, so only
/// docker's own errors come here, and nothing when the shell exits with its last command's status
fn docker_error(stderr: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            format::sanitize(
                docker::without_daemon_prefix(line).as_bytes(),
                MAX_ERROR_BYTES,
            )
        })
}

/// The input thread, which can stop reading the keyboard while a shell has it
struct Input {
    paused: Arc<AtomicBool>,
    stopped: Receiver<()>,
}

impl Input {
    /// Returns once the thread stopped reading, or after a second if it's gone
    fn pause(&self) {
        while self.stopped.try_recv().is_ok() {}
        self.paused.store(true, Ordering::SeqCst);
        let _ = self.stopped.recv_timeout(Duration::from_secs(1));
    }

    fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }
}

/// Keys, the wheel and resizes, from a thread that waits for them
fn read_input(actions: Sender<Action>) -> Input {
    let paused = Arc::new(AtomicBool::new(false));
    let (stopped, acknowledged) = mpsc::channel();
    let flag = Arc::clone(&paused);
    thread::spawn(move || {
        forward(
            |timeout| {
                if event::poll(timeout)? {
                    event::read().map(Some)
                } else {
                    Ok(None)
                }
            },
            &flag,
            &stopped,
            &actions,
        );
    });
    Input {
        paused,
        stopped: acknowledged,
    }
}

/// Sends what `next` reads as actions until it fails or the dashboard's gone. While paused it doesn't read,
/// so the keys go to the shell, and it says once per pause that it stopped
fn forward(
    mut next: impl FnMut(Duration) -> io::Result<Option<Event>>,
    paused: &AtomicBool,
    stopped: &Sender<()>,
    actions: &Sender<Action>,
) {
    let mut said = false;
    loop {
        if paused.load(Ordering::SeqCst) {
            if !said {
                said = stopped.send(()).is_ok();
            }
            thread::sleep(POLL);
            continue;
        }
        said = false;
        let event = match next(POLL) {
            Ok(Some(event)) => event,
            Ok(None) => continue,
            Err(_) => return,
        };
        let events = if is_esc(&event) {
            after_esc(event, &mut next)
        } else {
            vec![event]
        };
        for event in events {
            if let Some(action) = action(&event)
                && actions.send(action).is_err()
            {
                return;
            }
        }
    }
}

/// Esc pressed alone, as the terminal's parser reads the first byte of an escape sequence the rest of which
/// is late. Its release, where the terminal tells of one, is no key of its own
fn is_esc(event: &Event) -> bool {
    matches!(event, Event::Key(key) if key.code == KeyCode::Esc
        && key.modifiers.is_empty()
        && key.kind == KeyEventKind::Press)
}

/// What `esc` was: the key whose sequence came in pieces, like ↓ as Esc, `[` and `B`, or Esc itself and
/// whatever came next when no sequence follows it, which may be another Esc that starts one. A sequence
/// that's no key box knows, or that breaks off, is dropped, so its start never counts as Esc
fn after_esc(
    mut esc: Event,
    next: &mut impl FnMut(Duration) -> io::Result<Option<Event>>,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut sequence = String::new();
    loop {
        let event = next(ESC_WAIT).ok().flatten();
        if let Some(c) = sequence_char(event.as_ref())
            && (!sequence.is_empty() || matches!(c, '[' | 'O'))
        {
            sequence.push(c);
            // After `O` comes one character, after `[` parameters and then a letter or `~`
            if sequence.len() > 1 && (sequence.starts_with('O') || ('\x40'..='\x7e').contains(&c)) {
                events.extend(sequence_key(&sequence).map(|code| Event::Key(code.into())));
                return events;
            }
            continue;
        }
        // Nothing of a sequence: Esc was itself, unless a sequence began and broke off here
        if sequence.is_empty() {
            events.push(esc);
        }
        match event {
            Some(another) if is_esc(&another) => {
                esc = another;
                sequence.clear();
            }
            other => {
                events.extend(other);
                return events;
            }
        }
    }
}

/// The character `event` adds to an escape sequence: a key pressed without Ctrl or Alt, which the bytes of
/// a sequence never come with, so Ctrl-C stays itself
fn sequence_char(event: Option<&Event>) -> Option<char> {
    match event {
        Some(Event::Key(key))
            if key.kind == KeyEventKind::Press
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            key.code.as_char()
        }
        _ => None,
    }
}

/// The key an escape sequence stands for, without its Esc. Held with Shift, Ctrl or Alt it's the same key,
/// as box tells no difference: `[1;5B` is ↓ as `[B` is
fn sequence_key(sequence: &str) -> Option<KeyCode> {
    let unmodified = match sequence.split_once(';') {
        Some((start, modifier)) => {
            let last = modifier.chars().last()?;
            let start = if last == '~' {
                start
            } else {
                start.strip_suffix('1').unwrap_or(start)
            };
            format!("{start}{last}")
        }
        None => sequence.to_string(),
    };
    Some(match unmodified.as_str() {
        "[A" | "OA" => KeyCode::Up,
        "[B" | "OB" => KeyCode::Down,
        "[C" | "OC" => KeyCode::Right,
        "[D" | "OD" => KeyCode::Left,
        "[H" | "OH" | "[1~" | "[7~" => KeyCode::Home,
        "[F" | "OF" | "[4~" | "[8~" => KeyCode::End,
        "[5~" => KeyCode::PageUp,
        "[6~" => KeyCode::PageDown,
        "[Z" => KeyCode::BackTab,
        _ => return None,
    })
}

/// The wheel moves as ↑↓ do
fn action(event: &Event) -> Option<Action> {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => Some(Action::Key(*key)),
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Some(Action::Key(KeyEvent::from(KeyCode::Up))),
            MouseEventKind::ScrollDown => Some(Action::Key(KeyEvent::from(KeyCode::Down))),
            _ => None,
        },
        Event::Resize(..) => Some(Action::Resize),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
