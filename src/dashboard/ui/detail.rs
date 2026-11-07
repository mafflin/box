//! The selected item's details, with a container's CPU and memory over time

use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::table::{fraction, state_color};
use super::{NO_VALUE, bold, fg, fill, head, padded, put, put_line, rounded_box, size};
use crate::dashboard::app::{App, History, View};
use crate::dashboard::keys::{self, names};
use crate::docker::{Container, Image, Stats, Volume};
use crate::format::{self, ELLIPSIS, compact_bytes};
use crate::theme::Theme;

/// The labels' column, like `workdir  `
const LABEL_WIDTH: usize = 9;
/// Narrower details show nothing
const MIN_INNER_WIDTH: u16 = 4;
/// Smaller graphs aren't drawn: a label and a row of bars at the least
const MIN_GRAPH_WIDTH: u16 = 10;
const MIN_GRAPH_HEIGHT: u16 = 2;
/// What the graphs keep beside a container's details, for their labels
const MIN_GRAPHS_WIDTH: u16 = 30;
/// Between the details and the graphs
const GRAPHS_GAP: u16 = 3;
/// The text beside the graphs doesn't start out wider than this, however wide the pane
const MAX_HALF_WIDTH: u16 = 64;
/// The most rows the graphs take under the text
const MAX_GRAPHS_HEIGHT: u16 = 11;
/// Fewer rows than this aren't enough for the graphs
const MIN_GRAPHS_HEIGHT: u16 = 5;
/// The rows the graphs take when the text would leave too few, cutting the text
const CROWDED_GRAPHS_HEIGHT: u16 = 7;
/// How tall the pane must be for that; below it, there are no graphs
const MIN_CROWDED_HEIGHT: u16 = 14;
/// A graph's bars, from an empty cell to a full one in eighths
const EIGHTHS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
/// One less than `EIGHTHS`, which starts with the empty cell; a `u8`, so it converts to a float without loss
const EIGHTHS_PER_CELL: u8 = 8;
/// The CPU graph's top is at least this, so an idle container's graph stays low
const MIN_CPU_TOP: f64 = 5.0;
/// Room above the CPU peak
const CPU_HEADROOM: f64 = 1.15;
/// Where the memory graph starts, from its lowest value
const MEMORY_FLOOR: f64 = 0.9;
/// Room above the highest memory value
const MEMORY_HEADROOM: f64 = 1.05;
/// The rows the graphs need for the network and PIDs line under them
const MIN_USAGE_LINE_HEIGHT: u16 = 6;

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App, bottom: bool) {
    let theme = app.theme;
    let (rows, selected) = app.selection();
    let row = selected.map(|position| rows[position]);
    let title = row.map_or_else(String::new, |row| title(app, row));
    let inner = padded(rounded_box(buf, area, theme.border, &title, fg(theme.text)));
    if inner.width < MIN_INNER_WIDTH || inner.height == 0 {
        return;
    }
    let Some(row) = row else {
        put(
            buf,
            inner.x,
            inner.y,
            "nothing selected",
            fg(theme.dim),
            inner.right(),
        );
        return;
    };
    match app.view {
        View::Containers if bottom => draw_container_wide(buf, inner, app, &app.containers[row]),
        View::Containers => draw_container_tall(buf, inner, app, &app.containers[row]),
        View::Images => draw_lines(buf, inner, &image_lines(app, &app.images[row]), &theme),
        View::Volumes => draw_lines(buf, inner, &volume_lines(app, &app.volumes[row]), &theme),
    }
}

/// The rows the selection's details need with their box: their lines and the two borders, 0 for no selection
pub fn height(app: &mut App) -> u16 {
    let (rows, selected) = app.selection();
    let Some(row) = selected.map(|position| rows[position]) else {
        return 0;
    };
    let lines = match app.view {
        // Its mounts take a line each however wide they are
        View::Containers => container_lines(app, &app.containers[row], usize::MAX).len(),
        View::Images => image_lines(app, &app.images[row]).len(),
        View::Volumes => volume_lines(app, &app.volumes[row]).len(),
    };
    size(lines).saturating_add(2)
}

/// The selected container's name, image's reference or volume's short name
fn title(app: &App, row: usize) -> String {
    match app.view {
        View::Containers => app.containers[row].name.clone(),
        View::Images => app.images[row].reference(),
        View::Volumes => format::short_volume(&app.volumes[row].name).to_string(),
    }
}

/// In a pane at the bottom: the text on the left, the graphs beside it
fn draw_container_wide(buf: &mut Buffer, area: Rect, app: &App, container: &Container) {
    let longest = container_lines(app, container, usize::MAX)
        .iter()
        .take(usize::from(area.height))
        .map(Line::width)
        .max()
        .unwrap_or(0);
    let left = text_width(longest, area.width);
    let lines = container_lines(app, container, usize::from(left));
    draw_lines(
        buf,
        Rect {
            width: left,
            ..area
        },
        &lines,
        &app.theme,
    );
    let graphs = Rect {
        x: area.x + left + GRAPHS_GAP,
        width: area.width.saturating_sub(left + GRAPHS_GAP),
        ..area
    };
    draw_graphs(buf, graphs, app, container);
}

/// In a pane on the right: the text on top, the graphs under it when there's room
fn draw_container_tall(buf: &mut Buffer, area: Rect, app: &App, container: &Container) {
    let lines = container_lines(app, container, usize::from(area.width));
    let graphs = graphs_height(lines.len(), area.height);
    let text_height = area.height - if graphs > 0 { graphs + 1 } else { 0 };
    draw_lines(
        buf,
        Rect {
            height: text_height,
            ..area
        },
        &lines,
        &app.theme,
    );
    if graphs > 0 {
        let graphs_area = Rect {
            y: area.bottom() - graphs,
            height: graphs,
            ..area
        };
        draw_graphs(buf, graphs_area, app, container);
    }
}

/// Half the pane, wider when a line needs it, as long as the graphs keep `MIN_GRAPHS_WIDTH`
fn text_width(longest: usize, width: u16) -> u16 {
    let half = (width / 2).min(MAX_HALF_WIDTH);
    let widest = width
        .saturating_sub(MIN_GRAPHS_WIDTH + GRAPHS_GAP)
        .max(half);
    size(longest).clamp(half, widest)
}

/// What the text leaves, up to `MAX_GRAPHS_HEIGHT`; when that's too little, `CROWDED_GRAPHS_HEIGHT` in a
/// pane tall enough, which cuts the text, else none
fn graphs_height(text_lines: usize, height: u16) -> u16 {
    let text = size(text_lines);
    let graphs = height.saturating_sub(text + 1).min(MAX_GRAPHS_HEIGHT);
    if graphs >= MIN_GRAPHS_HEIGHT {
        graphs
    } else if height >= MIN_CROWDED_HEIGHT {
        CROWDED_GRAPHS_HEIGHT
    } else {
        0
    }
}

/// `label    value` lines, a list's items under each other, `—` for an empty list
struct Lines {
    lines: Vec<Line<'static>>,
    theme: Theme,
}

impl Lines {
    fn new(theme: Theme) -> Self {
        Self {
            lines: Vec::new(),
            theme,
        }
    }

    /// One value
    fn text(&mut self, label: &str, value: impl Into<String>, color: Color) {
        self.add(label, vec![vec![Span::styled(value.into(), fg(color))]]);
    }

    /// One value, no line when it's empty
    fn optional(&mut self, label: &str, value: impl Into<String>, color: Color) {
        let value = value.into();
        if !value.is_empty() {
            self.text(label, value, color);
        }
    }

    /// An item per line
    fn list(
        &mut self,
        label: &str,
        items: impl IntoIterator<Item = impl Into<String>>,
        color: Color,
    ) {
        let items = items
            .into_iter()
            .map(|item| vec![Span::styled(item.into(), fg(color))])
            .collect();
        self.add(label, items);
    }

    /// The containers using an image or volume, the running ones green, each followed by `detail`
    fn users(&mut self, users: &[&Container], detail: impl Fn(&Container) -> String) {
        let theme = self.theme;
        let items = users
            .iter()
            .map(|container| {
                let color = if container.is_up() {
                    theme.green
                } else {
                    theme.dim
                };
                vec![
                    Span::styled(container.name.clone(), fg(color)),
                    Span::styled(detail(container), fg(theme.text)),
                ]
            })
            .collect();
        self.add("used by", items);
    }

    /// A blank line, then that pruning removes it
    fn prune_hint(&mut self) {
        let theme = self.theme;
        self.lines.push(Line::default());
        self.lines.push(Line::from(vec![
            Span::styled("dangling · removed by ", fg(theme.dim)),
            Span::styled(names(&[keys::PRUNE]), bold(theme.accent)),
            Span::styled(" prune", fg(theme.dim)),
        ]));
    }

    fn into_lines(self) -> Vec<Line<'static>> {
        self.lines
    }

    fn add(&mut self, label: &str, items: Vec<Vec<Span<'static>>>) {
        if items.is_empty() {
            self.lines.push(Line::from(vec![
                self.label(label),
                Span::styled(NO_VALUE, fg(self.theme.dim)),
            ]));
        }
        for (i, item) in items.into_iter().enumerate() {
            let mut spans = vec![self.label(if i == 0 { label } else { "" })];
            spans.extend(item);
            self.lines.push(Line::from(spans));
        }
    }

    fn label(&self, label: &str) -> Span<'static> {
        Span::styled(format!("{label:<LABEL_WIDTH$}"), fg(self.theme.muted))
    }
}

/// Mounts fit in `width`
fn container_lines(app: &App, container: &Container, width: usize) -> Vec<Line<'static>> {
    let theme = app.theme;
    let mut lines = Lines::new(theme);
    lines.text("image", &container.image, theme.image);
    let status = app.pending(container).map_or_else(
        || container.status.clone(),
        |pending| pending.label().to_string(),
    );
    lines.text("status", status, state_color(&theme, container));
    if container.is_interactive() {
        let tty = format!("interactive ({}) · no logs", container.interactive_flags());
        lines.text("tty", tty, theme.yellow);
    }
    lines.list("ip", &container.ips, theme.ip);
    lines.text("user", container.shown_user(), theme.user);
    lines.text("workdir", container.shown_workdir(), theme.workdir);
    lines.list("ports", &container.ports, theme.ports);
    // Plain, as the workdir has its own line
    let room = width.saturating_sub(LABEL_WIDTH);
    let mounts = container.mounts.iter().map(|mount| {
        let mount = format::fit_mount(mount, &container.workdir, room);
        mount.prefix + &mount.highlight + &mount.suffix
    });
    lines.list("mounts", mounts, theme.text);
    lines.into_lines()
}

fn image_lines(app: &App, image: &Image) -> Vec<Line<'static>> {
    let theme = app.theme;
    let mut lines = Lines::new(theme);
    lines.text("id", &image.id, theme.id);
    if image.is_foreign(&app.host) {
        let platform = format!("{} · host is {}", image.platform, app.host);
        lines.text("platform", platform, theme.yellow);
    } else {
        lines.optional("platform", &image.platform, theme.text);
    }
    // Every tag when it has more than one, as removing one of them frees nothing
    let tags = tags(app, image);
    if tags.len() > 1 {
        lines.list("tags", tags, theme.image);
    }
    lines.text("size", &image.size, theme.size);
    lines.text("created", &image.created_since, theme.created);
    let users = app.image_users(image);
    lines.users(&users, |container| format!("  {}", container.status));
    if image.is_dangling() && users.is_empty() {
        // What's left of an old build, which pruning removes, so what it runs doesn't matter
        lines.prune_hint();
        return lines.into_lines();
    }
    if image.configured {
        let command = Some(&image.command).filter(|command| !command.is_empty());
        lines.list("command", command, theme.text);
        lines.list("ports", &image.ports, theme.ports);
        lines.text("user", image.shown_user(), theme.user);
        lines.text("workdir", image.shown_workdir(), theme.workdir);
    }
    lines.optional("source", &image.source, theme.text);
    lines.optional("version", version(image), theme.text);
    lines.into_lines()
}

/// The references of every image with its ID, sorted
fn tags(app: &App, image: &Image) -> Vec<String> {
    let mut tags: Vec<String> = app
        .images
        .iter()
        .filter(|other| other.id == image.id && other.is_tagged())
        .map(Image::reference)
        .collect();
    tags.sort();
    tags
}

/// Its version and short revision labels, `version · revision` when it has both
fn version(image: &Image) -> String {
    match (image.version.as_str(), image.revision.as_str()) {
        ("", "") => String::new(),
        (version, "") => version.to_string(),
        ("", revision) => format::short_id(revision).to_string(),
        (version, revision) => format!("{version} · {}", format::short_id(revision)),
    }
}

fn volume_lines(app: &App, volume: &Volume) -> Vec<Line<'static>> {
    let theme = app.theme;
    let mut lines = Lines::new(theme);
    lines.text("name", &volume.name, theme.name);
    lines.text("driver", &volume.driver, theme.driver);
    lines.text("mount", &volume.mountpoint, theme.text);
    lines.text("created", &volume.created, theme.created);
    let users = app.volume_users(volume);
    lines.users(&users, |container| {
        format!(" → {}", mount_destination(container, volume))
    });
    if users.is_empty() {
        lines.prune_hint();
    }
    lines.into_lines()
}

/// Where the container mounts the volume
fn mount_destination<'a>(container: &'a Container, volume: &Volume) -> &'a str {
    container
        .mounts
        .iter()
        .find(|mount| mount.name.as_deref() == Some(&volume.name))
        .map_or("", |mount| mount.destination.as_str())
}

/// As many as fit, the last saying how many more there are, each ending in `…` where it's cut
fn draw_lines(buf: &mut Buffer, area: Rect, lines: &[Line], theme: &Theme) {
    let height = usize::from(area.height);
    let shown = if lines.len() > height {
        height.saturating_sub(1)
    } else {
        lines.len()
    };
    for (line, y) in lines[..shown].iter().zip(area.y..) {
        let line = cut(line, usize::from(area.width));
        put_line(buf, area.x, y, &line, area.right());
    }
    if shown < lines.len() && height > 0 {
        let y = area.y + size(shown);
        let more = format!("{ELLIPSIS} {} more", lines.len() - shown);
        put(buf, area.x, y, &more, fg(theme.dim), area.right());
    }
}

/// `line` in `width` columns, with `…` for what doesn't fit
fn cut<'a>(line: &Line<'a>, width: usize) -> Line<'a> {
    if line.width() <= width {
        return line.clone();
    }
    let mut room = width.saturating_sub(1);
    let mut spans = Vec::new();
    let mut style = line.style;
    for span in &line.spans {
        if room == 0 {
            break;
        }
        let (kept, used) = head(&span.content, room);
        let whole = kept.len() == span.content.len();
        room -= used;
        style = span.style;
        spans.push(Span::styled(kept, span.style));
        // A character too wide for what's left ends it
        if !whole {
            break;
        }
    }
    spans.push(Span::styled(ELLIPSIS.to_string(), style));
    Line::from(spans)
}

/// CPU and memory over time, with network and PIDs under them when there's room
fn draw_graphs(buf: &mut Buffer, area: Rect, app: &App, container: &Container) {
    if area.width < MIN_GRAPH_WIDTH || area.height < MIN_GRAPH_HEIGHT {
        return;
    }
    let theme = app.theme;
    let stats = app.container_stats(container);
    let empty = History::default();
    let history = app.history.get(&container.id).unwrap_or(&empty);
    let width = usize::from(area.width);
    let cpu: Vec<f64> = recent(&history.cpu, width).collect();
    let memory: Vec<f64> = recent(&history.memory, width).map(format::kib).collect();
    // Dim without current stats
    let color = |color: Color| if stats.is_some() { color } else { theme.dim };
    let usage_line = area.height >= MIN_USAGE_LINE_HEIGHT;
    let graph_height = ((area.height - 2 - u16::from(usage_line)) / 2).max(1);

    let peak = cpu.iter().copied().fold(0.0, f64::max);
    let cpu = Graph {
        label: cpu_label(&theme, stats, peak),
        values: &cpu,
        range: cpu_range(peak),
        color: color(theme.cpu),
    };
    let area = labeled_graph(buf, area, graph_height, &cpu, &theme);
    let memory = Graph {
        label: memory_label(&theme, stats),
        values: &memory,
        range: memory_range(&memory),
        color: color(theme.memory),
    };
    let area = labeled_graph(buf, area, graph_height, &memory, &theme);
    if usage_line && area.height > 0 {
        put_line(
            buf,
            area.x,
            area.y,
            &usage_label(&theme, stats),
            area.right(),
        );
    }
}

/// The newest values, as many as the graph has columns
fn recent<T: Copy>(values: &VecDeque<T>, width: usize) -> impl Iterator<Item = T> + '_ {
    values
        .iter()
        .skip(values.len().saturating_sub(width))
        .copied()
}

/// `CPU 18.0%   peak 25.2%`
fn cpu_label(theme: &Theme, stats: Option<&Stats>, peak: f64) -> Line<'static> {
    let now = stats.map_or_else(
        || NO_VALUE.to_string(),
        |stats| format!("{:.1}%", stats.cpu),
    );
    Line::from(vec![
        Span::styled("CPU ", fg(theme.muted)),
        Span::styled(now, bold(theme.cpu)),
        Span::styled(format!("   peak {peak:.1}%"), fg(theme.dim)),
    ])
}

/// `MEM 649M / 2.0G`, colored by how full the memory is
fn memory_label(theme: &Theme, stats: Option<&Stats>) -> Line<'static> {
    let now = stats.map_or_else(|| NO_VALUE.to_string(), |stats| compact_bytes(stats.memory));
    let limit = stats
        .map(|stats| format!(" / {}", compact_bytes(stats.memory_limit)))
        .unwrap_or_default();
    let level = stats.map_or(theme.memory, |stats| {
        theme.level(fraction(stats.memory, stats.memory_limit), theme.memory)
    });
    Line::from(vec![
        Span::styled("MEM ", fg(theme.muted)),
        Span::styled(now, bold(level)),
        Span::styled(limit, fg(theme.dim)),
    ])
}

/// `NET ↓ 75M  ↑ 10M   PIDS 12`
fn usage_label(theme: &Theme, stats: Option<&Stats>) -> Line<'static> {
    let (network, pids) = stats.map_or_else(
        || (NO_VALUE.to_string(), NO_VALUE.to_string()),
        |stats| {
            let (received, sent) = stats.network;
            (
                format!("↓ {}  ↑ {}", compact_bytes(received), compact_bytes(sent)),
                stats.pids.to_string(),
            )
        },
    );
    Line::from(vec![
        Span::styled("NET ", fg(theme.muted)),
        Span::styled(network, fg(theme.text)),
        Span::styled("   PIDS ", fg(theme.muted)),
        Span::styled(pids, fg(theme.text)),
    ])
}

/// From 0 to a little over the peak, at least `MIN_CPU_TOP`
fn cpu_range(peak: f64) -> (f64, f64) {
    (0.0, peak.max(MIN_CPU_TOP) * CPU_HEADROOM)
}

/// From a little under the lowest to a little over the highest, so changes show
fn memory_range(memory: &[f64]) -> (f64, f64) {
    let low = memory.iter().copied().fold(f64::INFINITY, f64::min);
    let low = if low.is_finite() {
        low * MEMORY_FLOOR
    } else {
        0.0
    };
    let high = memory.iter().copied().fold(0.0, f64::max).max(low + 1.0) * MEMORY_HEADROOM;
    (low, high)
}

/// What a graph shows: its label line, its values from the oldest, the range they're drawn in, and its color
struct Graph<'a> {
    label: Line<'static>,
    values: &'a [f64],
    range: (f64, f64),
    color: Color,
}

/// A label over a graph, the area below them
fn labeled_graph(buf: &mut Buffer, area: Rect, height: u16, graph: &Graph, theme: &Theme) -> Rect {
    put_line(buf, area.x, area.y, &graph.label, area.right());
    let graph_area = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        height.min(area.height.saturating_sub(1)),
    );
    draw_bars(buf, graph_area, graph, theme.stripe);
    let used = (height + 1).min(area.height);
    Rect {
        y: area.y + used,
        height: area.height - used,
        ..area
    }
}

/// Bars in eighths of a cell, the newest on the right
fn draw_bars(buf: &mut Buffer, area: Rect, graph: &Graph, background: Color) {
    let (low, high) = graph.range;
    fill(buf, area, graph.color, background);
    let height = f64::from(area.height);
    let per_cell = f64::from(EIGHTHS_PER_CELL);
    let offset = usize::from(area.width).saturating_sub(graph.values.len());
    for (i, &value) in graph.values.iter().enumerate() {
        let x = area.x + size(offset + i);
        if x >= area.right() {
            break;
        }
        let mut eighths = ((value - low) / (high - low)).clamp(0.0, 1.0) * height * per_cell;
        if value > low && eighths < 1.0 {
            eighths = 1.0;
        }
        for row in 0..area.height {
            let left = (eighths - f64::from(row) * per_cell).round();
            if left <= 0.0 {
                break;
            }
            // The most eighths not above what's left, a full cell at most
            let eighths_here = (0..=EIGHTHS_PER_CELL)
                .rev()
                .find(|&n| f64::from(n) <= left)
                .unwrap_or(0);
            buf[(x, area.bottom() - 1 - row)]
                .set_symbol(EIGHTHS[usize::from(eighths_here)])
                .set_style(Style::new().fg(graph.color));
        }
    }
}

#[cfg(test)]
mod tests;
