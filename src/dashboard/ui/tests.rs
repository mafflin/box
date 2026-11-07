use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::*;
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
    // Too narrow for the long mount beside the graphs: its source is shortened
    assert_snapshot("containers_100x40", &screen(&mut app, 100, 40));
    press(&mut app, "ji");
    assert_snapshot("containers_right_details", &screen(&mut app, 140, 30));
}

#[test]
fn bottom_details_grow_to_fit() {
    let mut app = sample::app();
    // The usual share of 100x30 cuts api's details; all of them fit in half
    let shown = screen(&mut app, 100, 30);
    assert!(shown.contains("bundle:/usr/local/bundle"));
    assert!(!shown.contains(" more"));
    // Never more than half
    assert!(screen(&mut app, 80, 24).contains("… 2 more"));
}

#[test]
fn styles() {
    let mut app = sample::app();
    // A count color unlike plain text, so the check below can tell them apart
    app.theme.count = Color::Magenta;
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    let theme = app.theme;
    // The active tab: reversed, so its text is the terminal's background on the accent
    let tab = &buffer[(2, 0)];
    assert_eq!((tab.fg, tab.bg), (theme.accent, Color::Reset));
    assert!(tab.modifier.contains(Modifier::REVERSED));
    // The selected row, a zebra row and the stopped container
    assert_eq!(buffer[(3, 3)].bg, theme.selected);
    assert_eq!(buffer[(3, 4)].bg, theme.stripe);
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
    assert_eq!(count("pgdata"), theme.count);
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
    let mut app = App::new(Theme::default(), Instant::now());
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
