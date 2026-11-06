//! Dialogs over the dashboard: help, confirming a change, and prune

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};

use super::{bold, fg, fill, fit, put, put_line, rounded_box};
use crate::dashboard::app::{App, Confirm, Modal, PruneDialog};
use crate::dashboard::theme::Theme;
use crate::format::human_size;

const HELP: [(&str, &[(&str, &str)]); 3] = [
    (
        "General",
        &[
            ("↑↓ j k", "move"),
            ("g G", "top / bottom"),
            ("tab 1 2 3", "switch view"),
            ("/", "filter"),
            ("esc", "clear filter"),
            ("P M N", "sort cpu / mem / name"),
            ("S T", "sort size / created"),
            ("i", "details bottom/right/off"),
            ("p", "prune"),
            ("q", "quit"),
        ],
    ),
    (
        "Containers",
        &[
            ("s", "stop / start"),
            ("r", "restart"),
            ("d", "remove"),
            ("l enter", "logs (not for -it)"),
            ("e", "shell (bash or sh)"),
            ("a", "show stopped"),
        ],
    ),
    ("Images · Volumes", &[("d", "remove if unused")]),
];

/// In `PruneTarget::ORDER`
const PRUNE_LABELS: [&str; 3] = ["stopped containers", "dangling volumes", "dangling images"];
const PRUNE_ORDER: &str = "runs in order: containers → volumes → images";

pub fn draw(buf: &mut Buffer, area: Rect, app: &App) {
    match &app.modal {
        None => {}
        Some(Modal::Help) => help(buf, area, &app.theme),
        Some(Modal::Confirm(confirm)) => confirmation(buf, area, &app.theme, confirm),
        Some(Modal::Prune(state)) => prune(buf, area, app, state),
    }
}

fn help(buf: &mut Buffer, area: Rect, theme: &Theme) {
    let inner = dialog(buf, area, 76, 19, "Keys", theme.accent, theme.bg_bar);
    let mut column = |x: u16, sections: &[(&str, &[(&str, &str)])]| {
        let mut y = inner.y + 1;
        for (title, keys) in sections {
            put(buf, x, y, title, bold(theme.accent), inner.right());
            y += 1;
            for (key, description) in *keys {
                put(buf, x, y, key, bold(theme.text), inner.right());
                put(buf, x + 12, y, description, fg(theme.text), inner.right());
                y += 1;
            }
            y += 1;
        }
    };
    column(inner.x + 1, &HELP[..1]);
    column(inner.x + 40, &HELP[1..]);
    let y = inner.bottom().saturating_sub(1);
    put(
        buf,
        inner.x + 1,
        y,
        "any key closes",
        fg(theme.dim),
        inner.right(),
    );
}

fn confirmation(buf: &mut Buffer, area: Rect, theme: &Theme, confirm: &Confirm) {
    let color = if confirm.danger {
        theme.red
    } else {
        theme.accent
    };
    let width = confirm
        .question
        .chars()
        .count()
        .max(confirm.command.chars().count())
        + 8;
    let width = u16::try_from(width.max(44)).unwrap_or(u16::MAX);
    let inner = dialog(buf, area, width, 7, &confirm.title, color, theme.bg_bar);
    put(
        buf,
        inner.x + 1,
        inner.y + 1,
        &confirm.question,
        bold(theme.text),
        inner.right(),
    );
    put(
        buf,
        inner.x + 1,
        inner.y + 2,
        &confirm.command,
        fg(theme.muted),
        inner.right(),
    );
    let buttons = Line::from(vec![
        Span::styled(" y ", bold(theme.on_color).bg(color)),
        Span::styled(" confirm    ", fg(theme.text)),
        Span::styled(" n ", bold(theme.text).bg(theme.bg_selected)),
        Span::styled(" cancel", fg(theme.text)),
    ]);
    put_line(
        buf,
        inner.x + 1,
        inner.bottom().saturating_sub(1),
        &buttons,
        inner.right(),
    );
}

fn prune(buf: &mut Buffer, area: Rect, app: &App, state: &PruneDialog) {
    let theme = &app.theme;
    let inner = dialog(buf, area, 56, 11, "Prune", theme.accent, theme.bg_bar);
    let choices = PRUNE_LABELS.iter().zip(app.prunable()).zip(state.chosen);
    for (i, ((label, (count, size)), chosen)) in choices.enumerate() {
        let y = inner.y + 1 + u16::try_from(i).unwrap_or(0);
        let current = i == state.cursor;
        if current {
            fill(
                buf,
                Rect::new(inner.x, y, inner.width, 1),
                theme.text,
                theme.bg_selected,
            );
        }
        let size = if size > 0 {
            format!("  {}", human_size(size))
        } else {
            String::new()
        };
        let line = Line::from(vec![
            Span::styled(if current { "› " } else { "  " }, bold(theme.accent)),
            Span::styled(
                if chosen { "[x] " } else { "[ ] " },
                bold(if chosen { theme.accent } else { theme.muted }),
            ),
            Span::styled(fit(label, 20, false), fg(theme.text)),
            Span::styled(
                fit(&count.to_string(), 4, true),
                bold(if count > 0 { theme.text } else { theme.dim }),
            ),
            Span::styled(size, fg(theme.text)),
        ]);
        put_line(buf, inner.x, y, &line, inner.right());
    }
    put(
        buf,
        inner.x + 1,
        inner.y + 5,
        PRUNE_ORDER,
        fg(theme.dim),
        inner.right(),
    );
    let keys = Line::from(vec![
        Span::styled("↑↓", bold(theme.accent)),
        Span::styled(" move  ", fg(theme.muted)),
        Span::styled("space", bold(theme.accent)),
        Span::styled(" toggle  ", fg(theme.muted)),
        Span::styled("enter", bold(theme.accent)),
        Span::styled(" run  ", fg(theme.muted)),
        Span::styled("esc", bold(theme.accent)),
        Span::styled(" cancel", fg(theme.muted)),
    ]);
    put_line(
        buf,
        inner.x + 1,
        inner.bottom().saturating_sub(1),
        &keys,
        inner.right(),
    );
}

/// A box in the middle, the area inside its padding
fn dialog(
    buf: &mut Buffer,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    color: Color,
    background: Color,
) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let outer = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    fill(buf, outer, color, background);
    let inner = rounded_box(buf, outer, color, title, bold(color));
    Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    }
}
