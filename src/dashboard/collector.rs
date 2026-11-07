//! The threads that ask docker for what the dashboard shows and run what it asks for

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::app::{Action, LogLine, Snapshot, Task};
use crate::commands::prune;
use crate::docker::streams::{self, Children};
use crate::docker::{self, Captured, Docker};

/// How often to look again when nothing happened
const REFRESH: Duration = Duration::from_secs(5);
/// Events often come in bursts, like a compose project starting
const SETTLE: Duration = Duration::from_millis(150);
const MIN_INTERVAL: Duration = Duration::from_secs(1);
const LOG_TAIL: &str = "500";

pub struct Collector {
    actions: Sender<Action>,
    refresh: Sender<()>,
    children: Arc<Children>,
    running: Arc<AtomicBool>,
    /// The docker logs process of each log session
    logs: HashMap<u64, u64>,
}

impl Collector {
    /// Starts reading containers, images, volumes, stats and events
    pub fn start(actions: Sender<Action>) -> Self {
        let (refresh, requests) = mpsc::channel();
        let children = Arc::new(Children::default());
        let running = Arc::new(AtomicBool::new(true));

        let snapshots = actions.clone();
        thread::spawn(move || refresher(&requests, &snapshots));

        let stats = actions.clone();
        streams::follow(
            Arc::clone(&children),
            args(&["stats", "--no-trunc", "--format", "{{json .}}"]),
            Arc::clone(&running),
            move |line| match docker::stats(&String::from_utf8_lossy(line)) {
                Some(update) => stats.send(Action::Stats(update)).is_ok(),
                None => true,
            },
            || {},
        );

        let on_event = refresh.clone();
        let on_end = refresh.clone();
        streams::follow(
            Arc::clone(&children),
            args(&["events", "--format", "{{json .}}"]),
            Arc::clone(&running),
            move |line| {
                !docker::is_change(&String::from_utf8_lossy(line)) || on_event.send(()).is_ok()
            },
            // The daemon may be gone, which the next look shows
            move || {
                let _ = on_end.send(());
            },
        );

        Self {
            actions,
            refresh,
            children,
            running,
            logs: HashMap::new(),
        }
    }

    pub fn refresh(&self) {
        let _ = self.refresh.send(());
    }

    pub fn start_task(&self, task: Task) {
        let actions = self.actions.clone();
        thread::spawn(move || {
            let result = run(&task).map_err(|e| format!("{e:#}"));
            let _ = actions.send(Action::Done { task, result });
        });
    }

    pub fn open_logs(&mut self, session: u64, id: &str) {
        let lines = self.actions.clone();
        let ended = self.actions.clone();
        let started = streams::run_once(
            &self.children,
            &args(&["logs", "--follow", "--timestamps", "--tail", LOG_TAIL, id]),
            move |line| {
                lines
                    .send(Action::Log {
                        session,
                        line: LogLine::parse(line),
                    })
                    .is_ok()
            },
            move || {
                let _ = ended.send(Action::LogEnded { session });
            },
        );
        match started {
            Ok(key) => {
                self.logs.insert(session, key);
            }
            Err(e) => {
                let _ = self.actions.send(Action::Log {
                    session,
                    line: LogLine::parse(format!("can't run docker logs: {e}").as_bytes()),
                });
                let _ = self.actions.send(Action::LogEnded { session });
            }
        }
    }

    pub fn close_logs(&mut self, session: u64) {
        if let Some(key) = self.logs.remove(&session) {
            self.children.kill(key);
        }
    }

    /// Ends every docker process it started
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.children.kill_all();
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.stop();
    }
}

fn args(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

/// Looks now, then every few seconds, or soon after a request
fn refresher(requests: &Receiver<()>, snapshots: &Sender<Action>) {
    let mut last = Instant::now();
    let mut host = String::new();
    loop {
        if host.is_empty() {
            host = host_platform().unwrap_or_default();
        }
        let snapshot = snapshot(&host);
        // Docker may come back on another platform, like Colima restarted for x86-64
        if snapshot.is_err() {
            host.clear();
        }
        if snapshots.send(Action::Snapshot(snapshot)).is_err() {
            return;
        }
        last = last.max(Instant::now());
        match requests.recv_timeout(REFRESH) {
            Ok(()) => {
                thread::sleep(SETTLE.max(MIN_INTERVAL.saturating_sub(last.elapsed())));
                while requests.try_recv().is_ok() {}
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        last = Instant::now();
    }
}

/// Docker's OS and CPU, like `linux/arm64`, which don't change while it runs
fn host_platform() -> Option<String> {
    let lines = Captured
        .lines(&["version", "--format", "{{.Server.Os}}/{{.Server.Arch}}"])
        .ok()?;
    let platform = lines.first()?.trim();
    let known = platform
        .split_once('/')
        .is_some_and(|(os, arch)| !os.is_empty() && !arch.is_empty());
    known.then(|| platform.to_string())
}

/// Containers, images and their details, and volumes
fn snapshot(host: &str) -> Result<Snapshot, String> {
    let read = || -> crate::Result<Snapshot> {
        let ps = Captured.lines(&["ps", "--all", "--no-trunc", "--format", "{{json .}}"])?;
        let inspect = docker::inspect_all(&["inspect"], &docker::ps_ids(&ps))?;
        let images = Captured.lines(&["image", "ls", "--no-trunc", "--format", "{{json .}}"])?;
        // Only details, the images show without them
        let image_inspect = docker::inspect_all(&["image", "inspect"], &docker::image_ids(&images))
            .unwrap_or_default();
        let names = Captured.lines(&["volume", "ls", "-q"])?;
        let volumes = docker::inspect_all(&["volume", "inspect"], &names)?;
        Ok(Snapshot {
            containers: docker::containers(&ps, &inspect)?,
            images: docker::images(&images, &image_inspect)?,
            volumes: docker::volumes(&volumes)?,
            host: host.to_string(),
        })
    };
    read().map_err(|e| format!("{e:#}"))
}

/// The task's message when it worked
fn run(task: &Task) -> crate::Result<String> {
    match task {
        Task::Run { args, done, .. } => {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            Captured.run(&args)?;
            Ok(done.clone())
        }
        Task::Prune(targets) => {
            let removed: Vec<String> = prune(&Captured, targets)?
                .into_iter()
                .filter(|&(_, count)| count > 0)
                .map(|(target, count)| target.count(count))
                .collect();
            Ok(if removed.is_empty() {
                "nothing to prune".to_string()
            } else {
                format!("pruned {}", removed.join(", "))
            })
        }
    }
}
