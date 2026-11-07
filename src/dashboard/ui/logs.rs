//! A container's logs, following new lines until paused

use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};

use super::{badge, bold, fg, padded, put, put_line, put_on_border, rounded_box};
use crate::dashboard::app::{App, Level, LogLine, Logs};
use crate::theme::Theme;

/// Between a line's time and its text
const TIME_GAP: &str = "  ";

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;
    let Some(logs) = &app.logs else { return };
    let title = format!("logs · {}", logs.name);
    let inner = padded(rounded_box(buf, area, theme.border, &title, fg(theme.text)));
    let (state, color) = state(logs, &theme);
    put_on_border(buf, area, area.y, state, bold(color));

    let height = usize::from(inner.height);
    let shown = shown(logs, height);
    if logs.lines.is_empty() {
        let message = if logs.ended {
            "no output"
        } else {
            "waiting for output…"
        };
        put(buf, inner.x, inner.y, message, fg(theme.dim), inner.right());
    }
    for (line, y) in logs.lines.range(shown.clone()).zip(inner.y..) {
        put_line(buf, inner.x, y, &log_line(line, &theme), inner.right());
    }
    let newer = logs.lines.len() - shown.end;
    if logs.view.is_some() && newer > 0 {
        let label = format!(" ↓ {newer} newer ");
        put_on_border(buf, area, area.bottom() - 1, &label, badge(theme.yellow));
    }
    app.log_page = height;
}

/// ` ● following `, ` ‖ paused ` or ` ■ ended `, in its color
fn state(logs: &Logs, theme: &Theme) -> (&'static str, Color) {
    if logs.ended {
        (" ■ ended ", theme.dim)
    } else if logs.view.is_none() {
        (" ● following ", theme.green)
    } else {
        (" ‖ paused ", theme.yellow)
    }
}

/// The lines that fit in `height`, ending at the newest or where it's scrolled to
fn shown(logs: &Logs, height: usize) -> Range<usize> {
    let length = logs.lines.len();
    let end = logs.view.unwrap_or(length).min(length);
    end.saturating_sub(height)..end
}

/// Its time, dim, then its text in its level's color
fn log_line(line: &LogLine, theme: &Theme) -> Line<'static> {
    let color = match line.level {
        Level::Error => theme.red,
        Level::Warning => theme.yellow,
        Level::Info => theme.text,
        Level::Debug => theme.dim,
    };
    let mut spans = Vec::new();
    if !line.time.is_empty() {
        spans.push(Span::styled(line.time.clone(), fg(theme.dim)));
        spans.push(Span::raw(TIME_GAP));
    }
    spans.push(Span::styled(line.text.clone(), fg(color)));
    Line::from(spans)
}
