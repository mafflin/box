//! A container's logs, following new lines until paused

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::{badge, bold, fg, put, put_line, rounded_box};
use crate::dashboard::app::{App, Level};

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;
    let Some(logs) = &app.logs else { return };
    let inner = rounded_box(
        buf,
        area,
        theme.border,
        &format!("logs · {}", logs.name),
        fg(theme.text),
    );
    let (state, color) = if logs.ended {
        (" ■ ended ", theme.dim)
    } else if logs.view.is_none() {
        (" ● following ", theme.green)
    } else {
        (" ‖ paused ", theme.yellow)
    };
    let width = u16::try_from(state.chars().count()).unwrap_or(0);
    put(
        buf,
        area.right().saturating_sub(2 + width),
        area.y,
        state,
        bold(color),
        area.right() - 1,
    );

    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    let height = usize::from(inner.height);
    let length = logs.lines.len();
    let end = logs.view.unwrap_or(length).min(length);
    let start = end.saturating_sub(height);

    if length == 0 {
        let message = if logs.ended {
            "no output"
        } else {
            "waiting for output…"
        };
        put(buf, inner.x, inner.y, message, fg(theme.dim), inner.right());
    }
    for (i, line) in logs.lines.range(start..end).enumerate() {
        let y = inner.y + u16::try_from(i).unwrap_or(0);
        let color = match line.level {
            Level::Error => theme.red,
            Level::Warning => theme.yellow,
            Level::Info => theme.text,
            Level::Debug => theme.dim,
        };
        let mut spans = Vec::new();
        if !line.time.is_empty() {
            spans.push(Span::styled(line.time.clone(), fg(theme.dim)));
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(line.text.clone(), fg(color)));
        put_line(buf, inner.x, y, &Line::from(spans), inner.right());
    }

    let newer = length - end;
    if logs.view.is_some() && newer > 0 {
        let label = format!(" ↓ {newer} newer ");
        let width = u16::try_from(label.chars().count()).unwrap_or(0);
        let x = area.right().saturating_sub(2 + width);
        put(
            buf,
            x,
            area.bottom() - 1,
            &label,
            badge(theme.yellow),
            area.right() - 1,
        );
    }
    app.log_page = height;
}
