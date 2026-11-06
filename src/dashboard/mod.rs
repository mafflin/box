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
use ratatui::crossterm::event::{self, Event, KeyEventKind};

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
    let result = event_loop(
        &mut terminal,
        &updates,
        &mut collector,
        App::new(theme, Instant::now()),
    );
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

/// Keys and resizes, from a thread that waits for them
fn read_input(actions: Sender<Action>) {
    thread::spawn(move || {
        loop {
            let action = match event::read() {
                Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => Action::Key(key),
                Ok(Event::Resize(..)) => Action::Resize,
                Ok(_) => continue,
                Err(_) => return,
            };
            if actions.send(action).is_err() {
                return;
            }
        }
    });
}
