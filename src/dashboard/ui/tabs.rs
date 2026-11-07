//! The first line: a tab per view, then the sort, filter and connection

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;

use super::{BAR_MARGIN, badge, bold, fg, put, put_right, sort_arrow};
use crate::dashboard::app::{App, View};
use crate::format::ELLIPSIS;

/// Between two tabs
const TAB_GAP: u16 = 1;
/// Between two things on the right
const GROUP_GAP: &str = "   ";

/// Something on the right, and how long it stays when there isn't room for all: the lowest `keep` goes first
struct Group {
    keep: u8,
    spans: Vec<Span<'static>>,
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &App) {
    let x = draw_tabs(buf, area, app);
    draw_status(buf, area, x, status_groups(app));
}

/// ` Containers 8 ` for each view, the current one highlighted; where they end
fn draw_tabs(buf: &mut Buffer, area: Rect, app: &App) -> u16 {
    let theme = app.theme;
    let counts = [app.containers.len(), app.images.len(), app.volumes.len()];
    let mut x = area.x + BAR_MARGIN;
    for view in View::ALL {
        let label = format!(" {} {} ", view.title(), counts[view.index()]);
        let style = if view == app.view {
            badge(theme.accent)
        } else {
            fg(theme.muted).bg(theme.bar)
        };
        x = put(buf, x, area.y, &label, style, area.right()) + TAB_GAP;
    }
    x
}

/// The connection, sort, filter and what's shown, in the order shown
fn status_groups(app: &App) -> Vec<Group> {
    let theme = app.theme;
    let sort = app.sort[app.view.index()];
    let filter = &app.filter[app.view.index()];
    let arrow = sort_arrow(sort);
    let mut groups = Vec::new();
    if app.disconnected.is_some() {
        groups.push(Group {
            keep: 3,
            spans: vec![Span::styled(" disconnected · retrying ", badge(theme.red))],
        });
    }
    groups.push(Group {
        keep: 1,
        spans: vec![
            Span::styled("sort ", fg(theme.dim)),
            Span::styled(format!("{} {arrow}", sort.key.label()), fg(theme.text)),
        ],
    });
    if !filter.is_empty() || app.filtering {
        // Until something's typed
        let shown = if filter.is_empty() {
            ELLIPSIS.to_string()
        } else {
            filter.clone()
        };
        groups.push(Group {
            keep: 2,
            spans: vec![
                Span::styled("filter ", fg(theme.dim)),
                Span::styled(shown, bold(theme.accent)),
            ],
        });
    }
    if app.view == View::Containers {
        groups.push(Group {
            keep: 0,
            spans: vec![
                Span::styled("showing ", fg(theme.dim)),
                Span::styled(if app.show_all { "all" } else { "running" }, fg(theme.text)),
            ],
        });
    }
    groups
}

/// The groups that fit after `x`, at the right; of two as low, the first goes first
fn draw_status(buf: &mut Buffer, area: Rect, x: u16, mut groups: Vec<Group>) {
    let room = usize::from(area.right().saturating_sub(x));
    while !groups.is_empty() {
        let line = status_line(&groups);
        let width = line.iter().map(Span::width).sum::<usize>() + usize::from(BAR_MARGIN);
        if width < room {
            put_right(buf, area.right().saturating_sub(BAR_MARGIN), area.y, &line);
            return;
        }
        let first = groups
            .iter()
            .enumerate()
            .min_by_key(|(_, group)| group.keep)
            .map_or(0, |(i, _)| i);
        groups.remove(first);
    }
}

/// The groups' spans, a gap between two
fn status_line(groups: &[Group]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(GROUP_GAP));
        }
        spans.extend(group.spans.iter().cloned());
    }
    spans
}
