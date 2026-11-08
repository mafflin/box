//! The last line: the keys that work now, or the filter being typed; help and quit or a message on the right

use std::cmp::Reverse;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::Span;

use super::{BAR_MARGIN, bold, fg, put, put_right, size, width};
use crate::dashboard::app::{App, Logs, Tone, View};
use crate::dashboard::keys::{self, names};
use crate::docker::Container;
use crate::theme::Theme;

/// Between the selection's keys and the list's
const SEPARATOR: &str = "  │  ";
/// Between two keys
const HINT_GAP: u16 = 2;
/// Between a key and what it does
const KEY_GAP: u16 = 1;
/// The end of the filter being typed
const CURSOR: &str = "▏";
/// Between the filter and the hints
const FILTER_GAP: u16 = 3;

/// A key, what it does, whether it does it for the selection, and how long it stays when the line is too
/// short: the one with the lowest `keep` goes first, so the higher it is, the longer it stays
struct Hint {
    key: String,
    description: &'static str,
    enabled: bool,
    keep: u8,
}

/// `key` as `keys` names it
fn hint(key: impl Into<String>, description: &'static str, keep: u8) -> Hint {
    Hint {
        key: key.into(),
        description,
        enabled: true,
        keep,
    }
}

impl Hint {
    /// Dimmed when it doesn't work for the selection
    fn enabled_if(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    fn width(&self) -> usize {
        width(&self.key) + usize::from(KEY_GAP) + width(self.description)
    }
}

/// Always on the right, unless a message is
fn pinned_hints() -> [Hint; 2] {
    [
        hint(names(&[keys::HELP]), "help", u8::MAX),
        hint(names(&[keys::QUIT]), "quit", u8::MAX),
    ]
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;

    let toast = toast(app);
    let (mut groups, pinned) = hints(app);
    let right = match (&toast, pinned) {
        (Some((text, _)), _) => width(text) + usize::from(BAR_MARGIN),
        (None, true) => line_width(&[&pinned_hints()[..]]) + usize::from(BAR_MARGIN),
        (None, false) => 0,
    };

    let mut x = area.x + BAR_MARGIN;
    if app.filtering {
        x = draw_filter(buf, x, area, app) + FILTER_GAP;
    }
    let room =
        usize::from(area.right().saturating_sub(x)).saturating_sub(right + usize::from(HINT_GAP));
    fit(&mut groups, room);

    let groups: Vec<&[Hint]> = groups.iter().map(Vec::as_slice).collect();
    x = draw_hints(buf, x, area, &groups, &theme);
    if let Some((text, color)) = toast {
        put_right(
            buf,
            area.right().saturating_sub(BAR_MARGIN),
            area.y,
            &[Span::styled(text, bold(color))],
        );
    } else if pinned {
        let start = area.right().saturating_sub(size(right));
        draw_hints(buf, start.max(x), area, &[&pinned_hints()[..]], &theme);
    }
}

/// The message to show, in its tone's color
fn toast(app: &App) -> Option<(String, Color)> {
    let theme = app.theme;
    app.toast.as_ref().map(|toast| {
        let color = match toast.tone {
            Tone::Info => theme.text,
            Tone::Good => theme.green,
            Tone::Warn => theme.yellow,
            Tone::Bad => theme.red,
        };
        (toast.text.clone(), color)
    })
}

/// `filter text▏` from `x`; where it ends
fn draw_filter(buf: &mut Buffer, x: u16, area: Rect, app: &App) -> u16 {
    let theme = app.theme;
    let filter = &app.filter[app.view.index()];
    let x = put(buf, x, area.y, "filter ", fg(theme.muted), area.right());
    let x = put(buf, x, area.y, filter, bold(theme.text), area.right());
    put(buf, x, area.y, CURSOR, bold(theme.accent), area.right())
}

/// The hints from `x`, a separator between groups; where they end
fn draw_hints(buf: &mut Buffer, mut x: u16, area: Rect, groups: &[&[Hint]], theme: &Theme) -> u16 {
    let shown: Vec<&[Hint]> = groups
        .iter()
        .copied()
        .filter(|group| !group.is_empty())
        .collect();
    for (i, group) in shown.iter().enumerate() {
        if i > 0 {
            x = put(buf, x, area.y, SEPARATOR, fg(theme.dim), area.right());
        }
        for (j, hint) in group.iter().enumerate() {
            if j > 0 {
                x += HINT_GAP;
            }
            let (key_style, description_style) = if hint.enabled {
                (bold(theme.accent), fg(theme.muted))
            } else {
                (fg(theme.dim), fg(theme.dim))
            };
            x = put(buf, x, area.y, &hint.key, key_style, area.right());
            x = put(
                buf,
                x + KEY_GAP,
                area.y,
                hint.description,
                description_style,
                area.right(),
            );
        }
    }
    x
}

fn line_width(groups: &[&[Hint]]) -> usize {
    let shown: Vec<&&[Hint]> = groups.iter().filter(|group| !group.is_empty()).collect();
    let hints: usize = shown
        .iter()
        .map(|group| {
            group.iter().map(Hint::width).sum::<usize>() + usize::from(HINT_GAP) * (group.len() - 1)
        })
        .sum();
    hints + width(SEPARATOR) * shown.len().saturating_sub(1)
}

/// Drops the hints that go first until the rest fit in `room`; of two that go as soon, the later one
fn fit(groups: &mut [Vec<Hint>], room: usize) {
    loop {
        let line: Vec<&[Hint]> = groups.iter().map(Vec::as_slice).collect();
        if line_width(&line) <= room {
            return;
        }
        let first = groups
            .iter()
            .enumerate()
            .flat_map(|(g, group)| {
                group
                    .iter()
                    .enumerate()
                    .map(move |(h, hint)| (hint.keep, g, h))
            })
            // The lowest keep; of two as low, the later one
            .max_by_key(|&(keep, g, h)| (Reverse(keep), g, h));
        let Some((_, g, h)) = first else {
            return;
        };
        groups[g].remove(h);
    }
}

/// The keys that work now, in groups, and whether help and quit are pinned on the right
fn hints(app: &mut App) -> (Vec<Vec<Hint>>, bool) {
    if app.filtering {
        return (vec![filter_hints()], false);
    }
    if let Some(logs) = &app.logs {
        return (vec![logs_hints(logs)], false);
    }
    match app.view {
        View::Containers => (container_hints(app), true),
        View::Images | View::Volumes => {
            let remove = hint(names(&[keys::REMOVE]), "remove", 9);
            (vec![vec![remove], list_hints(app.view)], true)
        }
    }
}

fn filter_hints() -> Vec<Hint> {
    vec![
        hint(keys::ENTER, "keep", 3),
        hint(keys::ESC, "clear", 2),
        hint(keys::ARROWS, "move", 1),
    ]
}

fn logs_hints(logs: &Logs) -> Vec<Hint> {
    let pause = if logs.view.is_none() {
        "pause"
    } else {
        "follow"
    };
    vec![
        hint(keys::ESC, "back", 4),
        hint(keys::SPACE_NAME, pause, 3),
        hint(keys::ARROWS, "scroll", 2),
        hint(names(&[keys::TOP, keys::BOTTOM]), "top / end", 1),
    ]
}

/// What the list shows and how, with the view's sort keys
fn list_hints(view: View) -> Vec<Hint> {
    let sort: Vec<char> = view.sort_keys().iter().map(|key| key.key()).collect();
    vec![
        hint(names(&[keys::FILTER]), "filter", 8),
        hint(names(&[keys::DETAILS]), "details", 4),
        hint(names(&sort), "sort", 3),
        hint(keys::TAB, "view", 2),
        hint(names(&[keys::PRUNE]), "prune", 5),
    ]
}

/// What works for the selected container, then the list's keys with showing all or only the running ones
fn container_hints(app: &mut App) -> Vec<Vec<Hint>> {
    let show = if app.show_all { "running" } else { "all" };
    let selected = app.selected_container();
    let stop = if selected.is_some_and(|container| !container.is_up()) {
        "start"
    } else {
        "stop"
    };
    let logs = !selected.is_some_and(Container::is_interactive);
    let shell = selected.is_some_and(Container::is_running);
    let selection = vec![
        hint(names(&[keys::STOP]), stop, 10),
        hint(names(&[keys::RESTART]), "restart", 7),
        hint(names(&[keys::REMOVE]), "remove", 8),
        hint(names(&[keys::LOGS]), "logs", 10).enabled_if(logs),
        hint(names(&[keys::SHELL]), "shell", 9).enabled_if(shell),
    ];
    let mut list = list_hints(View::Containers);
    list.insert(1, hint(names(&[keys::SHOW_ALL]), show, 6));
    vec![selection, list]
}

#[cfg(test)]
mod tests;
