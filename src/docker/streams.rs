//! Long-running docker commands the dashboard follows, like `docker stats`, killed when it's done with them

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use super::run::command;

const RESTART_DELAY: Duration = Duration::from_secs(2);

/// The docker processes still running, by key
#[derive(Default)]
pub struct Children {
    children: Mutex<HashMap<u64, Child>>,
    next: AtomicU64,
}

impl Children {
    fn add(&self, child: Child) -> u64 {
        let key = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock().insert(key, child);
        key
    }

    fn take(&self, key: u64) -> Option<Child> {
        self.lock().remove(&key)
    }

    /// Ends the process, which closes its output, so whoever reads it stops
    pub fn kill(&self, key: u64) {
        if let Some(mut child) = self.take(key) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn kill_all(&self) {
        let children: Vec<Child> = self.lock().drain().map(|(_, child)| child).collect();
        for mut child in children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Waits for a process that ended by itself
    fn reap(&self, key: u64) {
        if let Some(mut child) = self.take(key) {
            let _ = child.wait();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Child>> {
        self.children.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Calls `on_line` with each line, without the newline, until the end or it returns false
pub fn read_lines(reader: impl Read, mut on_line: impl FnMut(&[u8]) -> bool) {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if !on_line(&line) {
                    return;
                }
            }
        }
    }
}

/// Runs docker with `args` in the background, again a little after each time it ends, until `running` is false or
/// `on_line` returns false; `on_end` hears of each end
pub fn follow(
    children: Arc<Children>,
    args: Vec<String>,
    running: Arc<AtomicBool>,
    mut on_line: impl FnMut(&[u8]) -> bool + Send + 'static,
    mut on_end: impl FnMut() + Send + 'static,
) {
    thread::spawn(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        while running.load(Ordering::Relaxed) {
            let mut wanted = true;
            let spawned = command(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = spawned {
                let stdout = child.stdout.take();
                let key = children.add(child);
                if let Some(stdout) = stdout {
                    read_lines(stdout, |line| {
                        wanted = on_line(line);
                        wanted
                    });
                }
                children.reap(key);
            }
            if !wanted || !running.load(Ordering::Relaxed) {
                return;
            }
            on_end();
            thread::sleep(RESTART_DELAY);
        }
    });
}

/// Runs docker with `args` once, reading both its outputs; `on_end` comes after both end. The key kills it.
pub fn run_once(
    children: &Arc<Children>,
    args: &[String],
    on_line: impl Fn(&[u8]) -> bool + Send + Sync + 'static,
    on_end: impl FnOnce() + Send + 'static,
) -> io::Result<u64> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut child = command(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let key = children.add(child);

    let on_line = Arc::new(on_line);
    let on_error_line = Arc::clone(&on_line);
    let errors = thread::spawn(move || {
        if let Some(stderr) = stderr {
            read_lines(stderr, |line| on_error_line(line));
        }
    });
    let children = Arc::clone(children);
    thread::spawn(move || {
        if let Some(stdout) = stdout {
            read_lines(stdout, |line| on_line(line));
        }
        let _ = errors.join();
        children.reap(key);
        on_end();
    });
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lines_until_told_to_stop() {
        let mut seen = Vec::new();
        read_lines(&b"one\ntwo\r\nthree"[..], |line| {
            seen.push(String::from_utf8_lossy(line).into_owned());
            true
        });
        assert_eq!(seen, ["one", "two\r", "three"]);

        let mut count = 0;
        read_lines(&b"a\nb\nc\n"[..], |_| {
            count += 1;
            count < 2
        });
        assert_eq!(count, 2);
    }
}
