//! `box` without a command: a live view of containers, images and volumes

mod app;
mod collector;
#[cfg(test)]
mod sample;
mod theme;
mod ui;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseEventKind,
};
use ratatui::crossterm::execute;

use crate::settings::Settings;
use crate::{Result, docker};
use app::{Action, App, Effect};
use collector::Collector;
use theme::Theme;

/// How often old messages are checked for
const TICK: Duration = Duration::from_millis(250);
/// Updates handled before drawing again, so a burst of log lines draws once
const BATCH: usize = 1000;

pub fn run() -> Result<()> {
    // Before taking over the screen, so the errors show
    let settings = Settings::load()?;
    docker::check()?;

    let theme = Theme::new(settings.accent);
    let (actions, updates) = mpsc::channel();
    let mut collector = Collector::start(actions.clone());
    read_input(actions);

    let mut terminal = ratatui::init();
    capture_mouse();
    let result = execute!(std::io::stdout(), EnableMouseCapture)
        .map_err(Into::into)
        .and_then(|()| {
            event_loop(
                &mut terminal,
                &updates,
                &mut collector,
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
                    Effect::Refresh => collector.refresh(),
                    Effect::Quit => return Ok(()),
                }
            }
        }
        dirty |= app.expire(now);
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

/// Keys, the wheel and resizes, from a thread that waits for them
fn read_input(actions: Sender<Action>) {
    thread::spawn(move || {
        loop {
            let Ok(event) = event::read() else { return };
            if let Some(action) = action(&event)
                && actions.send(action).is_err()
            {
                return;
            }
        }
    });
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
mod tests {
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
}
