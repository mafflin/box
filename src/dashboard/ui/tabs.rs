//! The first line: a tab per view, then the sort, filter and connection

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;

use super::{bold, fg, put, put_right};
use crate::dashboard::app::{App, View};

pub fn draw(buf: &mut Buffer, area: Rect, app: &App) {
    let theme = app.theme;
    let counts = [app.containers.len(), app.images.len(), app.volumes.len()];
    let mut x = area.x + 1;
    for view in View::ALL {
        let label = format!(" {} {} ", view.title(), counts[view.index()]);
        let style = if view == app.view {
            bold(theme.on_color).bg(theme.accent)
        } else {
            fg(theme.muted).bg(theme.bg_bar)
        };
        x = put(buf, x, area.y, &label, style, area.right()) + 1;
    }

    // In the order shown, each with how much it matters when there isn't room for all
    let sort = app.sort[app.view.index()];
    let filter = &app.filter[app.view.index()];
    let arrow = if sort.descending { "▼" } else { "▲" };
    let mut groups: Vec<(u8, Vec<Span>)> = Vec::new();
    if app.disconnected.is_some() {
        groups.push((
            3,
            vec![Span::styled(
                " disconnected · retrying ",
                bold(theme.on_color).bg(theme.red),
            )],
        ));
    }
    groups.push((
        1,
        vec![
            Span::styled("sort ", fg(theme.dim)),
            Span::styled(format!("{} {arrow}", sort.key.label()), fg(theme.text)),
        ],
    ));
    if !filter.is_empty() || app.filtering {
        let shown = if filter.is_empty() { "…" } else { filter };
        groups.push((
            2,
            vec![
                Span::styled("filter ", fg(theme.dim)),
                Span::styled(shown.to_string(), bold(theme.accent)),
            ],
        ));
    }
    if app.view == View::Containers {
        groups.push((
            0,
            vec![
                Span::styled("showing ", fg(theme.dim)),
                Span::styled(if app.show_all { "all" } else { "running" }, fg(theme.text)),
            ],
        ));
    }

    let room = usize::from(area.right().saturating_sub(x));
    while !groups.is_empty() {
        let mut spans = Vec::new();
        for (i, (_, group)) in groups.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("   "));
            }
            spans.extend(group.iter().cloned());
        }
        spans.push(Span::raw(" "));
        if spans.iter().map(Span::width).sum::<usize>() < room {
            put_right(buf, area.right(), area.y, &spans);
            return;
        }
        let least = groups
            .iter()
            .enumerate()
            .min_by_key(|(_, (priority, _))| *priority)
            .map_or(0, |(i, _)| i);
        groups.remove(least);
    }
}
