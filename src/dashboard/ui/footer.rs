//! The last line: the keys that work now, or the filter being typed; help and quit or a message on the right

use std::cmp::Reverse;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::Span;

use super::{BAR_MARGIN, bold, fg, fill, put, put_right, size, width};
use crate::dashboard::app::{App, Logs, Tone, View};
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
    key: &'static str,
    description: &'static str,
    enabled: bool,
    keep: u8,
}

const fn hint(key: &'static str, description: &'static str, keep: u8) -> Hint {
    Hint {
        key,
        description,
        enabled: true,
        keep,
    }
}

impl Hint {
    /// Dimmed when it doesn't work for the selection
    const fn enabled_if(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    fn width(&self) -> usize {
        width(self.key) + usize::from(KEY_GAP) + width(self.description)
    }
}

/// Always on the right, unless a message is
const PINNED: [Hint; 2] = [hint("?", "help", u8::MAX), hint("q", "quit", u8::MAX)];

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;
    fill(buf, area, theme.muted, theme.bar);

    let toast = toast(app);
    let (mut groups, pinned) = hints(app);
    let right = match (&toast, pinned) {
        (Some((text, _)), _) => width(text) + usize::from(BAR_MARGIN),
        (None, true) => line_width(&[&PINNED[..]]) + usize::from(BAR_MARGIN),
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
        draw_hints(buf, start.max(x), area, &[&PINNED[..]], &theme);
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
            x = put(buf, x, area.y, hint.key, key_style, area.right());
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
        View::Images => (
            vec![vec![hint("d", "remove", 9)], list_hints("N S T")],
            true,
        ),
        View::Volumes => (vec![vec![hint("d", "remove", 9)], list_hints("N T")], true),
    }
}

fn filter_hints() -> Vec<Hint> {
    vec![
        hint("enter", "keep", 3),
        hint("esc", "clear", 2),
        hint("↑↓", "move", 1),
    ]
}

fn logs_hints(logs: &Logs) -> Vec<Hint> {
    let pause = if logs.view.is_none() {
        "pause"
    } else {
        "follow"
    };
    vec![
        hint("esc", "back", 4),
        hint("space", pause, 3),
        hint("↑↓", "scroll", 2),
        hint("g G", "top / end", 1),
    ]
}

/// What the list shows and how, `sort` its sort keys
fn list_hints(sort: &'static str) -> Vec<Hint> {
    vec![
        hint("/", "filter", 8),
        hint("i", "details", 4),
        hint(sort, "sort", 3),
        hint("tab", "view", 2),
        hint("p", "prune", 5),
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
        hint("s", stop, 10),
        hint("r", "restart", 7),
        hint("d", "remove", 8),
        hint("l", "logs", 10).enabled_if(logs),
        hint("e", "shell", 9).enabled_if(shell),
    ];
    let mut list = list_hints("P M N T");
    list.insert(1, hint("a", show, 6));
    vec![selection, list]
}

#[cfg(test)]
mod tests;
