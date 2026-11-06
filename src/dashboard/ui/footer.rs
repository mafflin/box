//! The last line: the keys that work now, or the filter being typed, and a message on the right

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;

use super::{bold, fg, fill, put, put_right, width};
use crate::dashboard::app::{App, Tone, View};

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;
    fill(buf, area, theme.muted, theme.bg_bar);

    let toast = app.toast.as_ref().map(|toast| {
        let color = match toast.tone {
            Tone::Info => theme.text,
            Tone::Good => theme.green,
            Tone::Warn => theme.yellow,
            Tone::Bad => theme.red,
        };
        (format!("{} ", toast.text), color)
    });
    let toast_width = toast.as_ref().map_or(0, |(text, _)| width(text));
    let limit = usize::from(area.right()).saturating_sub(toast_width + 2);

    let mut x = area.x + 1;
    if app.filtering {
        let filter = &app.filter[app.view.index()];
        x = put(buf, x, area.y, "filter ", fg(theme.muted), area.right());
        x = put(buf, x, area.y, filter, bold(theme.text), area.right());
        x = put(buf, x, area.y, "▏", bold(theme.accent), area.right()) + 3;
    }
    let hints = hints(app);
    for (key, description, enabled) in hints {
        if usize::from(x) + width(key) + 1 + width(description) > limit {
            break;
        }
        let (key_style, description_style) = if enabled {
            (bold(theme.accent), fg(theme.muted))
        } else {
            (fg(theme.dim), fg(theme.dim))
        };
        x = put(buf, x, area.y, key, key_style, area.right());
        x = put(
            buf,
            x + 1,
            area.y,
            description,
            description_style,
            area.right(),
        ) + 2;
    }

    if let Some((text, color)) = toast {
        put_right(
            buf,
            area.right(),
            area.y,
            &[Span::styled(text, bold(color))],
        );
    }
}

/// The keys that work now, each with what it does and whether it does it for the selection
fn hints(app: &mut App) -> Vec<(&'static str, &'static str, bool)> {
    if app.filtering {
        return vec![
            ("enter", "keep", true),
            ("esc", "clear", true),
            ("↑↓", "move", true),
        ];
    }
    if let Some(logs) = &app.logs {
        let pause = if logs.view.is_none() {
            "pause"
        } else {
            "follow"
        };
        return vec![
            ("esc", "back", true),
            ("space", pause, true),
            ("↑↓", "scroll", true),
            ("g G", "top / end", true),
        ];
    }
    if app.view != View::Containers {
        let sort = if app.view == View::Images {
            "N S T"
        } else {
            "N T"
        };
        return vec![
            ("↑↓", "move", true),
            ("tab", "view", true),
            ("d", "remove", true),
            ("/", "filter", true),
            ("i", "details", true),
            (sort, "sort", true),
            ("p", "prune", true),
            ("?", "help", true),
            ("q", "quit", true),
        ];
    }
    let show = if app.show_all {
        "running only"
    } else {
        "show all"
    };
    let selected = app.selected_container();
    let stop = if selected.is_some_and(|container| !container.is_up()) {
        "start"
    } else {
        "stop"
    };
    let logs = !selected.is_some_and(crate::docker::Container::is_interactive);
    vec![
        ("↑↓", "move", true),
        ("tab", "view", true),
        ("s", stop, true),
        ("r", "restart", true),
        ("d", "remove", true),
        ("l", "logs", logs),
        ("/", "filter", true),
        ("a", show, true),
        ("i", "details", true),
        ("P M N", "sort", true),
        ("p", "prune", true),
        ("?", "help", true),
        ("q", "quit", true),
    ]
}
