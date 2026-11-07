//! Plain tables for `box list`, colored with ANSI codes, a cell can span lines

use ratatui::style::Color;

use crate::format::{ESC, Highlighted};
use crate::theme::Theme;

/// The SGR parameter for bold, the titles' style
const BOLD: &str = "1";
/// Between two columns
const COLUMN_GAP: &str = "  ";
/// The SGR parameter that ends a style
const RESET: &str = "0";
/// The SGR parameters before a text color's number of the 256, or its red, green and blue
const INDEXED_COLOR: &str = "38;5";
const RGB_COLOR: &str = "38;2";

/// Its color is the theme's for its field
pub struct Column {
    pub title: &'static str,
    pub color: fn(&Theme) -> Color,
    pub right_aligned: bool,
}

impl Column {
    pub const fn left(title: &'static str, color: fn(&Theme) -> Color) -> Self {
        Self {
            title,
            color,
            right_aligned: false,
        }
    }

    pub const fn right(title: &'static str, color: fn(&Theme) -> Color) -> Self {
        Self {
            title,
            color,
            right_aligned: true,
        }
    }
}

/// A cell's lines, none for an empty list
pub type Cell = Vec<Highlighted>;

/// One line cell
pub fn cell(text: impl Into<String>) -> Cell {
    vec![Highlighted::plain(text)]
}

/// A cell per line
pub fn lines(texts: impl IntoIterator<Item = impl Into<String>>) -> Cell {
    texts.into_iter().map(Highlighted::plain).collect()
}

/// Nothing for no rows
pub fn render(columns: &[Column], rows: &[Vec<Cell>], theme: &Theme) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let widths = column_widths(columns, rows);
    let colors: Vec<Option<String>> = columns
        .iter()
        .map(|column| sgr((column.color)(theme)))
        .collect();
    // A cell's highlight is the working directory
    let workdir = sgr(theme.workdir);
    let body: String = rows
        .iter()
        .flat_map(|row| row_lines(row))
        .map(|cells| table_line(columns, &cells, &widths, &colors, workdir.as_deref()))
        .collect();
    header(columns, &widths) + &body
}

/// Each column as wide as its title or its longest line
fn column_widths(columns: &[Column], rows: &[Vec<Cell>]) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            rows.iter()
                .flat_map(|row| &row[index])
                .map(Highlighted::len)
                .chain([column.title.chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect()
}

/// The titles in bold
fn header(columns: &[Column], widths: &[usize]) -> String {
    let titles: Vec<Highlighted> = columns
        .iter()
        .map(|column| Highlighted::plain(column.title))
        .collect();
    let bold = vec![Some(BOLD.to_string()); columns.len()];
    table_line(columns, &titles, widths, &bold, None)
}

/// A row's cells line by line, as many lines as its tallest cell, empty below a shorter cell's last line
fn row_lines(row: &[Cell]) -> impl Iterator<Item = Vec<Highlighted>> + '_ {
    let height = row.iter().map(Vec::len).max().unwrap_or(0);
    (0..height).map(move |line| {
        row.iter()
            .map(|cell| cell.get(line).cloned().unwrap_or_default())
            .collect()
    })
}

/// Each cell in its column's SGR code, its highlight in `highlight`'s
fn table_line(
    columns: &[Column],
    cells: &[Highlighted],
    widths: &[usize],
    codes: &[Option<String>],
    highlight: Option<&str>,
) -> String {
    let line = columns
        .iter()
        .zip(cells)
        .zip(widths)
        .zip(codes)
        .map(|(((column, cell), width), code)| {
            let padding = " ".repeat(width - cell.len());
            let code = code.as_deref();
            let painted = paint(&cell.prefix, code)
                + &paint(&cell.highlight, highlight)
                + &paint(&cell.suffix, code);
            if column.right_aligned {
                padding + &painted
            } else {
                painted + &padding
            }
        })
        .collect::<Vec<_>>()
        .join(COLUMN_GAP);
    format!("{}\n", line.trim_end())
}

/// The SGR parameters for a text color, none for the terminal's own
fn sgr(color: Color) -> Option<String> {
    let code = match color {
        Color::Reset => return None,
        Color::Black => 30,
        Color::Red => 31,
        Color::Green => 32,
        Color::Yellow => 33,
        Color::Blue => 34,
        Color::Magenta => 35,
        Color::Cyan => 36,
        Color::Gray => 37,
        Color::DarkGray => 90,
        Color::LightRed => 91,
        Color::LightGreen => 92,
        Color::LightYellow => 93,
        Color::LightBlue => 94,
        Color::LightMagenta => 95,
        Color::LightCyan => 96,
        Color::White => 97,
        Color::Indexed(index) => return Some(format!("{INDEXED_COLOR};{index}")),
        Color::Rgb(red, green, blue) => return Some(format!("{RGB_COLOR};{red};{green};{blue}")),
    };
    Some(code.to_string())
}

fn paint(text: &str, code: Option<&str>) -> String {
    match code {
        _ if text.is_empty() => String::new(),
        Some(code) => format!("{ESC}[{code}m{text}{ESC}[{RESET}m"),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests;
