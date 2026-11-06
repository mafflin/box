//! Plain tables for `box list`, colored with ANSI codes, a cell can span lines

use ratatui::style::Color;

use crate::format::Highlighted;
use crate::theme::Theme;

const BOLD: &str = "1";
const COLUMN_GAP: &str = "  ";

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
    let bold = vec![Some(BOLD.to_string()); columns.len()];
    let colors: Vec<Option<String>> = columns
        .iter()
        .map(|column| sgr((column.color)(theme)))
        .collect();
    // A cell's highlight is the working directory
    let workdir = sgr(theme.workdir);
    let mut table = table_line(columns, &titles, &widths, &bold, None);
    for row in rows {
        let height = row.iter().map(Vec::len).max().unwrap_or(0);
        for line in 0..height {
            let cells: Vec<Highlighted> = row
                .iter()
                .map(|cell| cell.get(line).cloned().unwrap_or_default())
                .collect();
            table.push_str(&table_line(
                columns,
                &cells,
                &widths,
                &colors,
                workdir.as_deref(),
            ));
        }
    }
    table
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
        Color::Indexed(index) => return Some(format!("38;5;{index}")),
        Color::Rgb(red, green, blue) => return Some(format!("38;2;{red};{green};{blue}")),
    };
    Some(code.to_string())
}

fn paint(text: &str, code: Option<&str>) -> String {
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
            Column::left("NAME", |theme| theme.name),
            Column::right("SIZE", |_| Color::Blue),
            Column::left("PORTS", |_| Color::Green),
        ];
        let rows = vec![
            vec![cell("web"), cell("1MB"), lines(["80/tcp", "443/tcp"])],
            vec![cell("database"), cell("12MB"), lines(Vec::<String>::new())],
        ];
        let plain = render(&columns, &rows, &Theme::default())
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
    fn highlights_the_workdir_in_a_cell() {
        let columns = [Column::left("MOUNTS", |_| Color::Blue)];
        let theme = Theme {
            workdir: Color::Magenta,
            ..Theme::default()
        };
        let highlighted = Highlighted {
            prefix: "/data:".into(),
            highlight: "/srv".into(),
            suffix: ":ro".into(),
        };
        let table = render(&columns, &[vec![vec![highlighted]]], &theme);
        assert!(
            table.ends_with(
                "\u{1b}[34m/data:\u{1b}[0m\u{1b}[35m/srv\u{1b}[0m\u{1b}[34m:ro\u{1b}[0m\n"
            )
        );
        assert_eq!(render(&columns, &[], &theme), "");
    }

    #[test]
    fn sgr_codes() {
        assert_eq!(sgr(Color::Reset), None);
        assert_eq!(sgr(Color::Cyan).as_deref(), Some("36"));
        assert_eq!(sgr(Color::DarkGray).as_deref(), Some("90"));
        assert_eq!(sgr(Color::White).as_deref(), Some("97"));
        assert_eq!(sgr(Color::Indexed(239)).as_deref(), Some("38;5;239"));
        assert_eq!(sgr(Color::Rgb(1, 2, 3)).as_deref(), Some("38;2;1;2;3"));
    }
}
