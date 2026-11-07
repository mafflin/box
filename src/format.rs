//! How box shows docker's values: mounts, IDs, dates, sizes and log lines

use std::cmp::Ordering;
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
/// Shells whose `-c` script is shown as written, like Docker's shell form `CMD rails s`, kept as
/// `/bin/sh -c "rails s"`
const SHELLS: [&str; 4] = ["sh", "/bin/sh", "bash", "/bin/bash"];
const SCRIPT_FLAG: &str = "-c";
/// Besides letters and digits, what a shell takes as plain text, so an argument of only these needs no quotes
const PLAIN: &str = "-_./:=@%+,";
/// A `'` inside single quotes: end them, an escaped `'`, start them again
const QUOTE_IN_QUOTES: &str = r"'\''";
/// Bytes in a kilobyte, as docker counts image sizes and traffic
const KB: u64 = 1000;
/// Bytes in a kibibyte, as top and docker stats count memory
const KIB: u64 = 1024;
/// Below this, `compact_bytes` shows one decimal, like `9.5M`; from it, none, like `310M`, as top does
const ONE_DECIMAL_BELOW: u64 = 10;
/// Docker shows image sizes to this many digits: `412MB`, `48.2MB`, `1.21GB`
const SIZE_DIGITS: usize = 3;

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

/// Arguments as a shell takes them, quoting the ones that need it; a shell's `-c` script as it's written
pub fn command(args: &[String]) -> String {
    if let [shell, flag, script] = args
        && SHELLS.contains(&shell.as_str())
        && flag == SCRIPT_FLAG
    {
        return script.clone();
    }
    args.iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `arg` as a shell reads it back: as it is when it's plain, else in single quotes
fn quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || PLAIN.contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', QUOTE_IN_QUOTES))
    }
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

/// Bytes in KiB as a float, for scales like a graph's: without loss, up to 4 TiB
pub fn kib(bytes: u64) -> f64 {
    f64::from(u32::try_from(bytes / KIB).unwrap_or(u32::MAX))
}

/// Bytes in powers of 1024 the way top shows them: `512B`, `9.5K`, `310M`, `1.9G`
pub fn compact_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    if bytes < KIB {
        return format!("{bytes}B");
    }
    let (scale, unit) = unit_for(bytes, KIB, UNITS.len());
    let bytes = u128::from(bytes);
    if bytes < u128::from(ONE_DECIMAL_BELOW) * scale {
        let tenths = div_round(bytes * 10, scale);
        format!("{}.{}{}", tenths / 10, tenths % 10, UNITS[unit])
    } else {
        format!("{}{}", div_round(bytes, scale), UNITS[unit])
    }
}

/// Bytes the way docker shows image sizes, in powers of 1000 to 3 digits: `412MB`, `1.21GB`
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let (scale, unit) = unit_for(bytes, KB, UNITS.len());
    let bytes = u128::from(bytes);
    let integer_digits = (bytes / scale).to_string().len();
    let decimals = SIZE_DIGITS.saturating_sub(integer_digits);
    let shift = 10_u128.pow(u32::try_from(decimals).unwrap_or(0));
    // A half up, as sizes are usually rounded
    let number = (bytes * shift + scale / 2) / scale;
    let whole = number / shift;
    let fraction = format!("{:0decimals$}", number % shift);
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        format!("{whole}{}", UNITS[unit])
    } else {
        format!("{whole}.{fraction}{}", UNITS[unit])
    }
}

/// The largest power of `base` not above `bytes`, up to the last of `units`, and which one it is
fn unit_for(bytes: u64, base: u64, units: usize) -> (u128, usize) {
    let (bytes, base) = (u128::from(bytes), u128::from(base));
    let mut scale = 1;
    let mut unit = 0;
    while bytes >= scale * base && unit < units - 1 {
        scale *= base;
        unit += 1;
    }
    (scale, unit)
}

/// `numerator / denominator` to the nearest whole number, a half to the even one, as top and `format!` round
fn div_round(numerator: u128, denominator: u128) -> u128 {
    let quotient = numerator / denominator;
    match (numerator % denominator * 2).cmp(&denominator) {
        Ordering::Less => quotient,
        Ordering::Greater => quotient + 1,
        Ordering::Equal => quotient + quotient % 2,
    }
}

/// Bytes from docker's sizes, decimal like `1.2kB` or binary like `3.5MiB`, to the nearest byte
pub fn parse_size(size: &str) -> Option<u64> {
    const UNITS: [(&str, u64); 13] = [
        ("", 1),
        ("B", 1),
        ("kB", KB),
        ("KB", KB),
        ("MB", KB.pow(2)),
        ("GB", KB.pow(3)),
        ("TB", KB.pow(4)),
        ("PB", KB.pow(5)),
        ("KiB", KIB),
        ("MiB", KIB.pow(2)),
        ("GiB", KIB.pow(3)),
        ("TiB", KIB.pow(4)),
        ("PiB", KIB.pow(5)),
    ];
    let size = size.trim();
    let split = size
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(size.len());
    let (number, unit) = size.split_at(split);
    let unit = unit.trim();
    let &(_, factor) = UNITS.iter().find(|(name, _)| *name == unit)?;
    // `1.21` as 121 hundredths, so no float rounds it
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let digits: u128 = format!("{whole}{fraction}").parse().ok()?;
    let scale = 10_u128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let bytes = digits.checked_mul(u128::from(factor))?;
    u64::try_from((bytes + scale / 2) / scale).ok()
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
