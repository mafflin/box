//! Plain tables for `box list`, colored with ANSI codes, a cell can span lines

use ratatui::style::Color;

use crate::colors;
use crate::format::Highlighted;

const BOLD: u8 = 1;
const COLUMN_GAP: &str = "  ";

pub struct Column {
    pub title: &'static str,
    pub color: Color,
    pub right_aligned: bool,
}

impl Column {
    pub const fn left(title: &'static str, color: Color) -> Self {
        Self {
            title,
            color,
            right_aligned: false,
        }
    }

    pub const fn right(title: &'static str, color: Color) -> Self {
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
pub fn render(columns: &[Column], rows: &[Vec<Cell>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let widths: Vec<usize> = columns
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
        .collect();

    let titles: Vec<Highlighted> = columns
        .iter()
        .map(|c| Highlighted::plain(c.title))
        .collect();
    let mut table = table_line(columns, &titles, &widths, true);
    for row in rows {
        let height = row.iter().map(Vec::len).max().unwrap_or(0);
        for line in 0..height {
            let cells: Vec<Highlighted> = row
                .iter()
                .map(|cell| cell.get(line).cloned().unwrap_or_default())
                .collect();
            table.push_str(&table_line(columns, &cells, &widths, false));
        }
    }
    table
}

fn table_line(columns: &[Column], cells: &[Highlighted], widths: &[usize], header: bool) -> String {
    let line = columns
        .iter()
        .zip(cells)
        .zip(widths)
        .map(|((column, cell), width)| {
            let padding = " ".repeat(width - cell.len());
            let painted = if header {
                paint(&cell.prefix, Some(BOLD))
            } else {
                paint_cell(cell, column.color)
            };
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

/// The highlight is the working directory
fn paint_cell(cell: &Highlighted, color: Color) -> String {
    let (color, highlight) = (ansi(color), ansi(colors::WORKDIR));
    paint(&cell.prefix, color) + &paint(&cell.highlight, highlight) + &paint(&cell.suffix, color)
}

/// The SGR code for one of the 8 ANSI colors, none for the terminal's own
fn ansi(color: Color) -> Option<u8> {
    match color {
        Color::Black => Some(30),
        Color::Red => Some(31),
        Color::Green => Some(32),
        Color::Yellow => Some(33),
        Color::Blue => Some(34),
        Color::Magenta => Some(35),
        Color::Cyan => Some(36),
        Color::Gray => Some(37),
        _ => None,
    }
}

fn paint(text: &str, code: Option<u8>) -> String {
    match code {
        _ if text.is_empty() => String::new(),
        Some(code) => format!("\u{1b}[{code}m{text}\u{1b}[0m"),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_cells_over_lines() {
        let columns = [
            Column::left("NAME", Color::Reset),
            Column::right("SIZE", Color::Blue),
            Column::left("PORTS", Color::Green),
        ];
        let rows = vec![
            vec![cell("web"), cell("1MB"), lines(["80/tcp", "443/tcp"])],
            vec![cell("database"), cell("12MB"), lines(Vec::<String>::new())],
        ];
        let plain = render(&columns, &rows)
            .replace("\u{1b}[34m", "")
            .replace("\u{1b}[32m", "")
            .replace("\u{1b}[1m", "")
            .replace("\u{1b}[0m", "");
        assert_eq!(
            plain,
            "NAME      SIZE  PORTS\nweb        1MB  80/tcp\n                443/tcp\ndatabase  12MB\n"
        );
    }

    #[test]
    fn highlights_part_of_a_cell() {
        let columns = [Column::left("MOUNTS", Color::Blue)];
        let highlighted = Highlighted {
            prefix: "/data:".into(),
            highlight: "/srv".into(),
            suffix: ":ro".into(),
        };
        let table = render(&columns, &[vec![vec![highlighted]]]);
        assert!(
            table.ends_with(
                "\u{1b}[34m/data:\u{1b}[0m\u{1b}[35m/srv\u{1b}[0m\u{1b}[34m:ro\u{1b}[0m\n"
            )
        );
        assert_eq!(render(&columns, &[]), "");
    }
}
