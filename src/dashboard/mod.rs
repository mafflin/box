//! `box` without a command: a live view of containers, images and volumes

mod app;
mod collector;
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
    MouseEventKind,
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
/// What `e` runs in the container: bash, else sh
const SHELL: &str = "command -v bash >/dev/null && exec bash || exec sh";

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
        Err(e) => Some(format!("can't run docker: {e}")),
    })
}

/// The first line docker wrote to stderr. With `-t` the shell's output goes through the terminal, so only
/// docker's own errors come here, and nothing when the shell exits with its last command's status
fn docker_error(stderr: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| format::sanitize(line.as_bytes(), 200))
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
        if let Some(action) = action(&event)
            && actions.send(action).is_err()
        {
            return;
        }
    }
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
