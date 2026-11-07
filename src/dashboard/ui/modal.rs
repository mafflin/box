//! Dialogs over the dashboard: help, confirming a change, and prune

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};

use super::{badge, bold, fg, fill, fit, padded, put, put_line, rounded_box, size, width};
use crate::commands::PruneTarget;
use crate::dashboard::app::{App, Confirm, Modal, PruneDialog, SortKey};
use crate::dashboard::keys::{self, either, names};
use crate::format::human_size;
use crate::theme::Theme;

/// Text's indent inside a dialog; the prune choices start before it, with their `›` marker
const INDENT: u16 = 1;

const HELP_WIDTH: u16 = 76;
const HELP_HEIGHT: u16 = 21;
/// Where the second column of keys starts
const HELP_SECOND_COLUMN: u16 = 40;
/// Where a key's description starts
const HELP_KEY_WIDTH: u16 = 12;

const CONFIRM_HEIGHT: u16 = 7;
const CONFIRM_MIN_WIDTH: usize = 44;
/// Room around the question or the command, whichever is longer
const CONFIRM_MARGIN: usize = 8;

const PRUNE_WIDTH: u16 = 56;
const PRUNE_HEIGHT: u16 = 11;
const PRUNE_LABEL_WIDTH: usize = 20;
const PRUNE_COUNT_WIDTH: usize = 4;

/// A title and its keys with what they do
type Section = (&'static str, Vec<(String, String)>);

/// What the help shows, its keys from `keys`: General and Sort on the left, the rest on the right
fn help_sections() -> [Section; 4] {
    let key = |key: char| names(&[key]);
    let line = |keys: String, what: &str| (keys, what.to_string());
    let general = vec![
        line(
            either(&[keys::ARROWS, &names(&[keys::DOWN, keys::UP])]),
            "move",
        ),
        line(names(&[keys::TOP, keys::BOTTOM]), "top / bottom"),
        line(
            either(&[keys::TAB, &names(&keys::VIEW_KEYS)]),
            "switch view",
        ),
        line(key(keys::FILTER), "filter"),
        line(keys::ESC.to_string(), "clear filter"),
        line(key(keys::DETAILS), "details bottom/right/off"),
        line(key(keys::PRUNE), "prune"),
        line(key(keys::QUIT), "quit"),
    ];
    // In the tabs line's words; time is when containers started, when images and volumes were created
    let time = format!(
        "{} / {}",
        SortKey::Started.label(),
        SortKey::Created.label()
    );
    let sort = [SortKey::Cpu, SortKey::Memory, SortKey::Name, SortKey::Size]
        .into_iter()
        .map(|sort| line(key(sort.key()), sort.label()))
        .chain([line(key(keys::SORT_TIME), &time)])
        .collect();
    let containers = vec![
        line(key(keys::STOP), "stop / start"),
        line(key(keys::RESTART), "restart"),
        line(key(keys::REMOVE), "remove"),
        line(
            either(&[&key(keys::LOGS), keys::ENTER]),
            "logs (not for -it)",
        ),
        line(key(keys::SHELL), "shell (bash or sh)"),
        line(key(keys::SHOW_ALL), "show stopped"),
    ];
    let images_and_volumes = vec![line(key(keys::REMOVE), "remove if unused")];
    [
        ("General", general),
        ("Sort", sort),
        ("Containers", containers),
        ("Images · Volumes", images_and_volumes),
    ]
}

/// Where `help_sections` splits into the help's two columns
const LEFT_SECTIONS: usize = 2;

/// In `PruneTarget::ORDER`
const PRUNE_LABELS: [&str; PruneTarget::ORDER.len()] =
    ["stopped containers", "unused volumes", "dangling images"];
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
    let inner = dialog(
        buf,
        area,
        HELP_WIDTH,
        HELP_HEIGHT,
        "Keys",
        theme.accent,
        theme.bar,
    );
    let sections = help_sections();
    help_column(
        buf,
        inner,
        inner.x + INDENT,
        &sections[..LEFT_SECTIONS],
        theme,
    );
    help_column(
        buf,
        inner,
        inner.x + HELP_SECOND_COLUMN,
        &sections[LEFT_SECTIONS..],
        theme,
    );
    put(
        buf,
        inner.x + INDENT,
        last_row(inner),
        "any key closes",
        fg(theme.dim),
        inner.right(),
    );
}

/// The sections from `x` down, a blank line after each
fn help_column(buf: &mut Buffer, inner: Rect, x: u16, sections: &[Section], theme: &Theme) {
    let mut y = inner.y + 1;
    for (title, keys) in sections {
        put(buf, x, y, title, bold(theme.accent), inner.right());
        y += 1;
        for (key, description) in keys {
            put(buf, x, y, key, bold(theme.text), inner.right());
            put(
                buf,
                x + HELP_KEY_WIDTH,
                y,
                description,
                fg(theme.text),
                inner.right(),
            );
            y += 1;
        }
        y += 1;
    }
}

fn confirmation(buf: &mut Buffer, area: Rect, theme: &Theme, confirm: &Confirm) {
    let color = if confirm.danger {
        theme.red
    } else {
        theme.accent
    };
    let text_width = width(&confirm.question).max(width(&confirm.command));
    let dialog_width = (text_width + CONFIRM_MARGIN).max(CONFIRM_MIN_WIDTH);
    let dialog_width = size(dialog_width);
    let inner = dialog(
        buf,
        area,
        dialog_width,
        CONFIRM_HEIGHT,
        &confirm.title,
        color,
        theme.bar,
    );
    let x = inner.x + INDENT;
    put(
        buf,
        x,
        inner.y + 1,
        &confirm.question,
        bold(theme.text),
        inner.right(),
    );
    put(
        buf,
        x,
        inner.y + 2,
        &confirm.command,
        fg(theme.muted),
        inner.right(),
    );
    let buttons = Line::from(vec![
        Span::styled(format!(" {} ", keys::YES), badge(color)),
        Span::styled(" confirm    ", fg(theme.text)),
        Span::styled(
            format!(" {} ", keys::NO),
            bold(theme.text).bg(theme.selected),
        ),
        Span::styled(" cancel", fg(theme.text)),
    ]);
    put_line(buf, x, last_row(inner), &buttons, inner.right());
}

fn prune(buf: &mut Buffer, area: Rect, app: &App, state: &PruneDialog) {
    let theme = &app.theme;
    let inner = dialog(
        buf,
        area,
        PRUNE_WIDTH,
        PRUNE_HEIGHT,
        "Prune",
        theme.accent,
        theme.bar,
    );
    let choices = PRUNE_LABELS.iter().zip(app.prunable()).zip(state.chosen);
    let mut y = inner.y + 1;
    for (i, ((label, (count, size)), chosen)) in choices.enumerate() {
        let current = i == state.cursor;
        if current {
            fill(
                buf,
                Rect::new(inner.x, y, inner.width, 1),
                theme.text,
                theme.selected,
            );
        }
        let line = prune_choice(theme, label, count, size, chosen, current);
        put_line(buf, inner.x, y, &line, inner.right());
        y += 1;
    }
    put(
        buf,
        inner.x + INDENT,
        y + 1,
        PRUNE_ORDER,
        fg(theme.dim),
        inner.right(),
    );
    let hints = keys_line(
        &[
            (keys::ARROWS, "move"),
            (keys::SPACE_NAME, "toggle"),
            (keys::ENTER, "run"),
            (keys::ESC, "cancel"),
        ],
        theme,
    );
    put_line(
        buf,
        inner.x + INDENT,
        last_row(inner),
        &hints,
        inner.right(),
    );
}

/// `› [x] stopped containers     3  1.2GB`: the cursor's marker, whether it's chosen, how many and their size
fn prune_choice(
    theme: &Theme,
    label: &str,
    count: usize,
    size: u64,
    chosen: bool,
    current: bool,
) -> Line<'static> {
    let size = if size > 0 {
        format!("  {}", human_size(size))
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(if current { "› " } else { "  " }, bold(theme.accent)),
        Span::styled(
            if chosen { "[x] " } else { "[ ] " },
            bold(if chosen { theme.accent } else { theme.muted }),
        ),
        Span::styled(fit(label, PRUNE_LABEL_WIDTH, false), fg(theme.text)),
        Span::styled(
            fit(&count.to_string(), PRUNE_COUNT_WIDTH, true),
            bold(if count > 0 { theme.text } else { theme.dim }),
        ),
        Span::styled(size, fg(theme.text)),
    ])
}

/// `key what  key what`
fn keys_line(keys: &[(&'static str, &'static str)], theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, description)) in keys.iter().enumerate() {
        let gap = if i + 1 < keys.len() { "  " } else { "" };
        spans.push(Span::styled(*key, bold(theme.accent)));
        spans.push(Span::styled(
            format!(" {description}{gap}"),
            fg(theme.muted),
        ));
    }
    Line::from(spans)
}

/// The last row inside a dialog
const fn last_row(inner: Rect) -> u16 {
    inner.bottom().saturating_sub(1)
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
    padded(rounded_box(buf, outer, color, title, bold(color)))
}
