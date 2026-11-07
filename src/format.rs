//! How box shows docker's values: mounts, IDs, dates, sizes and log lines

use std::iter::Peekable;
use std::ops::RangeInclusive;
use std::str::Chars;

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use crate::docker::Mount;
use crate::paths;

pub const SHORT_ID_LENGTH: usize = 12;
/// Hex digits in a full docker ID, which anonymous volumes are named like
const FULL_ID_LENGTH: usize = 64;
const ID_PREFIX: &str = "sha256:";
const READ_ONLY_SUFFIX: &str = ":ro";
const HOME_SHORTCUT: &str = "~";
const DATE_FORMAT: &str = "%Y-%m-%d %H:%M";
/// A mount's source isn't shortened to less, the line is cut instead
const MIN_SOURCE: usize = 8;

/// A text with a part to stand out, empty when nothing does
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Highlighted {
    pub prefix: String,
    pub highlight: String,
    pub suffix: String,
}

impl Highlighted {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            prefix: text.into(),
            ..Self::default()
        }
    }

    pub fn len(&self) -> usize {
        [&self.prefix, &self.highlight, &self.suffix]
            .iter()
            .map(|part| part.chars().count())
            .sum()
    }
}

/// `source:destination[:ro]`, with the destination highlighted when it's the working directory
pub fn mount(mount: &Mount, workdir: &str) -> Highlighted {
    let source = mount_source(mount);
    let read_only = if mount.rw { "" } else { READ_ONLY_SUFFIX };
    if mount.destination != workdir {
        return Highlighted::plain(format!("{source}:{}{read_only}", mount.destination));
    }
    Highlighted {
        prefix: format!("{source}:"),
        highlight: mount.destination.clone(),
        suffix: read_only.to_string(),
    }
}

/// `mount` in at most `width` columns, its source shortened in the middle so the destination and `:ro` stay
/// whole; as it is when even that doesn't fit
pub fn fit_mount(mount: &Mount, workdir: &str, width: usize) -> Highlighted {
    let mut shown = self::mount(mount, workdir);
    let source = mount_source(mount);
    let rest = shown.len() - source.chars().count();
    if shown.len() > width
        && let Some(room) = width.checked_sub(rest).filter(|&room| room >= MIN_SOURCE)
    {
        shown.prefix = shorten_middle(&source, room) + &shown.prefix[source.len()..];
    }
    shown
}

/// A volume's short name, a path with `~` for home, or the kind, like `tmpfs`
fn mount_source(mount: &Mount) -> String {
    let source = mount.name.as_deref().unwrap_or(&mount.source);
    let source = if source.is_empty() {
        &mount.kind
    } else {
        source
    };
    shorten_home(short_volume(source))
}

/// `text` in at most `width` characters, with `…` for what's cut from the middle
pub fn shorten_middle(text: &str, width: usize) -> String {
    let length = text.chars().count();
    if length <= width {
        return text.to_string();
    }
    let kept = width.saturating_sub(1);
    let head: String = text.chars().take(kept - kept / 2).collect();
    let tail: String = text.chars().skip(length - kept / 2).collect();
    format!("{head}…{tail}")
}

/// Docker names anonymous volumes with 64 hex digits
pub fn is_anonymous_volume(name: &str) -> bool {
    name.len() == FULL_ID_LENGTH && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// An anonymous volume's first 12 hex digits, a named one whole
pub fn short_volume(name: &str) -> &str {
    if is_anonymous_volume(name) {
        &name[..SHORT_ID_LENGTH]
    } else {
        name
    }
}

/// Like `1 image` or `2 images`
pub fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// `~/src` for `$HOME/src`
pub fn shorten_home(path: &str) -> String {
    shorten(path, &paths::home().to_string_lossy())
}

/// As it is when there's no home
fn shorten(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("{HOME_SHORTCUT}{rest}")
        }
        _ => path.to_string(),
    }
}

/// Arguments as a shell takes them, quoting the ones that need it; a `sh -c` script as it's written
pub fn command(args: &[String]) -> String {
    if let [shell, flag, script] = args
        && flag == "-c"
        && matches!(shell.as_str(), "sh" | "/bin/sh" | "bash" | "/bin/bash")
    {
        return script.clone();
    }
    args.iter()
        .map(|arg| {
            let safe = !arg.is_empty()
                && arg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c));
            if safe {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The first 12 hex digits, without `sha256:`
pub fn short_id(id: &str) -> &str {
    let id = id.strip_prefix(ID_PREFIX).unwrap_or(id);
    id.get(..SHORT_ID_LENGTH).unwrap_or(id)
}

/// An ISO 8601 time as a local `2026-10-15 09:30`, as given when it isn't one
pub fn local_date(timestamp: &str) -> String {
    let zoned = match timestamp.parse::<Timestamp>() {
        Ok(time) => Some(time.to_zoned(TimeZone::system())),
        // Without an offset it's a local time
        Err(_) if timestamp.contains('T') => timestamp
            .parse::<DateTime>()
            .ok()
            .and_then(|time| time.to_zoned(TimeZone::system()).ok()),
        Err(_) => None,
    };
    zoned.map_or_else(
        || timestamp.to_string(),
        |zoned| zoned.strftime(DATE_FORMAT).to_string(),
    )
}

/// Bytes in powers of 1024 the way top shows them: `512B`, `9.5K`, `310M`, `1.9G`
#[allow(clippy::cast_precision_loss)]
pub fn compact_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    if bytes < 1024 {
        return format!("{bytes}B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.0}{}", UNITS[unit])
    }
}

/// Bytes the way docker shows image sizes, in powers of 1000 to 3 digits: `412MB`, `1.21GB`
#[allow(clippy::cast_precision_loss)]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let decimals = if value >= 100.0 {
        0
    } else if value >= 10.0 {
        1
    } else {
        2
    };
    let number = format!("{value:.decimals$}");
    let number = if number.contains('.') {
        number.trim_end_matches('0').trim_end_matches('.')
    } else {
        &number
    };
    format!("{number}{}", UNITS[unit])
}

/// Bytes from docker's sizes, decimal like `1.2kB` or binary like `3.5MiB`
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn parse_size(size: &str) -> Option<u64> {
    const UNITS: [(&str, u64); 13] = [
        ("", 1),
        ("B", 1),
        ("kB", 1_000),
        ("KB", 1_000),
        ("MB", 1_000_000),
        ("GB", 1_000_000_000),
        ("TB", 1_000_000_000_000),
        ("PB", 1_000_000_000_000_000),
        ("KiB", 1 << 10),
        ("MiB", 1 << 20),
        ("GiB", 1 << 30),
        ("TiB", 1 << 40),
        ("PiB", 1 << 50),
    ];
    let size = size.trim();
    let split = size
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(size.len());
    let (number, unit) = size.split_at(split);
    let number: f64 = number.parse().ok()?;
    let unit = unit.trim();
    let &(_, factor) = UNITS.iter().find(|(name, _)| *name == unit)?;
    Some((number * factor as f64).round() as u64)
}

/// Both sides of docker's `1.2kB / 648B`
pub fn parse_size_pair(pair: &str) -> Option<(u64, u64)> {
    let (first, second) = pair.split_once('/')?;
    Some((parse_size(first)?, parse_size(second)?))
}

/// Escape, which starts every 7-bit escape sequence
const ESC: char = '\u{1b}';
/// Control Sequence Introducer, the 8-bit form of `ESC [`
const CSI: char = '\u{9b}';
/// Bell, which xterm also accepts as a String Terminator
const BEL: char = '\u{7}';
/// String Terminator, the 8-bit form of `ESC \`
const ST: char = '\u{9c}';
/// What follows ESC to start a string that runs until a String Terminator: OSC, DCS, SOS, PM, APC
const STRING_STARTS: [char; 5] = [']', 'P', 'X', '^', '_'];
/// The byte range that ends a control sequence (`@` through `~`)
const FINAL_BYTES: RangeInclusive<char> = '@'..='~';

/// A line of a container's output as plain text that can't change the terminal: only what a carriage return
/// leaves, without escape sequences and control characters, tabs expanded, at most `max_bytes`
pub fn sanitize(line: &[u8], max_bytes: usize) -> String {
    const TAB_WIDTH: usize = 8;
    let line = String::from_utf8_lossy(line);
    let line = line.trim_end_matches(['\r', '\n']);
    let line = line.rsplit_once('\r').map_or(line, |(_, last)| last);

    let mut clean = String::with_capacity(line.len().min(max_bytes));
    let mut column = 0;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ESC => skip_escape(&mut chars),
            CSI => skip_until_final_byte(&mut chars),
            '\t' => {
                let spaces = TAB_WIDTH - column % TAB_WIDTH;
                clean.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            c if c.is_control() => {}
            c => {
                clean.push(c);
                column += 1;
            }
        }
        if clean.len() >= max_bytes {
            break;
        }
    }
    clean.truncate(clean.floor_char_boundary(max_bytes));
    clean
}

/// After an ESC: CSI up to its final byte, OSC and other strings up to their terminator, else one character
fn skip_escape(chars: &mut Peekable<Chars>) {
    match chars.next() {
        // CSI
        Some('[') => skip_until_final_byte(chars),
        Some(c) if STRING_STARTS.contains(&c) => {
            while let Some(c) = chars.next() {
                if c == BEL || c == ST {
                    break;
                }
                if c == ESC && chars.peek() == Some(&'\\') {
                    chars.next();
                    break;
                }
            }
        }
        _ => {}
    }
}

fn skip_until_final_byte(chars: &mut Peekable<Chars>) {
    for c in chars.by_ref() {
        if FINAL_BYTES.contains(&c) {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
