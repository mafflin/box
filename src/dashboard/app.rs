//! The dashboard's state and how keys and docker's updates change it; no threads or terminal here

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::commands::PruneTarget;
use crate::docker::{Container, Image, Stats, Volume};
use crate::format;
use crate::theme::Theme;

const HISTORY: usize = 240;
const MAX_LOG_LINES: usize = 5000;
const MAX_LOG_LINE_BYTES: usize = 4096;
/// How far back a line from the other output can go
const LOG_REORDER: usize = 200;
const TOAST_TIME: Duration = Duration::from_millis(3500);
/// How many views there are, for what each keeps
const VIEWS: usize = View::ALL.len();
/// How many things prune can remove
const PRUNE_TARGETS: usize = PruneTarget::ORDER.len();
/// Rows a page moves until the first draw measures the table and the logs
const FIRST_PAGE: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Containers,
    Images,
    Volumes,
}

impl View {
    pub const ALL: [Self; 3] = [Self::Containers, Self::Images, Self::Volumes];

    pub fn index(self) -> usize {
        match self {
            Self::Containers => 0,
            Self::Images => 1,
            Self::Volumes => 2,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Containers => "Containers",
            Self::Images => "Images",
            Self::Volumes => "Volumes",
        }
    }

    fn sort_keys(self) -> &'static [SortKey] {
        match self {
            Self::Containers => &[
                SortKey::Name,
                SortKey::Cpu,
                SortKey::Memory,
                SortKey::Started,
            ],
            Self::Images => &[SortKey::Name, SortKey::Size, SortKey::Created],
            Self::Volumes => &[SortKey::Name, SortKey::Created],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Details {
    Bottom,
    Right,
    Off,
}

impl Details {
    fn next(self) -> Self {
        match self {
            Self::Bottom => Self::Right,
            Self::Right => Self::Off,
            Self::Off => Self::Bottom,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Right => "right",
            Self::Off => "off",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Cpu,
    Memory,
    Size,
    Created,
    /// A container's last start
    Started,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Cpu => "cpu",
            Self::Memory => "mem",
            Self::Size => "size",
            Self::Created => "created",
            Self::Started => "started",
        }
    }

    /// `T` is time: when containers started, when images and volumes were created
    fn from_key(key: char, view: View) -> Option<Self> {
        match key {
            'N' => Some(Self::Name),
            'P' => Some(Self::Cpu),
            'M' => Some(Self::Memory),
            'S' => Some(Self::Size),
            'T' if view == View::Containers => Some(Self::Started),
            'T' => Some(Self::Created),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub descending: bool,
}

/// A container change box asked docker for, shown until docker's next update
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Stopping,
    Starting,
    Restarting,
    Removing,
}

impl Pending {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stopping => "Stopping…",
            Self::Starting => "Starting…",
            Self::Restarting => "Restarting…",
            Self::Removing => "Removing…",
        }
    }
}

#[derive(Default)]
pub struct History {
    pub cpu: VecDeque<f64>,
    pub memory: VecDeque<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    Info,
    Debug,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    /// Local `13:08:42`, empty when docker gave none
    pub time: String,
    pub text: String,
    pub level: Level,
    /// For order, as stdout's and stderr's lines are read apart
    at: Option<jiff::Timestamp>,
}

impl LogLine {
    /// A `docker logs --timestamps` line, made safe to draw
    pub fn parse(raw: &[u8]) -> Self {
        let stamped = raw.iter().position(|&b| b == b' ').and_then(|space| {
            let at = std::str::from_utf8(&raw[..space])
                .ok()?
                .parse::<jiff::Timestamp>()
                .ok()?;
            Some((at, &raw[space + 1..]))
        });
        let (at, text) = match stamped {
            Some((at, text)) => (Some(at), text),
            None => (None, raw),
        };
        let time = at.map_or_else(String::new, |at| {
            at.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%H:%M:%S")
                .to_string()
        });
        let text = format::sanitize(text, MAX_LOG_LINE_BYTES);
        let level = level(&text);
        Self {
            time,
            text,
            level,
            at,
        }
    }
}

/// From words like `ERROR` or `level=warn`
fn level(text: &str) -> Level {
    for word in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '=') {
        let word = word.strip_prefix("level=").unwrap_or(word);
        match word {
            "ERROR" | "FATAL" | "PANIC" | "CRITICAL" | "error" | "fatal" | "panic" => {
                return Level::Error;
            }
            "WARN" | "WARNING" | "warn" | "warning" => return Level::Warning,
            "DEBUG" | "TRACE" | "debug" | "trace" => return Level::Debug,
            _ => {}
        }
    }
    Level::Info
}

pub struct Logs {
    pub session: u64,
    pub id: String,
    pub name: String,
    pub lines: VecDeque<LogLine>,
    /// The line after the last one shown when scrolled, `None` while following
    pub view: Option<usize>,
    pub ended: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Good,
    Warn,
    Bad,
}

pub struct Toast {
    pub text: String,
    pub tone: Tone,
    until: Instant,
}

/// Something docker should do, in the background
#[derive(Clone, Debug, PartialEq)]
pub enum Task {
    Run {
        args: Vec<String>,
        /// The container and how it shows meanwhile
        container: Option<(String, Pending)>,
        done: String,
    },
    Prune(Vec<PruneTarget>),
}

pub struct Confirm {
    pub title: String,
    pub question: String,
    /// The docker command it runs, a container by its short ID
    pub command: String,
    pub danger: bool,
    task: Task,
}

pub struct PruneDialog {
    pub cursor: usize,
    pub chosen: [bool; PRUNE_TARGETS],
}

pub enum Modal {
    Help,
    Confirm(Confirm),
    Prune(PruneDialog),
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    Start(Task),
    OpenLogs {
        session: u64,
        id: String,
    },
    CloseLogs {
        session: u64,
    },
    /// Hand the terminal to a shell in the container until it exits
    Shell {
        id: String,
        name: String,
    },
    Refresh,
    Quit,
}

#[derive(Default, Debug)]
pub struct Snapshot {
    pub containers: Vec<Container>,
    pub images: Vec<Image>,
    pub volumes: Vec<Volume>,
    /// Docker's platform, like `linux/arm64`, empty when unknown
    pub host: String,
}

pub enum Action {
    Key(KeyEvent),
    /// Docker couldn't open a shell in the container, with its message
    ShellFailed {
        name: String,
        message: String,
    },
    Resize,
    Snapshot(Result<Snapshot, String>),
    Stats(Stats),
    Log {
        session: u64,
        line: LogLine,
    },
    LogEnded {
        session: u64,
    },
    Done {
        task: Task,
        result: Result<String, String>,
    },
}

pub struct App {
    pub theme: Theme,
    pub view: View,
    pub containers: Vec<Container>,
    pub images: Vec<Image>,
    pub volumes: Vec<Volume>,
    /// Docker's platform, like `linux/arm64`, empty when unknown
    pub host: String,
    pub stats: HashMap<String, Stats>,
    pub history: HashMap<String, History>,
    /// By container ID, and whether docker's done with it
    pending: HashMap<String, (Pending, bool)>,
    selected: [Option<String>; VIEWS],
    index: [usize; VIEWS],
    pub scroll: [usize; VIEWS],
    /// Rows the table shows, for page up and down
    pub page: usize,
    pub log_page: usize,
    pub sort: [Sort; VIEWS],
    pub show_all: bool,
    pub filter: [String; VIEWS],
    pub filtering: bool,
    pub details: Details,
    pub modal: Option<Modal>,
    pub logs: Option<Logs>,
    pub toast: Option<Toast>,
    /// Why docker can't be reached
    pub disconnected: Option<String>,
    pub loaded: bool,
    next_session: u64,
    now: Instant,
}

impl App {
    pub fn new(theme: Theme, now: Instant) -> Self {
        Self {
            theme,
            view: View::Containers,
            containers: Vec::new(),
            images: Vec::new(),
            volumes: Vec::new(),
            host: String::new(),
            stats: HashMap::new(),
            history: HashMap::new(),
            pending: HashMap::new(),
            selected: Default::default(),
            index: [0; VIEWS],
            scroll: [0; VIEWS],
            page: FIRST_PAGE,
            log_page: FIRST_PAGE,
            sort: [
                Sort {
                    key: SortKey::Started,
                    descending: true,
                },
                Sort {
                    key: SortKey::Name,
                    descending: false,
                },
                Sort {
                    key: SortKey::Name,
                    descending: false,
                },
            ],
            show_all: true,
            filter: Default::default(),
            filtering: false,
            details: Details::Bottom,
            modal: None,
            logs: None,
            toast: None,
            disconnected: None,
            loaded: false,
            next_session: 0,
            now,
        }
    }

    pub fn update(&mut self, action: Action, now: Instant) -> Vec<Effect> {
        self.now = now;
        match action {
            Action::Key(key) => self.key(key),
            Action::Resize => Vec::new(),
            Action::ShellFailed { name, message } => {
                self.shell_failed(&name, &message);
                Vec::new()
            }
            Action::Snapshot(snapshot) => self.snapshot(snapshot),
            Action::Stats(stats) => {
                self.add_stats(stats);
                Vec::new()
            }
            Action::Log { session, line } => {
                self.add_log_line(session, line);
                Vec::new()
            }
            Action::LogEnded { session } => {
                self.log_ended(session);
                Vec::new()
            }
            Action::Done { task, result } => self.task_done(&task, result),
        }
    }

    /// Says why `e` didn't open a shell, shorter when the image has none
    fn shell_failed(&mut self, name: &str, message: &str) {
        let message = if message.contains("executable file not found") {
            format!("no shell in {name}")
        } else {
            format!("can't open a shell in {name}: {message}")
        };
        self.toast(message, Tone::Bad);
    }

    fn log_ended(&mut self, session: u64) {
        if let Some(logs) = self.logs.as_mut().filter(|logs| logs.session == session) {
            logs.ended = true;
        }
    }

    /// Its container is done with, it says how it went, and what docker has now is read again
    fn task_done(&mut self, task: &Task, result: Result<String, String>) -> Vec<Effect> {
        if let Task::Run {
            container: Some((id, _)),
            ..
        } = task
            && let Some(pending) = self.pending.get_mut(id)
        {
            pending.1 = true;
        }
        match result {
            Ok(message) => self.toast(message, Tone::Good),
            Err(message) => self.toast(message, Tone::Bad),
        }
        vec![Effect::Refresh]
    }

    /// Drops an old message, true when it did
    pub fn expire(&mut self, now: Instant) -> bool {
        self.now = now;
        if self.toast.as_ref().is_some_and(|toast| now >= toast.until) {
            self.toast = None;
            return true;
        }
        false
    }

    // ---------------------------------------------------------------- what's shown

    /// The view's rows, filtered and sorted, as indexes into its list
    pub fn rows(&self, view: View) -> Vec<usize> {
        let filter = self.filter[view.index()].to_lowercase();
        let mut rows: Vec<usize> = match view {
            View::Containers => (0..self.containers.len())
                .filter(|&i| self.show_all || self.is_up(&self.containers[i]))
                .collect(),
            View::Images => (0..self.images.len()).collect(),
            View::Volumes => (0..self.volumes.len()).collect(),
        };
        if !filter.is_empty() {
            rows.retain(|&i| self.search_text(view, i).to_lowercase().contains(&filter));
        }
        let sort = self.sort[view.index()];
        rows.sort_by(|&a, &b| {
            let up_first = match view {
                View::Containers => self
                    .is_up(&self.containers[b])
                    .cmp(&self.is_up(&self.containers[a])),
                _ => std::cmp::Ordering::Equal,
            };
            let mut order = self.compare(view, sort.key, a, b);
            if sort.descending {
                order = order.reverse();
            }
            up_first
                .then(order)
                .then_with(|| self.key_of(view, a).cmp(&self.key_of(view, b)))
        });
        rows
    }

    fn compare(&self, view: View, key: SortKey, a: usize, b: usize) -> std::cmp::Ordering {
        match (view, key) {
            (View::Containers, SortKey::Cpu) => {
                let cpu = |i: usize| {
                    self.container_stats(&self.containers[i])
                        .map_or(-1.0, |stats| stats.cpu)
                };
                cpu(a).total_cmp(&cpu(b))
            }
            (View::Containers, SortKey::Memory) => {
                let memory = |i: usize| {
                    self.container_stats(&self.containers[i])
                        .map(|stats| stats.memory)
                };
                memory(a).cmp(&memory(b))
            }
            // When what its status shows happened: up since, or exited since
            (View::Containers, SortKey::Started) => {
                let time = |i: usize| {
                    let container = &self.containers[i];
                    if container.is_up() {
                        container.started_at
                    } else {
                        container.finished_at
                    }
                };
                time(a).cmp(&time(b))
            }
            (View::Images, SortKey::Size) => {
                self.images[a].size_bytes.cmp(&self.images[b].size_bytes)
            }
            (View::Images, SortKey::Created) => {
                self.images[a].created_at.cmp(&self.images[b].created_at)
            }
            (View::Volumes, SortKey::Created) => {
                self.volumes[a].created_at.cmp(&self.volumes[b].created_at)
            }
            _ => self.name_of(view, a).cmp(&self.name_of(view, b)),
        }
    }

    fn name_of(&self, view: View, index: usize) -> String {
        match view {
            View::Containers => self.containers[index].name.clone(),
            View::Images => self.images[index].reference(),
            View::Volumes => self.volumes[index].name.clone(),
        }
    }

    /// What the selection follows when the rows move
    fn key_of(&self, view: View, index: usize) -> String {
        match view {
            View::Containers => self.containers[index].id.clone(),
            View::Images => self.images[index].key(),
            View::Volumes => self.volumes[index].name.clone(),
        }
    }

    fn search_text(&self, view: View, index: usize) -> String {
        match view {
            View::Containers => {
                let container = &self.containers[index];
                format!(
                    "{} {} {}",
                    container.name,
                    container.image,
                    container.ports.join(" ")
                )
            }
            View::Images => {
                let image = &self.images[index];
                // The platform only where it shows
                let platform = if image.is_foreign(&self.host) {
                    image.platform.as_str()
                } else {
                    ""
                };
                format!("{} {} {platform}", image.reference(), image.id)
            }
            View::Volumes => self.volumes[index].name.clone(),
        }
    }

    /// The rows and the selected one's position among them, which follows the item when they move
    pub fn selection(&mut self) -> (Vec<usize>, Option<usize>) {
        let view = self.view;
        let rows = self.rows(view);
        let i = view.index();
        let found = self.selected[i]
            .as_ref()
            .and_then(|key| rows.iter().position(|&row| &self.key_of(view, row) == key));
        let position = found.or_else(|| {
            rows.len()
                .checked_sub(1)
                .map(|last| self.index[i].min(last))
        });
        if let Some(position) = position {
            self.index[i] = position;
            self.selected[i] = Some(self.key_of(view, rows[position]));
        }
        (rows, position)
    }

    pub fn selected_container(&mut self) -> Option<&Container> {
        if self.view != View::Containers {
            return None;
        }
        let (rows, position) = self.selection();
        position.map(|position| &self.containers[rows[position]])
    }

    fn move_selection(&mut self, to: Move) {
        let view = self.view;
        let (rows, position) = self.selection();
        let Some(position) = position else { return };
        let last = rows.len() - 1;
        let target = match to {
            Move::By(delta) => position.saturating_add_signed(delta).min(last),
            Move::Top => 0,
            Move::Bottom => last,
        };
        self.index[view.index()] = target;
        self.selected[view.index()] = Some(self.key_of(view, rows[target]));
    }

    /// Running, paused or restarting, or about to change
    pub fn is_up(&self, container: &Container) -> bool {
        container.is_up() || self.pending.contains_key(&container.id)
    }

    pub fn pending(&self, container: &Container) -> Option<Pending> {
        self.pending.get(&container.id).map(|(pending, _)| *pending)
    }

    /// Only for running containers, a stopped one keeps its last
    pub fn container_stats(&self, container: &Container) -> Option<&Stats> {
        container
            .is_running()
            .then(|| self.stats.get(&container.id))
            .flatten()
    }

    pub fn image_users(&self, image: &Image) -> Vec<&Container> {
        self.containers
            .iter()
            .filter(|container| container.image_id == image.id)
            .collect()
    }

    pub fn volume_users(&self, volume: &Volume) -> Vec<&Container> {
        self.containers
            .iter()
            .filter(|container| {
                container.mounts.iter().any(|mount| {
                    mount.kind == "volume" && mount.name.as_deref() == Some(&volume.name)
                })
            })
            .collect()
    }

    /// Stopped containers, unused volumes and dangling images no container uses, as prune would find them
    pub fn prunable(&self) -> [(usize, u64); PRUNE_TARGETS] {
        let containers = self
            .containers
            .iter()
            .filter(|container| matches!(container.state.as_str(), "exited" | "created"))
            .count();
        let volumes = self
            .volumes
            .iter()
            .filter(|volume| self.volume_users(volume).is_empty())
            .count();
        let dangling: Vec<&Image> = self
            .images
            .iter()
            .filter(|image| image.is_dangling() && self.image_users(image).is_empty())
            .collect();
        let size = dangling.iter().map(|image| image.size_bytes).sum();
        [(containers, 0), (volumes, 0), (dangling.len(), size)]
    }

    fn toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.toast = Some(Toast {
            text: text.into(),
            tone,
            until: self.now + TOAST_TIME,
        });
    }

    // ---------------------------------------------------------------- docker's updates

    fn snapshot(&mut self, snapshot: Result<Snapshot, String>) -> Vec<Effect> {
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.disconnected = Some(error);
                return Vec::new();
            }
        };
        self.disconnected = None;
        self.loaded = true;
        self.containers = snapshot.containers;
        self.images = snapshot.images;
        self.volumes = snapshot.volumes;
        self.host = snapshot.host;
        self.forget_removed_containers();
        self.close_interactive_logs()
    }

    /// The stats, history and pending changes of containers that are gone, and the changes docker's done with
    fn forget_removed_containers(&mut self) {
        let ids: Vec<&String> = self
            .containers
            .iter()
            .map(|container| &container.id)
            .collect();
        self.stats.retain(|id, _| ids.contains(&id));
        self.history.retain(|id, _| ids.contains(&id));
        self.pending
            .retain(|id, (_, done)| !*done && ids.contains(&id));
    }

    /// The logs of a container that's been recreated with `-t` or `-i`, whose output is a terminal session now
    fn close_interactive_logs(&mut self) -> Vec<Effect> {
        let Some(logs) = &self.logs else {
            return Vec::new();
        };
        let Some(container) = self
            .containers
            .iter()
            .find(|container| container.id == logs.id)
            .filter(|container| container.is_interactive())
        else {
            return Vec::new();
        };
        let message = format!(
            "{} is now interactive ({}), logs closed",
            container.name,
            container.interactive_flags()
        );
        let session = logs.session;
        self.logs = None;
        self.toast(message, Tone::Warn);
        vec![Effect::CloseLogs { session }]
    }

    fn add_stats(&mut self, stats: Stats) {
        let history = self.history.entry(stats.id.clone()).or_default();
        history.cpu.push_back(stats.cpu);
        history.memory.push_back(stats.memory);
        if history.cpu.len() > HISTORY {
            history.cpu.pop_front();
            history.memory.pop_front();
        }
        self.stats.insert(stats.id.clone(), stats);
    }

    fn add_log_line(&mut self, session: u64, line: LogLine) {
        let Some(logs) = self.logs.as_mut().filter(|logs| logs.session == session) else {
            return;
        };
        // After the last line that isn't later, looking back a little
        let mut index = logs.lines.len();
        if let Some(at) = line.at {
            while index > 0
                && logs.lines.len() - index < LOG_REORDER
                && logs.lines[index - 1].at.is_some_and(|before| before > at)
            {
                index -= 1;
            }
        }
        logs.lines.insert(index, line);
        if let Some(view) = logs.view.as_mut().filter(|view| index < **view) {
            *view += 1;
        }
        if logs.lines.len() > MAX_LOG_LINES {
            logs.lines.pop_front();
            if let Some(view) = logs.view.as_mut() {
                *view = view.saturating_sub(1).max(1);
            }
        }
    }

    // ---------------------------------------------------------------- keys

    fn key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return vec![Effect::Quit];
        }
        if self.modal.is_some() {
            return self.modal_key(key);
        }
        if self.filtering {
            self.filter_key(key);
            return Vec::new();
        }
        if self.logs.is_some() {
            return self.logs_key(key);
        }
        self.main_key(key)
    }

    fn main_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(Move::By(1)),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(Move::By(-1)),
            KeyCode::PageDown => self.move_selection(Move::By(page(self.page))),
            KeyCode::PageUp => self.move_selection(Move::By(-page(self.page))),
            KeyCode::Home | KeyCode::Char('g') => self.move_selection(Move::Top),
            KeyCode::End | KeyCode::Char('G') => self.move_selection(Move::Bottom),
            KeyCode::Tab | KeyCode::Right => self.switch_view(1),
            // One back, around the end
            KeyCode::BackTab | KeyCode::Left => self.switch_view(VIEWS - 1),
            KeyCode::Char(c @ '1'..='3') => self.view = View::ALL[usize::from(c as u8 - b'1')],
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Esc => self.filter[self.view.index()].clear(),
            KeyCode::Char('a') if self.view == View::Containers => self.toggle_show_all(),
            KeyCode::Char('i') => {
                self.details = self.details.next();
                self.toast(format!("details: {}", self.details.label()), Tone::Info);
            }
            KeyCode::Char('p') => self.open_prune(),
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char('q') => return vec![Effect::Quit],
            KeyCode::Char('s') => return self.start_or_stop(),
            KeyCode::Char('r') => self.restart(),
            KeyCode::Char('d') | KeyCode::Delete => self.remove(),
            KeyCode::Char('l') | KeyCode::Enter => return self.open_logs(),
            KeyCode::Char('e') => return self.open_shell(),
            KeyCode::Char(c) => {
                if let Some(key) = SortKey::from_key(c, self.view) {
                    self.sort_by(key);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// All containers or only the running ones
    fn toggle_show_all(&mut self) {
        self.show_all = !self.show_all;
        let shown = if self.show_all {
            "showing all containers"
        } else {
            "showing running containers"
        };
        self.toast(shown, Tone::Info);
    }

    /// Every target chosen, the cursor on the first
    fn open_prune(&mut self) {
        self.modal = Some(Modal::Prune(PruneDialog {
            cursor: 0,
            chosen: [true; PRUNE_TARGETS],
        }));
    }

    fn switch_view(&mut self, by: usize) {
        self.view = View::ALL[(self.view.index() + by) % View::ALL.len()];
    }

    fn sort_by(&mut self, key: SortKey) {
        let view = self.view;
        if !view.sort_keys().contains(&key) {
            let message = format!(
                "{} can't be sorted by {}",
                view.title().to_lowercase(),
                key.label()
            );
            self.toast(message, Tone::Info);
            return;
        }
        let sort = &mut self.sort[view.index()];
        if sort.key == key {
            sort.descending = !sort.descending;
        } else {
            *sort = Sort {
                key,
                descending: key != SortKey::Name,
            };
        }
    }

    fn start_or_stop(&mut self) -> Vec<Effect> {
        let Some(container) = self.selected_container().cloned() else {
            return Vec::new();
        };
        if self.pending.contains_key(&container.id) {
            return Vec::new();
        }
        let name = &container.name;
        if container.is_up() {
            self.confirm(
                "Stop",
                format!("Stop container {name}?"),
                false,
                run(
                    &["stop", &container.id],
                    Some((&container.id, Pending::Stopping)),
                    format!("stopped {name}"),
                ),
            );
            return Vec::new();
        }
        self.start(run(
            &["start", &container.id],
            Some((&container.id, Pending::Starting)),
            format!("started {name}"),
        ))
    }

    fn restart(&mut self) {
        let Some(container) = self.selected_container().cloned() else {
            return;
        };
        if self.pending.contains_key(&container.id) {
            return;
        }
        let name = &container.name;
        self.confirm(
            "Restart",
            format!("Restart container {name}?"),
            false,
            run(
                &["restart", &container.id],
                Some((&container.id, Pending::Restarting)),
                format!("restarted {name}"),
            ),
        );
    }

    fn remove(&mut self) {
        let (rows, Some(position)) = self.selection() else {
            return;
        };
        let row = rows[position];
        match self.view {
            View::Containers => self.remove_container(row),
            View::Images => self.remove_image(row),
            View::Volumes => self.remove_volume(row),
        }
    }

    /// Forced when it's up, with its anonymous volumes; not while something else is happening to it
    fn remove_container(&mut self, row: usize) {
        let container = self.containers[row].clone();
        if self.pending.contains_key(&container.id) {
            return;
        }
        let name = &container.name;
        let (title, args): (&str, &[&str]) = if container.is_up() {
            ("Force remove", &["rm", "-f", "-v"])
        } else {
            ("Remove", &["rm", "-v"])
        };
        let mut args = args.to_vec();
        args.push(&container.id);
        self.confirm(
            title,
            format!("{title} container {name}?"),
            true,
            run(
                &args,
                Some((&container.id, Pending::Removing)),
                format!("removed {name}"),
            ),
        );
    }

    fn remove_image(&mut self, row: usize) {
        let image = self.images[row].clone();
        if self.in_use(&image.reference(), self.image_users(&image).len()) {
            return;
        }
        let target = image.removal_target();
        self.confirm(
            "Remove image",
            format!("Remove {} ({})?", image.reference(), image.size),
            true,
            run(
                &["rmi", &target],
                None,
                format!("removed image {}", image.reference()),
            ),
        );
    }

    fn remove_volume(&mut self, row: usize) {
        let volume = self.volumes[row].clone();
        let name = format::short_volume(&volume.name).to_string();
        if self.in_use(&name, self.volume_users(&volume).len()) {
            return;
        }
        self.confirm(
            "Remove volume",
            format!("Remove volume {name}?"),
            true,
            run(
                &["volume", "rm", &volume.name],
                None,
                format!("removed volume {name}"),
            ),
        );
    }

    /// Whether containers use what's named, saying so when they do
    fn in_use(&mut self, name: &str, users: usize) -> bool {
        if users > 0 {
            let message = format!("{name} is used by {}", format::plural(users, "container"));
            self.toast(message, Tone::Bad);
        }
        users > 0
    }

    fn confirm(&mut self, title: &str, question: String, danger: bool, task: Task) {
        let Task::Run {
            args, container, ..
        } = &task
        else {
            return;
        };
        // A container by its short ID, as the full one doesn't fit
        let shown: Vec<&str> = args
            .iter()
            .map(|arg| match container {
                Some((id, _)) if arg == id => format::short_id(id),
                _ => arg,
            })
            .collect();
        self.modal = Some(Modal::Confirm(Confirm {
            title: title.to_string(),
            question,
            command: format!("docker {}", shown.join(" ")),
            danger,
            task,
        }));
    }

    fn start(&mut self, task: Task) -> Vec<Effect> {
        if let Task::Run {
            container: Some((id, pending)),
            ..
        } = &task
        {
            self.pending.insert(id.clone(), (*pending, false));
        }
        vec![Effect::Start(task)]
    }

    fn open_shell(&mut self) -> Vec<Effect> {
        let Some(container) = self.selected_container().cloned() else {
            return Vec::new();
        };
        if self.pending.contains_key(&container.id) {
            return Vec::new();
        }
        if !container.is_running() {
            self.toast(format!("{} is not running", container.name), Tone::Warn);
            return Vec::new();
        }
        vec![Effect::Shell {
            id: container.id,
            name: container.name,
        }]
    }

    fn open_logs(&mut self) -> Vec<Effect> {
        let Some(container) = self.selected_container().cloned() else {
            return Vec::new();
        };
        if container.is_interactive() {
            let message = format!(
                "{} is interactive ({}), logs are not shown",
                container.name,
                container.interactive_flags()
            );
            self.toast(message, Tone::Warn);
            return Vec::new();
        }
        self.next_session += 1;
        let session = self.next_session;
        self.logs = Some(Logs {
            session,
            id: container.id.clone(),
            name: container.name.clone(),
            lines: VecDeque::new(),
            view: None,
            ended: false,
        });
        vec![Effect::OpenLogs {
            session,
            id: container.id,
        }]
    }

    fn logs_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let rows = self.log_page.max(1);
        let Some(logs) = self.logs.as_mut() else {
            return Vec::new();
        };
        let length = logs.lines.len();
        let top = rows.min(length);
        let scroll = |view: Option<usize>, by: isize| -> Option<usize> {
            let at = view.unwrap_or(length).saturating_add_signed(by).max(top);
            (at < length).then_some(at)
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'l') => {
                let session = logs.session;
                self.logs = None;
                return vec![Effect::CloseLogs { session }];
            }
            KeyCode::Char(' ') => {
                logs.view = if logs.view.is_none() {
                    Some(length)
                } else {
                    None
                }
            }
            KeyCode::Up | KeyCode::Char('k') => logs.view = scroll(logs.view, -1).or(Some(top)),
            KeyCode::PageUp => logs.view = scroll(logs.view, -page(rows)).or(Some(top)),
            KeyCode::Down | KeyCode::Char('j') if logs.view.is_some() => {
                logs.view = scroll(logs.view, 1);
            }
            KeyCode::PageDown if logs.view.is_some() => logs.view = scroll(logs.view, page(rows)),
            KeyCode::Home | KeyCode::Char('g') => logs.view = Some(top),
            KeyCode::End | KeyCode::Char('G') => logs.view = None,
            _ => {}
        }
        Vec::new()
    }

    fn filter_key(&mut self, key: KeyEvent) {
        let filter = &mut self.filter[self.view.index()];
        match key.code {
            KeyCode::Enter => self.filtering = false,
            KeyCode::Esc => {
                filter.clear();
                self.filtering = false;
            }
            KeyCode::Backspace => {
                filter.pop();
            }
            KeyCode::Down => self.move_selection(Move::By(1)),
            KeyCode::Up => self.move_selection(Move::By(-1)),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => filter.push(c),
            _ => {}
        }
    }

    fn modal_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(modal) = self.modal.take() else {
            return Vec::new();
        };
        match modal {
            Modal::Help => {}
            Modal::Confirm(confirm) => match key.code {
                KeyCode::Char('y' | 'Y') => return self.start(confirm.task),
                KeyCode::Char('n' | 'N' | 'q') | KeyCode::Esc | KeyCode::Enter => {}
                _ => self.modal = Some(Modal::Confirm(confirm)),
            },
            Modal::Prune(dialog) => return self.prune_key(dialog, key),
        }
        Vec::new()
    }

    /// Moves through, toggles, runs or closes the prune dialog; running nothing closes it too
    fn prune_key(&mut self, mut dialog: PruneDialog, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                dialog.cursor = (dialog.cursor + 1) % PRUNE_TARGETS;
            }
            // One back, around the top
            KeyCode::Up | KeyCode::Char('k') => {
                dialog.cursor = (dialog.cursor + PRUNE_TARGETS - 1) % PRUNE_TARGETS;
            }
            KeyCode::Char(' ') => dialog.chosen[dialog.cursor] = !dialog.chosen[dialog.cursor],
            KeyCode::Enter => {
                let targets: Vec<PruneTarget> = PruneTarget::ORDER
                    .into_iter()
                    .zip(dialog.chosen)
                    .filter_map(|(target, chosen)| chosen.then_some(target))
                    .collect();
                if targets.is_empty() {
                    return Vec::new();
                }
                self.toast("pruning…", Tone::Info);
                return vec![Effect::Start(Task::Prune(targets))];
            }
            KeyCode::Esc | KeyCode::Char('q' | 'p') => return Vec::new(),
            _ => {}
        }
        self.modal = Some(Modal::Prune(dialog));
        Vec::new()
    }
}

#[derive(Clone, Copy)]
enum Move {
    By(isize),
    Top,
    Bottom,
}

fn page(rows: usize) -> isize {
    isize::try_from(rows.max(1)).unwrap_or(1)
}

fn run(args: &[&str], container: Option<(&String, Pending)>, done: String) -> Task {
    Task::Run {
        args: args.iter().map(ToString::to_string).collect(),
        container: container.map(|(id, pending)| (id.clone(), pending)),
        done,
    }
}

#[cfg(test)]
mod tests;
