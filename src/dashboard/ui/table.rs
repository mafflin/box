//! The view's table: columns as wide as their content, flexible ones cut to fit

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{bold, fg, fill, fit, put, rounded_box};
use crate::colors;
use crate::dashboard::app::{App, Pending, SortKey, View};
use crate::dashboard::theme::Theme;
use crate::docker::Container;
use crate::format::{compact_bytes, short_id};

const GAP: u16 = 2;
const LEAD: u16 = 1;

struct Column {
    title: &'static str,
    sort: Option<SortKey>,
    right_aligned: bool,
    /// How readily it gives up width when there isn't enough, 0 for never
    flex: usize,
    min: usize,
    cells: Vec<(String, Style)>,
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
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let theme = app.theme;
    let view = app.view;
    let inner = rounded_box(buf, area, theme.border, view.title(), bold(theme.text));
    if inner.width < 8 || inner.height < 2 {
        return;
    }

    let (rows, selected) = app.selection();
    let (mut columns, dimmed) = match view {
        View::Containers => containers(app, &rows),
        View::Images => images(app, &rows),
        View::Volumes => volumes(app, &rows),
    };
    let sort = app.sort[view.index()];
    let arrow = if sort.descending { " ▼" } else { " ▲" };
    let headers: Vec<String> = columns
        .iter()
        .map(|column| {
            let sorted = column.sort == Some(sort.key);
            format!("{}{}", column.title, if sorted { arrow } else { "" })
        })
        .collect();
    fit_widths(&mut columns, &headers, usize::from(inner.width - LEAD - 1));

    fill(buf, Rect { height: 1, ..inner }, theme.text, theme.bg_bar);
    let cells = headers.iter().zip(&columns).map(|(header, column)| {
        let color = if column.sort == Some(sort.key) {
            theme.accent
        } else {
            theme.text
        };
        (header.clone(), bold(color))
    });
    draw_cells(buf, Rect { height: 1, ..inner }, &columns, cells);

    let height = usize::from(inner.height - 1);
    app.page = height.saturating_sub(1).max(1);
    let scroll = scroll(&mut app.scroll[view.index()], selected, height, rows.len());
    for (line, position) in (scroll..rows.len()).take(height).enumerate() {
        let row = rows[position];
        let is_selected = Some(position) == selected;
        let row_area = Rect::new(
            inner.x,
            inner.y + 1 + u16::try_from(line).unwrap_or(0),
            inner.width,
            1,
        );
        if is_selected {
            fill(buf, row_area, theme.text, theme.bg_selected);
        } else if position % 2 == 1 {
            fill(buf, row_area, theme.text, theme.bg_alt);
        }
        let cells = columns.iter().map(|column| {
            let (text, style) = column.cells[row].clone();
            if dimmed[row] && column.title != "STATUS" {
                (text, fg(theme.dim))
            } else {
                (text, style)
            }
        });
        draw_cells(buf, row_area, &columns, cells);
    }

    if rows.is_empty() {
        let message = empty_message(app);
        let width = u16::try_from(message.chars().count()).unwrap_or(0);
        let x = inner.x + inner.width.saturating_sub(width) / 2;
        let y = inner.y + 1 + (inner.height - 1).min(3) / 2;
        put(buf, x, y, &message, fg(theme.dim), inner.right());
    } else if let Some(selected) = selected {
        let position = format!(" {}/{} ", selected + 1, rows.len());
        let width = u16::try_from(position.len()).unwrap_or(0);
        let x = area.right().saturating_sub(2 + width);
        put(
            buf,
            x,
            area.bottom() - 1,
            &position,
            fg(theme.muted),
            area.right() - 1,
        );
    }
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
fn draw_cells(
    buf: &mut Buffer,
    area: Rect,
    columns: &[Column],
    cells: impl Iterator<Item = (String, Style)>,
) {
    let mut x = area.x + LEAD;
    for (column, (text, style)) in columns.iter().zip(cells) {
        let text = fit(&text, column.width, column.right_aligned);
        put(buf, x, area.y, &text, style, area.right());
        x = x.saturating_add(column_width(column) + GAP);
    }
}

fn column_width(column: &Column) -> u16 {
    u16::try_from(column.width).unwrap_or(u16::MAX)
}

/// Each column as wide as its widest cell, then flexible ones narrowed until they fit
fn fit_widths(columns: &mut [Column], headers: &[String], available: usize) {
    for (column, header) in columns.iter_mut().zip(headers) {
        let header = header.chars().count();
        column.width = column
            .cells
            .iter()
            .map(|(text, _)| text.chars().count())
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
            .max_by_key(|column| column.width * 12 / column.flex)
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
        "no running containers · press a to show all".to_string()
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
    let mut status = Column::new("STATUS", Some(SortKey::Started)).flex(1, 10);
    let mut cpu = Column::new("CPU%", Some(SortKey::Cpu)).right();
    let mut memory = Column::new("MEM", Some(SortKey::Memory)).right();
    let mut network = Column::new("NET I/O", None).right();
    let mut pids = Column::new("PIDS", None).right();
    let mut ports = Column::new("PORTS", None).flex(3, 8);
    let none = || ("—".to_string(), fg(theme.dim));

    for container in &app.containers {
        name.cells.push((container.name.clone(), fg(colors::NAME)));
        image
            .cells
            .push((container.image.clone(), fg(colors::IMAGE)));
        status.cells.push(status_cell(app, container));
        let usage = app.container_stats(container).map_or_else(
            || [none(), none(), none(), none()],
            |usage| {
                [
                    (
                        format!("{:.1}", usage.cpu),
                        fg(theme.level(usage.cpu / 100.0)),
                    ),
                    (
                        compact_bytes(usage.memory),
                        fg(theme.level(fraction(usage.memory, usage.memory_limit))),
                    ),
                    (
                        format!(
                            "{} / {}",
                            compact_bytes(usage.network.0),
                            compact_bytes(usage.network.1)
                        ),
                        fg(theme.text),
                    ),
                    (usage.pids.to_string(), fg(theme.text)),
                ]
            },
        );
        for (column, cell) in [&mut cpu, &mut memory, &mut network, &mut pids]
            .into_iter()
            .zip(usage)
        {
            column.cells.push(cell);
        }
        ports
            .cells
            .push((container.ports.join(" "), fg(colors::PORTS)));
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

fn status_cell(app: &App, container: &Container) -> (String, Style) {
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
        "running" if container.status.contains("unhealthy") => theme.yellow,
        "running" => theme.green,
        "paused" | "restarting" => theme.yellow,
        "exited" if container.exit_code == 0 => theme.dim,
        "created" => theme.muted,
        "exited" | "dead" | "removing" => theme.red,
        _ => theme.text,
    }
}

#[allow(clippy::cast_precision_loss)]
pub fn fraction(used: u64, limit: u64) -> f64 {
    if limit == 0 {
        0.0
    } else {
        used as f64 / limit as f64
    }
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
        image.cells.push((entry.reference(), fg(colors::IMAGE)));
        id.cells
            .push((short_id(&entry.id).to_string(), fg(colors::ID)));
        let foreign = entry.is_foreign(&app.host);
        platform.cells.push((
            if foreign {
                entry.platform.clone()
            } else {
                String::new()
            },
            fg(theme.yellow),
        ));
        size.cells.push((entry.size.clone(), fg(colors::SIZE)));
        created
            .cells
            .push((entry.created_since.clone(), fg(colors::CREATED)));
        users
            .cells
            .push(count_cell(&theme, app.image_users(entry).len()));
    }
    let dimmed = app
        .images
        .iter()
        .map(crate::docker::Image::is_dangling)
        .collect();
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
        name.cells.push((volume.name.clone(), fg(colors::NAME)));
        driver
            .cells
            .push((volume.driver.clone(), fg(colors::DRIVER)));
        created
            .cells
            .push((volume.created.clone(), fg(colors::CREATED)));
        users
            .cells
            .push(count_cell(&theme, app.volume_users(volume).len()));
    }
    let dimmed = vec![false; app.volumes.len()];
    let mut columns = vec![name, driver, created, users];
    keep_rows(&mut columns, rows);
    (columns, dimmed)
}

fn count_cell(theme: &Theme, count: usize) -> (String, Style) {
    (
        count.to_string(),
        fg(if count == 0 { theme.dim } else { colors::COUNT }),
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
