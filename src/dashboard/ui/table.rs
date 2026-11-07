//! The view's table: columns as wide as their content, flexible ones cut to fit

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{
    NO_VALUE, bold, fg, fill, fit, put, put_on_border, rounded_box, size, sort_arrow, width,
};
use crate::dashboard::app::{App, Pending, Sort, SortKey, View};
use crate::dashboard::keys;
use crate::docker::{Container, Image, Stats, state};
use crate::format::{compact_bytes, short_id};
use crate::theme::Theme;

/// Between two columns
const GAP: u16 = 2;
/// Before the first column
const LEAD: u16 = 1;
/// After the last column
const TRAIL: u16 = 1;
/// The header and a row
const MIN_INNER_HEIGHT: u16 = 2;
/// Narrower tables show nothing
const MIN_INNER_WIDTH: u16 = 8;
/// So `width / flex` compares in whole numbers for flex 1 to 4
const FLEX_SCALE: usize = 12;

/// A cell's text and style
type Cell = (String, Style);

struct Column {
    title: &'static str,
    sort: Option<SortKey>,
    right_aligned: bool,
    /// How readily it gives up width when there isn't enough, 0 for never
    flex: usize,
    min: usize,
    /// Keeps its colors in a dimmed row
    never_dimmed: bool,
    cells: Vec<Cell>,
    width: usize,
}

impl Column {
    fn new(title: &'static str, sort: Option<SortKey>) -> Self {
        Self {
            title,
            sort,
            right_aligned: false,
            flex: 0,
            min: 0,
            never_dimmed: false,
            cells: Vec::new(),
            width: 0,
        }
    }

    fn right(mut self) -> Self {
        self.right_aligned = true;
        self
    }

    fn flex(mut self, flex: usize, min: usize) -> Self {
        self.flex = flex;
        self.min = min;
        self
    }

    fn never_dimmed(mut self) -> Self {
        self.never_dimmed = true;
        self
    }
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;
    let view = app.view;
    let inner = rounded_box(buf, area, theme.border, view.title(), bold(theme.text));
    if inner.width < MIN_INNER_WIDTH || inner.height < MIN_INNER_HEIGHT {
        return;
    }

    let (rows, selected) = app.selection();
    let (mut columns, dimmed) = match view {
        View::Containers => containers(app, &rows),
        View::Images => images(app, &rows),
        View::Volumes => volumes(app, &rows),
    };
    let sort = app.sort[view.index()];
    let headers = headers(&columns, sort);
    fit_widths(
        &mut columns,
        &headers,
        usize::from(inner.width - LEAD - TRAIL),
    );
    draw_header(buf, inner, &columns, &headers, sort, &theme);

    let body = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    let height = usize::from(body.height);
    app.page = height.saturating_sub(1).max(1);
    let scroll = scroll(&mut app.scroll[view.index()], selected, height, rows.len());
    for (y, position) in (body.y..body.bottom()).zip(scroll..rows.len()) {
        let row = rows[position];
        let background = if Some(position) == selected {
            Some(theme.selected)
        } else {
            (position % 2 == 1).then_some(theme.stripe)
        };
        let row_area = Rect {
            y,
            height: 1,
            ..body
        };
        draw_row(
            buf,
            row_area,
            &columns,
            row,
            dimmed[row],
            background,
            &theme,
        );
    }

    if rows.is_empty() {
        draw_empty(buf, body, &empty_message(app), &theme);
    } else if let Some(selected) = selected {
        let position = format!(" {}/{} ", selected + 1, rows.len());
        put_on_border(buf, area, area.bottom() - 1, &position, fg(theme.muted));
    }
}

/// The columns' titles, an arrow on the sorted one
fn headers(columns: &[Column], sort: Sort) -> Vec<String> {
    let arrow = format!(" {}", sort_arrow(sort));
    columns
        .iter()
        .map(|column| {
            let sorted = column.sort == Some(sort.key);
            format!(
                "{}{}",
                column.title,
                if sorted { arrow.as_str() } else { "" }
            )
        })
        .collect()
}

/// The titles on a bar, the sorted one in the accent color
fn draw_header(
    buf: &mut Buffer,
    inner: Rect,
    columns: &[Column],
    headers: &[String],
    sort: Sort,
    theme: &Theme,
) {
    let area = Rect { height: 1, ..inner };
    fill(buf, area, theme.text, theme.bar);
    let cells = headers.iter().zip(columns).map(|(header, column)| {
        let color = if column.sort == Some(sort.key) {
            theme.accent
        } else {
            theme.text
        };
        (header.clone(), bold(color))
    });
    draw_cells(buf, area, columns, cells);
}

/// A row's cells on its background, dim but for the columns that never are when it's dimmed
fn draw_row(
    buf: &mut Buffer,
    area: Rect,
    columns: &[Column],
    row: usize,
    dimmed: bool,
    background: Option<Color>,
    theme: &Theme,
) {
    if let Some(background) = background {
        fill(buf, area, theme.text, background);
    }
    let cells = columns.iter().map(|column| {
        let (text, style) = column.cells[row].clone();
        if dimmed && !column.never_dimmed {
            (text, fg(theme.dim))
        } else {
            (text, style)
        }
    });
    draw_cells(buf, area, columns, cells);
}

/// In the middle, a row below the header when there's room
fn draw_empty(buf: &mut Buffer, body: Rect, message: &str, theme: &Theme) {
    let message_width = size(width(message));
    let x = body.x + body.width.saturating_sub(message_width) / 2;
    let y = body.y + u16::from(body.height > 1);
    put(buf, x, y, message, fg(theme.dim), body.right());
}

/// The first row shown, moved to keep the selection in view
fn scroll(scroll: &mut usize, selected: Option<usize>, height: usize, rows: usize) -> usize {
    if let Some(selected) = selected {
        if selected < *scroll {
            *scroll = selected;
        } else if selected >= *scroll + height {
            *scroll = selected + 1 - height;
        }
    }
    *scroll = (*scroll).min(rows.saturating_sub(height));
    *scroll
}

/// A line's cells, each fitted to its column
fn draw_cells(buf: &mut Buffer, area: Rect, columns: &[Column], cells: impl Iterator<Item = Cell>) {
    let mut x = area.x + LEAD;
    for (column, (text, style)) in columns.iter().zip(cells) {
        let text = fit(&text, column.width, column.right_aligned);
        put(buf, x, area.y, &text, style, area.right());
        x = x.saturating_add(column_width(column) + GAP);
    }
}

fn column_width(column: &Column) -> u16 {
    size(column.width)
}

/// Each column as wide as its widest cell, then flexible ones narrowed until they fit
fn fit_widths(columns: &mut [Column], headers: &[String], available: usize) {
    for (column, header) in columns.iter_mut().zip(headers) {
        let header = width(header);
        column.width = column
            .cells
            .iter()
            .map(|(text, _)| width(text))
            .chain([header])
            .max()
            .unwrap_or(0);
        column.min = column.min.max(header);
    }
    let gaps = usize::from(GAP) * columns.len().saturating_sub(1);
    let total =
        |columns: &[Column]| columns.iter().map(|column| column.width).sum::<usize>() + gaps;
    while total(columns) > available {
        let Some(widest) = columns
            .iter_mut()
            .filter(|column| column.flex > 0 && column.width > column.min)
            .max_by_key(|column| column.width * FLEX_SCALE / column.flex)
        else {
            return;
        };
        widest.width -= 1;
    }
}

fn empty_message(app: &App) -> String {
    let filter = &app.filter[app.view.index()];
    if !app.loaded {
        "loading…".to_string()
    } else if !filter.is_empty() {
        format!("nothing matches “{filter}”")
    } else if app.view == View::Containers && !app.show_all && !app.containers.is_empty() {
        format!(
            "no running containers · press {} to show all",
            keys::SHOW_ALL
        )
    } else {
        format!("no {}", app.view.title().to_lowercase())
    }
}

/// The cells of every container, by its index, and which are stopped
fn containers(app: &App, rows: &[usize]) -> (Vec<Column>, Vec<bool>) {
    let theme = app.theme;
    let mut name = Column::new("NAME", Some(SortKey::Name)).flex(2, 8);
    let mut image = Column::new("IMAGE", None).flex(3, 10);
    // Up for how long, so sorted by the start
    let mut status = Column::new("STATUS", Some(SortKey::Started))
        .flex(1, 10)
        .never_dimmed();
    let mut cpu = Column::new("CPU%", Some(SortKey::Cpu)).right();
    let mut memory = Column::new("MEM", Some(SortKey::Memory)).right();
    let mut network = Column::new("NET I/O", None).right();
    let mut pids = Column::new("PIDS", None).right();
    let mut ports = Column::new("PORTS", None).flex(3, 8);

    for container in &app.containers {
        name.cells.push((container.name.clone(), fg(theme.name)));
        image.cells.push((container.image.clone(), fg(theme.image)));
        status.cells.push(status_cell(app, container));
        let usage = usage_cells(&theme, app.container_stats(container));
        for (column, cell) in [&mut cpu, &mut memory, &mut network, &mut pids]
            .into_iter()
            .zip(usage)
        {
            column.cells.push(cell);
        }
        ports
            .cells
            .push((container.ports.join(" "), fg(theme.ports)));
    }
    let dimmed = app
        .containers
        .iter()
        .map(|container| !app.is_up(container))
        .collect();
    let mut columns = vec![name, image, status, cpu, memory, network, pids, ports];
    keep_rows(&mut columns, rows);
    (columns, dimmed)
}

/// CPU, memory, network and PIDs, colored by how close they are to the limit; `—` without stats
fn usage_cells(theme: &Theme, stats: Option<&Stats>) -> [Cell; 4] {
    let Some(stats) = stats else {
        return std::array::from_fn(|_| (NO_VALUE.to_string(), fg(theme.dim)));
    };
    let (received, sent) = stats.network;
    let memory = fraction(stats.memory, stats.memory_limit);
    [
        (
            format!("{:.1}", stats.cpu),
            fg(theme.level(stats.cpu / 100.0, theme.text)),
        ),
        (
            compact_bytes(stats.memory),
            fg(theme.level(memory, theme.text)),
        ),
        (
            format!("{} / {}", compact_bytes(received), compact_bytes(sent)),
            fg(theme.text),
        ),
        (stats.pids.to_string(), fg(theme.text)),
    ]
}

fn status_cell(app: &App, container: &Container) -> Cell {
    let theme = app.theme;
    if let Some(pending) = app.pending(container) {
        let color = if pending == Pending::Removing {
            theme.red
        } else {
            theme.yellow
        };
        return (pending.label().to_string(), fg(color));
    }
    (container.status.clone(), fg(state_color(&theme, container)))
}

pub fn state_color(theme: &Theme, container: &Container) -> Color {
    match container.state.as_str() {
        state::RUNNING if container.is_unhealthy() => theme.yellow,
        state::RUNNING => theme.green,
        state::PAUSED | state::RESTARTING => theme.yellow,
        state::EXITED if container.exit_code == 0 => theme.dim,
        state::CREATED => theme.muted,
        state::EXITED | state::DEAD | state::REMOVING => theme.red,
        _ => theme.text,
    }
}

/// `used` of `limit`, 0 without a limit
pub fn fraction(used: u64, limit: u64) -> f64 {
    /// In thousandths, so it converts to a float without loss; plenty for the 50% and 80% colors
    const PARTS: u32 = 1000;
    if limit == 0 {
        return 0.0;
    }
    let parts = u128::from(used) * u128::from(PARTS) / u128::from(limit);
    f64::from(u32::try_from(parts).unwrap_or(u32::MAX)) / f64::from(PARTS)
}

fn images(app: &App, rows: &[usize]) -> (Vec<Column>, Vec<bool>) {
    let theme = app.theme;
    let mut image = Column::new("IMAGE", Some(SortKey::Name)).flex(3, 10);
    let mut id = Column::new("ID", None);
    // Only for images docker's host can't run natively, when there are any
    let mut platform = Column::new("PLATFORM", None);
    let mut size = Column::new("SIZE", Some(SortKey::Size)).right();
    let mut created = Column::new("CREATED", Some(SortKey::Created)).right();
    let mut users = Column::new("CONTAINERS", None).right();
    for entry in &app.images {
        image.cells.push((entry.reference(), fg(theme.image)));
        id.cells
            .push((short_id(&entry.id).to_string(), fg(theme.id)));
        let foreign = entry.is_foreign(&app.host);
        let shown_platform = if foreign {
            entry.platform.clone()
        } else {
            String::new()
        };
        platform.cells.push((shown_platform, fg(theme.yellow)));
        size.cells.push((entry.size.clone(), fg(theme.size)));
        created
            .cells
            .push((entry.created_since.clone(), fg(theme.created)));
        users
            .cells
            .push(count_cell(&theme, app.image_users(entry).len()));
    }
    let dimmed = app.images.iter().map(Image::is_dangling).collect();
    let mut columns = vec![image, id];
    if rows.iter().any(|&row| !platform.cells[row].0.is_empty()) {
        columns.push(platform);
    }
    columns.extend([size, created, users]);
    keep_rows(&mut columns, rows);
    (columns, dimmed)
}

fn volumes(app: &App, rows: &[usize]) -> (Vec<Column>, Vec<bool>) {
    let theme = app.theme;
    let mut name = Column::new("VOLUME", Some(SortKey::Name)).flex(3, 12);
    let mut driver = Column::new("DRIVER", None);
    let mut created = Column::new("CREATED", Some(SortKey::Created)).right();
    let mut users = Column::new("CONTAINERS", None).right();
    for volume in &app.volumes {
        name.cells.push((volume.name.clone(), fg(theme.name)));
        driver.cells.push((volume.driver.clone(), fg(theme.driver)));
        created
            .cells
            .push((volume.created.clone(), fg(theme.created)));
        users
            .cells
            .push(count_cell(&theme, app.volume_users(volume).len()));
    }
    let dimmed = vec![false; app.volumes.len()];
    let mut columns = vec![name, driver, created, users];
    keep_rows(&mut columns, rows);
    (columns, dimmed)
}

fn count_cell(theme: &Theme, count: usize) -> Cell {
    (
        count.to_string(),
        fg(if count == 0 { theme.dim } else { theme.count }),
    )
}

/// Only the shown rows count for the widths, kept by index
fn keep_rows(columns: &mut [Column], rows: &[usize]) {
    for column in columns {
        let cells = std::mem::take(&mut column.cells);
        column.cells = vec![(String::new(), Style::new()); cells.len()];
        for &row in rows {
            column.cells[row] = cells[row].clone();
        }
    }
}
