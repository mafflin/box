//! Drawing the dashboard: tabs, a table, its details or logs, the footer, and dialogs over them

mod detail;
mod footer;
mod logs;
mod modal;
mod table;
mod tabs;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::app::{App, Details};
use crate::theme::Theme;

const MIN_WIDTH: u16 = 30;
const MIN_HEIGHT: u16 = 6;
/// A shorter body shows no details at the bottom
const MIN_BODY_HEIGHT_FOR_BOTTOM: u16 = 14;
/// The details at the bottom usually take this share of the body, up to `BOTTOM_DETAILS_ROWS`, and more
/// when their lines need it
const BOTTOM_DETAILS_PERCENT: u16 = 42;
const BOTTOM_DETAILS_ROWS: u16 = 12;
/// A narrower body shows no details on the right
const MIN_BODY_WIDTH_FOR_RIGHT: u16 = 90;
/// The details on the right take this share of the body, kept between the two widths below
const RIGHT_DETAILS_PERCENT: u16 = 34;
const MIN_RIGHT_DETAILS_WIDTH: u16 = 36;
const MAX_RIGHT_DETAILS_WIDTH: u16 = 64;
/// Before the first thing on the tabs line and the footer, and after the last
const BAR_MARGIN: u16 = 1;
/// Shown for what has no value
const NO_VALUE: &str = "—";

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = app.theme;
    let buf = frame.buffer_mut();
    fill(buf, area, theme.text, Color::Reset);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        put(
            buf,
            area.x,
            area.y,
            "box needs a bigger terminal",
            fg(theme.muted),
            area.right(),
        );
        return;
    }

    tabs::draw(buf, Rect::new(area.x, area.y, area.width, 1), app);
    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 2);
    if app.logs.is_some() {
        logs::draw(buf, body, app);
    } else {
        draw_body(buf, body, app);
    }
    footer::draw(
        buf,
        Rect::new(area.x, area.bottom() - 1, area.width, 1),
        app,
    );

    if app.modal.is_some() {
        dim(buf, area, &theme);
        modal::draw(buf, area, app);
    }
}

/// The table, and its details below or beside it when there's room
fn draw_body(buf: &mut Buffer, area: Rect, app: &mut App) {
    match app.details {
        Details::Bottom if area.height >= MIN_BODY_HEIGHT_FOR_BOTTOM => {
            // As tall as they need, never shorter than usual, at most half
            let usual = (area.height * BOTTOM_DETAILS_PERCENT / 100).min(BOTTOM_DETAILS_ROWS);
            let height = detail::height(app).clamp(usual, area.height / 2);
            let table = Rect {
                height: area.height - height,
                ..area
            };
            table::draw(buf, table, app);
            detail::draw(
                buf,
                Rect::new(area.x, table.bottom(), area.width, height),
                app,
                true,
            );
        }
        Details::Right if area.width >= MIN_BODY_WIDTH_FOR_RIGHT => {
            let width = (area.width * RIGHT_DETAILS_PERCENT / 100)
                .clamp(MIN_RIGHT_DETAILS_WIDTH, MAX_RIGHT_DETAILS_WIDTH);
            let table = Rect {
                width: area.width - width,
                ..area
            };
            table::draw(buf, table, app);
            detail::draw(
                buf,
                Rect::new(table.right(), area.y, width, area.height),
                app,
                false,
            );
        }
        _ => table::draw(buf, area, app),
    }
}

pub fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

pub fn bold(color: Color) -> Style {
    Style::new().fg(color).add_modifier(Modifier::BOLD)
}

/// Bold text on `color`, reversed so the text is the terminal's background, readable on any color, even
/// the terminal's own text color
pub fn badge(color: Color) -> Style {
    Style::new()
        .fg(color)
        .bg(Color::Reset)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
}

/// Writes `text` up to `end`, the column after where it's written
fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, end: u16) -> u16 {
    if x >= end || y >= buf.area.bottom() {
        return x;
    }
    buf.set_stringn(x, y, text, usize::from(end - x), style).0
}

fn put_line(buf: &mut Buffer, x: u16, y: u16, line: &Line, end: u16) -> u16 {
    if x >= end || y >= buf.area.bottom() {
        return x;
    }
    buf.set_line(x, y, line, end - x).0
}

/// Spans that end at `end`
fn put_right(buf: &mut Buffer, end: u16, y: u16, spans: &[Span]) {
    let width = size(spans.iter().map(Span::width).sum());
    let x = end.saturating_sub(width).max(buf.area.x);
    put_line(buf, x, y, &Line::from(spans.to_vec()), end);
}

/// `text` at the right of the border's row `y`, a cell before the corner
fn put_on_border(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style) {
    let text_width = size(width(text));
    let x = area.right().saturating_sub(text_width.saturating_add(2));
    put(buf, x, y, text, style, area.right().saturating_sub(1));
}

/// Its inside without a column on each side
fn padded(inner: Rect) -> Rect {
    Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    }
}

/// Blanks the area in these colors
fn fill(buf: &mut Buffer, area: Rect, fg: Color, bg: Color) {
    for position in area.intersection(buf.area).positions() {
        let cell = &mut buf[position];
        cell.reset();
        cell.set_fg(fg).set_bg(bg);
    }
}

/// A rounded border with a title, the area inside it
fn rounded_box(
    buf: &mut Buffer,
    area: Rect,
    border: Color,
    title: &str,
    title_style: Style,
) -> Rect {
    let area = area.intersection(buf.area);
    if area.width < 2 || area.height < 2 {
        return Rect::default();
    }
    let (left, right, top, bottom) = (area.left(), area.right() - 1, area.top(), area.bottom() - 1);
    let style = fg(border);
    for x in left + 1..right {
        buf[(x, top)].set_symbol("─").set_style(style);
        buf[(x, bottom)].set_symbol("─").set_style(style);
    }
    for y in top + 1..bottom {
        buf[(left, y)].set_symbol("│").set_style(style);
        buf[(right, y)].set_symbol("│").set_style(style);
    }
    for (x, y, corner) in [
        (left, top, "╭"),
        (right, top, "╮"),
        (left, bottom, "╰"),
        (right, bottom, "╯"),
    ] {
        buf[(x, y)].set_symbol(corner).set_style(style);
    }
    if !title.is_empty() {
        put(
            buf,
            left + 2,
            top,
            &format!(" {title} "),
            title_style,
            right.saturating_sub(1),
        );
    }
    Rect::new(left + 1, top + 1, area.width - 2, area.height - 2)
}

/// Fades what's behind a dialog
fn dim(buf: &mut Buffer, area: Rect, theme: &Theme) {
    for position in area.intersection(buf.area).positions() {
        let cell = &mut buf[position];
        let colored = cell.bg != Color::Reset || cell.modifier.contains(Modifier::REVERSED);
        cell.fg = theme.dim;
        cell.modifier = Modifier::empty();
        if colored {
            cell.bg = theme.stripe;
        }
    }
}

fn width(text: &str) -> usize {
    Span::raw(text).width()
}

/// Padded or cut with `…` to `width` columns
fn fit(text: &str, columns: usize, right_aligned: bool) -> String {
    let (shown, shown_width) = if width(text) <= columns {
        (text.to_string(), width(text))
    } else if columns == 0 {
        return String::new();
    } else {
        let (head, used) = head(text, columns - 1);
        (format!("{head}…"), used + 1)
    };
    let padding = " ".repeat(columns - shown_width);
    if right_aligned {
        padding + &shown
    } else {
        shown + &padding
    }
}

/// `n` as a terminal size, the largest one when it's more
fn size(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// As much of `text`'s start as fits in `room` columns, and how many of them it takes; by what the terminal
/// draws as one character, so an emoji made of several, like `❤️`, is kept or left whole
fn head(text: &str, room: usize) -> (String, usize) {
    let mut kept = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let grapheme_width = grapheme.width();
        if used + grapheme_width > room {
            break;
        }
        kept.push_str(grapheme);
        used += grapheme_width;
    }
    (kept, used)
}

#[cfg(test)]
mod tests;
