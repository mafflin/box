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

use super::app::{App, Details};
use super::theme::Theme;

const MIN_WIDTH: u16 = 30;
const MIN_HEIGHT: u16 = 6;

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
        Details::Bottom if area.height >= 14 => {
            let height = (area.height * 42 / 100).min(12);
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
        Details::Right if area.width >= 90 => {
            let width = (area.width * 34 / 100).clamp(36, 64);
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
    let width = u16::try_from(spans.iter().map(Span::width).sum::<usize>()).unwrap_or(u16::MAX);
    let x = end.saturating_sub(width).max(buf.area.x);
    put_line(buf, x, y, &Line::from(spans.to_vec()), end);
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
        cell.fg = theme.dim;
        cell.modifier = Modifier::empty();
        if cell.bg != Color::Reset {
            cell.bg = theme.bg_alt;
        }
    }
}

fn width(text: &str) -> usize {
    Span::raw(text).width()
}

/// Padded or cut with `…` to `width` columns
fn fit(text: &str, width: usize, right_aligned: bool) -> String {
    let length = text.chars().count();
    if length > width {
        if width == 0 {
            return String::new();
        }
        let cut: String = text.chars().take(width - 1).collect();
        return format!("{cut}…");
    }
    let padding = " ".repeat(width - length);
    if right_aligned {
        padding + text
    } else {
        text.to_string() + &padding
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    use super::*;
    use crate::colors;
    use crate::dashboard::app::{Action, LogLine};
    use crate::dashboard::sample;

    /// Named after the screen alone, as the module path would be `r#box::...`
    fn assert_snapshot(name: &str, screen: &str) {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        settings.bind(|| insta::assert_snapshot!(name, screen));
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let line: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
                line.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            let code = match c {
                '⏎' => KeyCode::Enter,
                c => KeyCode::Char(c),
            };
            app.update(Action::Key(KeyEvent::from(code)), Instant::now());
        }
    }

    #[test]
    fn containers() {
        let mut app = sample::app();
        assert_snapshot("containers_140x40", &screen(&mut app, 140, 40));
        assert_snapshot("containers_80x24", &screen(&mut app, 80, 24));
        press(&mut app, "ji");
        assert_snapshot("containers_right_details", &screen(&mut app, 140, 30));
    }

    #[test]
    fn styles() {
        let mut app = sample::app();
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let theme = app.theme;
        // The active tab, the selected row, a zebra row and the stopped container
        assert_eq!(buffer[(2, 0)].bg, theme.accent);
        assert_eq!(buffer[(3, 3)].bg, theme.bg_selected);
        assert_eq!(buffer[(3, 4)].bg, theme.bg_alt);
        assert!(!buffer[(3, 3)].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(3, 7)].fg, theme.dim);

        // How many containers use a volume, dimmed for none
        press(&mut app, "3");
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let count = |name: &str| {
            let area = buffer.area;
            let y = (0..area.height)
                .find(|&y| {
                    let line: String = (0..area.width).map(|x| buffer[(x, y)].symbol()).collect();
                    line.contains(name)
                })
                .unwrap();
            let x = (0..area.width)
                .rev()
                .find(|&x| buffer[(x, y)].symbol().chars().all(|c| c.is_ascii_digit()))
                .unwrap();
            buffer[(x, y)].fg
        };
        assert_eq!(count("pgdata"), colors::COUNT);
        assert_eq!(count("old-data"), theme.dim);

        // An image docker's host can't run natively
        press(&mut app, "2");
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let screen: Vec<String> = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect();
        let y = screen
            .iter()
            .position(|line| line.contains("postgres:16"))
            .unwrap();
        let x = screen[y]
            .chars()
            .take(screen[y].find("linux/amd64").unwrap())
            .count();
        let cell = &buffer[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())];
        assert_eq!(cell.fg, theme.yellow);
    }

    #[test]
    fn images_and_volumes() {
        let mut app = sample::app();
        press(&mut app, "2");
        assert_snapshot("images", &screen(&mut app, 120, 24));
        press(&mut app, "3");
        assert_snapshot("volumes", &screen(&mut app, 120, 24));
        press(&mut app, "2ji");
        assert_snapshot("image_details", &screen(&mut app, 140, 30));
        press(&mut app, "jjj");
        assert!(screen(&mut app, 140, 30).contains("platform linux/amd64 · host is linux/arm64"));
        // Only while a shown image is foreign
        assert!(screen(&mut app, 140, 30).contains("PLATFORM"));
        press(&mut app, "/nginx⏎");
        assert!(!screen(&mut app, 140, 30).contains("PLATFORM"));
    }

    #[test]
    fn dialogs() {
        let mut app = sample::app();
        press(&mut app, "?");
        assert_snapshot("help", &screen(&mut app, 100, 30));
        press(&mut app, " s");
        assert_snapshot("confirm", &screen(&mut app, 100, 30));
        press(&mut app, "np");
        assert_snapshot("prune", &screen(&mut app, 100, 30));
    }

    #[test]
    fn logs() {
        let mut app = sample::app();
        press(&mut app, "l");
        for (i, text) in [
            "GET / 200",
            "WARN slow query",
            "ERROR timeout",
            "\u{1b}[2Jcleared",
        ]
        .iter()
        .enumerate()
        {
            let line = LogLine::parse(format!("2026-11-06T13:08:0{i}Z {text}").as_bytes());
            app.update(Action::Log { session: 1, line }, Instant::now());
        }
        assert_snapshot("logs", &screen(&mut app, 80, 12));
    }

    #[test]
    fn empty_and_filtered() {
        let mut app = App::new(Theme::new(crate::settings::Accent::Yellow), Instant::now());
        assert_snapshot("loading", &screen(&mut app, 80, 12));
        let mut app = sample::app();
        press(&mut app, "/nothing⏎");
        assert_snapshot("no_match", &screen(&mut app, 80, 20));
    }

    #[test]
    fn too_small() {
        let mut app = sample::app();
        assert_eq!(
            screen(&mut app, 20, 4).lines().next().unwrap(),
            "box needs a bigger t"
        );
    }

    #[test]
    fn fits_text() {
        assert_eq!(fit("abc", 5, false), "abc  ");
        assert_eq!(fit("abc", 5, true), "  abc");
        assert_eq!(fit("abcdef", 4, false), "abc…");
        assert_eq!(fit("abc", 0, false), "");
    }
}
