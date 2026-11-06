//! The last line: the keys that work now, or the filter being typed; help and quit or a message on the right

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;

use super::{bold, fg, fill, put, put_right, width};
use crate::dashboard::app::{App, Tone, View};
use crate::theme::Theme;

/// Between the selection's keys and the list's
const SEPARATOR: &str = "  │  ";
/// Between two keys
const GAP: usize = 2;

/// A key, what it does, whether it does it for the selection, and how long it stays when the line is too
/// short: the lowest goes first
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
    fn width(&self) -> usize {
        width(self.key) + 1 + width(self.description)
    }
}

/// Always on the right, unless a message is
const PINNED: [Hint; 2] = [hint("?", "help", u8::MAX), hint("q", "quit", u8::MAX)];

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;
    fill(buf, area, theme.muted, theme.bar);

    let toast = app.toast.as_ref().map(|toast| {
        let color = match toast.tone {
            Tone::Info => theme.text,
            Tone::Good => theme.green,
            Tone::Warn => theme.yellow,
            Tone::Bad => theme.red,
        };
        (format!("{} ", toast.text), color)
    });
    let (mut groups, pinned) = hints(app);
    let right = match (&toast, pinned) {
        (Some((text, _)), _) => width(text),
        (None, true) => line_width(&[&PINNED[..]]) + 1,
        (None, false) => 0,
    };

    let mut x = area.x + 1;
    if app.filtering {
        let filter = &app.filter[app.view.index()];
        x = put(buf, x, area.y, "filter ", fg(theme.muted), area.right());
        x = put(buf, x, area.y, filter, bold(theme.text), area.right());
        x = put(buf, x, area.y, "▏", bold(theme.accent), area.right()) + 3;
    }
    let room = usize::from(area.right().saturating_sub(x)).saturating_sub(right + GAP);
    fit(&mut groups, room);

    let groups: Vec<&[Hint]> = groups.iter().map(Vec::as_slice).collect();
    x = draw_hints(buf, x, area, &groups, &theme);
    if let Some((text, color)) = toast {
        put_right(
            buf,
            area.right(),
            area.y,
            &[Span::styled(text, bold(color))],
        );
    } else if pinned {
        let start = area
            .right()
            .saturating_sub(u16::try_from(right).unwrap_or(0));
        draw_hints(buf, start.max(x), area, &[&PINNED[..]], &theme);
    }
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
                x += u16::try_from(GAP).unwrap_or(0);
            }
            let (key_style, description_style) = if hint.enabled {
                (bold(theme.accent), fg(theme.muted))
            } else {
                (fg(theme.dim), fg(theme.dim))
            };
            x = put(buf, x, area.y, hint.key, key_style, area.right());
            x = put(
                buf,
                x + 1,
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
        .map(|group| group.iter().map(Hint::width).sum::<usize>() + GAP * (group.len() - 1))
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
            .min_by(|a, b| a.0.cmp(&b.0).then((b.1, b.2).cmp(&(a.1, a.2))));
        let Some((_, g, h)) = first else {
            return;
        };
        groups[g].remove(h);
    }
}

/// The keys that work now, in groups, and whether help and quit are pinned on the right
fn hints(app: &mut App) -> (Vec<Vec<Hint>>, bool) {
    if app.filtering {
        let keys = vec![
            hint("enter", "keep", 3),
            hint("esc", "clear", 2),
            hint("↑↓", "move", 1),
        ];
        return (vec![keys], false);
    }
    if let Some(logs) = &app.logs {
        let pause = if logs.view.is_none() {
            "pause"
        } else {
            "follow"
        };
        let keys = vec![
            hint("esc", "back", 4),
            hint("space", pause, 3),
            hint("↑↓", "scroll", 2),
            hint("g G", "top / end", 1),
        ];
        return (vec![keys], false);
    }
    // What the list shows and how
    let list = |sort: &'static str| {
        vec![
            hint("/", "filter", 8),
            hint("i", "details", 4),
            hint(sort, "sort", 3),
            hint("tab", "view", 2),
            hint("p", "prune", 5),
        ]
    };
    if app.view != View::Containers {
        let sort = if app.view == View::Images {
            "N S T"
        } else {
            "N T"
        };
        return (vec![vec![hint("d", "remove", 9)], list(sort)], true);
    }
    let show = if app.show_all { "running" } else { "all" };
    let selected = app.selected_container();
    let stop = if selected.is_some_and(|container| !container.is_up()) {
        "start"
    } else {
        "stop"
    };
    let logs = !selected.is_some_and(crate::docker::Container::is_interactive);
    let shell = selected.is_some_and(crate::docker::Container::is_running);
    let selection = vec![
        hint("s", stop, 10),
        hint("r", "restart", 7),
        hint("d", "remove", 8),
        Hint {
            enabled: logs,
            ..hint("l", "logs", 10)
        },
        Hint {
            enabled: shell,
            ..hint("e", "shell", 9)
        },
    ];
    let mut list = list("P M N T");
    list.insert(1, hint("a", show, 6));
    (vec![selection, list], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(groups: &[Vec<Hint>]) -> Vec<Vec<&str>> {
        groups
            .iter()
            .map(|group| group.iter().map(|hint| hint.key).collect())
            .collect()
    }

    #[test]
    fn drops_the_hints_that_go_first() {
        // `s stop  r restart  │  / filter  i details`, 41 wide
        let mut groups = vec![
            vec![hint("s", "stop", 3), hint("r", "restart", 1)],
            vec![hint("/", "filter", 2), hint("i", "details", 1)],
        ];
        fit(&mut groups, 41);
        assert_eq!(keys(&groups), [vec!["s", "r"], vec!["/", "i"]]);
        // Of two that go as soon, the later one
        fit(&mut groups, 30);
        assert_eq!(keys(&groups), [vec!["s", "r"], vec!["/"]]);
        fit(&mut groups, 20);
        assert_eq!(keys(&groups), [vec!["s"], vec!["/"]]);
        // No separator for an empty group
        fit(&mut groups, 6);
        assert_eq!(keys(&groups), [vec!["s"], vec![]]);
        fit(&mut groups, 0);
        assert_eq!(keys(&groups), [Vec::<&str>::new(), vec![]]);
    }
}
