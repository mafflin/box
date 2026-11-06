//! The selected item's details, with a container's CPU and memory over time

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::table::{fraction, state_color};
use super::{bold, fg, fill, put, put_line, rounded_box};
use crate::colors;
use crate::dashboard::app::{App, History, View};
use crate::dashboard::theme::Theme;
use crate::docker::{Container, Image, Volume};
use crate::format::{self, compact_bytes};

const LABEL_WIDTH: usize = 9;
const EIGHTHS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App, bottom: bool) {
    let theme = app.theme;
    let (rows, selected) = app.selection();
    let row = selected.map(|position| rows[position]);
    let title = row.map_or_else(String::new, |row| match app.view {
        View::Containers => app.containers[row].name.clone(),
        View::Images => app.images[row].reference(),
        View::Volumes => format::short_volume(&app.volumes[row].name).to_string(),
    });
    let inner = rounded_box(buf, area, theme.border, &title, fg(theme.text));
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    if inner.width < 4 || inner.height == 0 {
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
        View::Containers => {
            let container = &app.containers[row];
            let lines = container_lines(app, container);
            if bottom {
                let left = (inner.width / 2).min(64);
                draw_lines(
                    buf,
                    Rect {
                        width: left,
                        ..inner
                    },
                    &lines,
                    &theme,
                );
                let graphs = Rect {
                    x: inner.x + left + 3,
                    width: inner.width.saturating_sub(left + 3),
                    ..inner
                };
                draw_graphs(buf, graphs, app, container);
            } else {
                let lines_height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
                let mut graphs = inner.height.saturating_sub(lines_height + 1).min(11);
                if graphs < 5 {
                    graphs = if inner.height >= 14 { 7 } else { 0 };
                }
                let text_height = inner.height - if graphs > 0 { graphs + 1 } else { 0 };
                draw_lines(
                    buf,
                    Rect {
                        height: text_height,
                        ..inner
                    },
                    &lines,
                    &theme,
                );
                if graphs > 0 {
                    let area = Rect {
                        y: inner.bottom() - graphs,
                        height: graphs,
                        ..inner
                    };
                    draw_graphs(buf, area, app, container);
                }
            }
        }
        View::Images => draw_lines(buf, inner, &image_lines(app, &app.images[row]), &theme),
        View::Volumes => draw_lines(buf, inner, &volume_lines(app, &app.volumes[row]), &theme),
    }
}

/// `label    value` lines, a list's items under each other
struct Lines {
    lines: Vec<Line<'static>>,
    label: Style,
}

impl Lines {
    fn new(theme: &Theme) -> Self {
        Self {
            lines: Vec::new(),
            label: fg(theme.muted),
        }
    }

    fn add(&mut self, label: &str, values: Vec<Vec<Span<'static>>>, empty: Style) {
        if values.is_empty() {
            self.lines.push(Line::from(vec![
                self.label(label),
                Span::styled("—", empty),
            ]));
        }
        for (i, value) in values.into_iter().enumerate() {
            let mut spans = vec![self.label(if i == 0 { label } else { "" })];
            spans.extend(value);
            self.lines.push(Line::from(spans));
        }
    }

    fn text(&mut self, label: &str, value: impl Into<String>, style: Style, empty: Style) {
        self.add(label, vec![vec![Span::styled(value.into(), style)]], empty);
    }

    fn label(&self, label: &str) -> Span<'static> {
        Span::styled(format!("{label:<LABEL_WIDTH$}"), self.label)
    }
}

fn container_lines(app: &App, container: &Container) -> Vec<Line<'static>> {
    let theme = app.theme;
    let value = fg(theme.text);
    let empty = fg(theme.dim);
    let mut lines = Lines::new(&theme);
    lines.text("image", &container.image, fg(colors::IMAGE), empty);
    let status = app.pending(container).map_or_else(
        || container.status.clone(),
        |pending| pending.label().to_string(),
    );
    lines.text("status", status, fg(state_color(&theme, container)), empty);
    if container.is_interactive() {
        let tty = format!("interactive ({}) · no logs", container.interactive_flags());
        lines.text("tty", tty, fg(theme.yellow), empty);
    }
    let list = |items: &[String], color| {
        items
            .iter()
            .map(|item| vec![Span::styled(item.clone(), fg(color))])
            .collect()
    };
    lines.add("ip", list(&container.ips, colors::IP), empty);
    lines.text("user", container.shown_user(), fg(colors::USER), empty);
    let workdir = container.shown_workdir();
    lines.text("workdir", workdir, fg(colors::WORKDIR), empty);
    lines.add("ports", list(&container.ports, colors::PORTS), empty);
    // Plain, as the workdir has its own line
    let mounts = container
        .mounts
        .iter()
        .map(|mount| {
            let mount = format::mount(mount, &container.workdir);
            let text = mount.prefix + &mount.highlight + &mount.suffix;
            vec![Span::styled(text, value)]
        })
        .collect();
    lines.add("mounts", mounts, empty);
    lines.lines
}

fn users(
    app: &App,
    users: &[&Container],
    suffix: impl Fn(&Container) -> String,
) -> Vec<Vec<Span<'static>>> {
    let theme = app.theme;
    users
        .iter()
        .map(|container| {
            let color = if container.is_up() {
                theme.green
            } else {
                theme.dim
            };
            vec![
                Span::styled(container.name.clone(), fg(color)),
                Span::styled(suffix(container), fg(theme.text)),
            ]
        })
        .collect()
}

fn image_lines(app: &App, image: &Image) -> Vec<Line<'static>> {
    let theme = app.theme;
    let empty = fg(theme.dim);
    let mut lines = Lines::new(&theme);
    lines.text("id", &image.id, fg(colors::ID), empty);
    lines.text("size", &image.size, fg(colors::SIZE), empty);
    lines.text("created", &image.created_since, fg(colors::CREATED), empty);
    let used_by = users(app, &app.image_users(image), |container| {
        format!("  {}", container.status)
    });
    let unused = used_by.is_empty();
    lines.add("used by", used_by, empty);
    if image.is_dangling() && unused {
        lines.lines.push(Line::default());
        lines.lines.push(prune_hint(&theme));
    }
    lines.lines
}

fn volume_lines(app: &App, volume: &Volume) -> Vec<Line<'static>> {
    let theme = app.theme;
    let (value, empty) = (fg(theme.text), fg(theme.dim));
    let mut lines = Lines::new(&theme);
    lines.text("name", &volume.name, fg(colors::NAME), empty);
    lines.text("driver", &volume.driver, fg(colors::DRIVER), empty);
    lines.text("mount", &volume.mountpoint, value, empty);
    lines.text("created", &volume.created, fg(colors::CREATED), empty);
    let users_list = app.volume_users(volume);
    let used_by = users(app, &users_list, |container| {
        let destination = container
            .mounts
            .iter()
            .find(|mount| mount.name.as_deref() == Some(&volume.name))
            .map_or("", |mount| mount.destination.as_str());
        format!(" → {destination}")
    });
    let unused = used_by.is_empty();
    lines.add("used by", used_by, empty);
    if unused {
        lines.lines.push(Line::default());
        lines.lines.push(prune_hint(&theme));
    }
    lines.lines
}

fn prune_hint(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("dangling · removed by ", fg(theme.dim)),
        Span::styled("p", bold(theme.accent)),
        Span::styled(" prune", fg(theme.dim)),
    ])
}

/// As many as fit, the last saying how many more there are
fn draw_lines(buf: &mut Buffer, area: Rect, lines: &[Line], theme: &Theme) {
    let height = usize::from(area.height);
    for (i, line) in lines.iter().take(height).enumerate() {
        let y = area.y + u16::try_from(i).unwrap_or(0);
        if i + 1 == height && lines.len() > height {
            put(
                buf,
                area.x,
                y,
                &format!("… {} more", lines.len() - i),
                fg(theme.dim),
                area.right(),
            );
            break;
        }
        put_line(buf, area.x, y, line, area.right());
    }
}

fn draw_graphs(buf: &mut Buffer, area: Rect, app: &App, container: &Container) {
    let theme = app.theme;
    if area.width < 10 || area.height < 2 {
        return;
    }
    let stats = app.container_stats(container);
    let empty = History::default();
    let history = app.history.get(&container.id).unwrap_or(&empty);
    let width = usize::from(area.width);
    let cpu: Vec<f64> = history
        .cpu
        .iter()
        .skip(history.cpu.len().saturating_sub(width))
        .copied()
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let memory: Vec<f64> = history
        .memory
        .iter()
        .skip(history.memory.len().saturating_sub(width))
        .map(|&bytes| bytes as f64)
        .collect();
    let color = |color: Color| if stats.is_some() { color } else { theme.dim };
    let usage_line = area.height >= 6;
    let height = ((area.height - 2 - u16::from(usage_line)) / 2).max(1);
    let mut area = area;

    let peak = cpu.iter().copied().fold(0.0, f64::max);
    let now = stats.map_or_else(|| "—".to_string(), |stats| format!("{:.1}%", stats.cpu));
    let label = Line::from(vec![
        Span::styled("CPU ", fg(theme.muted)),
        Span::styled(now, bold(theme.green)),
        Span::styled(format!("   peak {peak:.1}%"), fg(theme.dim)),
    ]);
    let range = (0.0, peak.max(5.0) * 1.15);
    area = labeled_graph(
        buf,
        area,
        height,
        &label,
        &cpu,
        range,
        color(theme.green),
        &theme,
    );

    let now = stats.map_or_else(|| "—".to_string(), |stats| compact_bytes(stats.memory));
    let limit = stats
        .map(|stats| format!(" / {}", compact_bytes(stats.memory_limit)))
        .unwrap_or_default();
    let level = stats.map_or(theme.blue, |stats| {
        match theme.level(fraction(stats.memory, stats.memory_limit)) {
            level if level == theme.text => theme.blue,
            level => level,
        }
    });
    let label = Line::from(vec![
        Span::styled("MEM ", fg(theme.muted)),
        Span::styled(now, bold(level)),
        Span::styled(limit, fg(theme.dim)),
    ]);
    // From a little under the lowest, so changes show
    let low = memory.iter().copied().fold(f64::INFINITY, f64::min);
    let low = if low.is_finite() { low * 0.9 } else { 0.0 };
    let high = memory.iter().copied().fold(0.0, f64::max).max(low + 1.0) * 1.05;
    area = labeled_graph(
        buf,
        area,
        height,
        &label,
        &memory,
        (low, high),
        color(theme.blue),
        &theme,
    );

    if usage_line && area.height > 0 {
        let (network, pids) = stats.map_or_else(
            || ("—".to_string(), "—".to_string()),
            |stats| {
                let (received, sent) = stats.network;
                (
                    format!("↓ {}  ↑ {}", compact_bytes(received), compact_bytes(sent)),
                    stats.pids.to_string(),
                )
            },
        );
        let line = Line::from(vec![
            Span::styled("NET ", fg(theme.muted)),
            Span::styled(network, fg(theme.text)),
            Span::styled("   PIDS ", fg(theme.muted)),
            Span::styled(pids, fg(theme.text)),
        ]);
        put_line(buf, area.x, area.y, &line, area.right());
    }
}

/// A label over a graph, the area below them
#[allow(clippy::too_many_arguments)]
fn labeled_graph(
    buf: &mut Buffer,
    area: Rect,
    height: u16,
    label: &Line,
    values: &[f64],
    (low, high): (f64, f64),
    color: Color,
    theme: &Theme,
) -> Rect {
    put_line(buf, area.x, area.y, label, area.right());
    let graph_area = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        height.min(area.height.saturating_sub(1)),
    );
    graph(buf, graph_area, values, low, high, color, theme.bg_alt);
    let used = (height + 1).min(area.height);
    Rect {
        y: area.y + used,
        height: area.height - used,
        ..area
    }
}

/// Bars in eighths of a cell, the newest on the right
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn graph(
    buf: &mut Buffer,
    area: Rect,
    values: &[f64],
    low: f64,
    high: f64,
    color: Color,
    background: Color,
) {
    fill(buf, area, color, background);
    let height = f64::from(area.height);
    let offset = usize::from(area.width).saturating_sub(values.len());
    for (i, &value) in values.iter().enumerate() {
        let x = area.x + u16::try_from(offset + i).unwrap_or(u16::MAX);
        if x >= area.right() {
            break;
        }
        let mut eighths = ((value - low) / (high - low)).clamp(0.0, 1.0) * height * 8.0;
        if value > low && eighths < 1.0 {
            eighths = 1.0;
        }
        for row in 0..area.height {
            let left = (eighths - f64::from(row) * 8.0).round();
            if left <= 0.0 {
                break;
            }
            let symbol = EIGHTHS[(left as usize).min(8)];
            buf[(x, area.bottom() - 1 - row)]
                .set_symbol(symbol)
                .set_style(Style::new().fg(color));
        }
    }
}
