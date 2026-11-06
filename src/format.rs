//! How box shows docker's values: mounts, IDs, dates, sizes and log lines

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use crate::docker::Mount;
use crate::paths;

pub const SHORT_ID_LENGTH: usize = 12;
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
    name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
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
    let size = size.trim();
    let split = size
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(size.len());
    let (number, unit) = size.split_at(split);
    let number: f64 = number.parse().ok()?;
    let factor: f64 = match unit.trim() {
        "B" | "" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "PB" => 1e15,
        "KiB" => 1024.0,
        "MiB" => 1024.0_f64.powi(2),
        "GiB" => 1024.0_f64.powi(3),
        "TiB" => 1024.0_f64.powi(4),
        "PiB" => 1024.0_f64.powi(5),
        _ => return None,
    };
    Some((number * factor).round() as u64)
}

/// Both sides of docker's `1.2kB / 648B`
pub fn parse_size_pair(pair: &str) -> Option<(u64, u64)> {
    let (first, second) = pair.split_once('/')?;
    Some((parse_size(first)?, parse_size(second)?))
}

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
            '\u{1b}' => skip_escape(&mut chars),
            '\u{9b}' => skip_until_final_byte(&mut chars),
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
    if clean.len() > max_bytes {
        let mut end = max_bytes;
        while !clean.is_char_boundary(end) {
            end -= 1;
        }
        clean.truncate(end);
    }
    clean
}

/// After an ESC: CSI up to its final byte, OSC and other strings up to their terminator, else one character
fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars>) {
    match chars.next() {
        Some('[') => skip_until_final_byte(chars),
        Some(']' | 'P' | 'X' | '^' | '_') => {
            while let Some(c) = chars.next() {
                if c == '\u{7}' || c == '\u{9c}' {
                    break;
                }
                if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                    chars.next();
                    break;
                }
            }
        }
        _ => {}
    }
}

fn skip_until_final_byte(chars: &mut std::iter::Peekable<std::str::Chars>) {
    for c in chars.by_ref() {
        if ('\u{40}'..='\u{7e}').contains(&c) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(name: &str, destination: &str) -> Mount {
        Mount {
            kind: "volume".into(),
            name: Some(name.into()),
            source: String::new(),
            destination: destination.into(),
            rw: true,
        }
    }

    #[test]
    fn mounts() {
        assert_eq!(
            mount(&volume("html", "/srv"), "/"),
            Highlighted::plain("html:/srv")
        );
        assert_eq!(
            mount(&volume(&"ab".repeat(32), "/cache"), ""),
            Highlighted::plain("abababababab:/cache")
        );
        let tmpfs = Mount {
            kind: "tmpfs".into(),
            destination: "/tmp".into(),
            rw: true,
            ..Mount::default()
        };
        assert_eq!(mount(&tmpfs, "").prefix, "tmpfs:/tmp");
        let bind = Mount {
            kind: "bind".into(),
            source: "/data".into(),
            destination: "/srv".into(),
            ..Mount::default()
        };
        assert_eq!(
            mount(&bind, "/srv"),
            Highlighted {
                prefix: "/data:".into(),
                highlight: "/srv".into(),
                suffix: ":ro".into()
            }
        );
    }

    #[test]
    fn home_shortcut() {
        let home = paths::home().to_string_lossy().into_owned();
        assert_eq!(shorten_home(&format!("{home}/src")), "~/src");
        assert_eq!(shorten_home(&home), "~");
        assert_eq!(
            shorten_home(&format!("{home}x/src")),
            format!("{home}x/src")
        );
        assert_eq!(shorten("/data", ""), "/data");
    }

    #[test]
    fn short_volumes() {
        let anonymous = "a".repeat(64);
        assert_eq!(short_volume(&anonymous), "aaaaaaaaaaaa");
        assert_eq!(
            short_volume("myproject_postgres_data"),
            "myproject_postgres_data"
        );
    }

    #[test]
    fn commands() {
        let args = |args: &[&str]| args.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(
            command(&args(&["nginx", "-g", "daemon off;"])),
            "nginx -g 'daemon off;'"
        );
        assert_eq!(command(&args(&["echo", "it's", ""])), r"echo 'it'\''s' ''");
        assert_eq!(
            command(&args(&["/bin/sh", "-c", "rails s -b 0.0.0.0"])),
            "rails s -b 0.0.0.0"
        );
        assert_eq!(command(&args(&["/bin/sh", "-c"])), "/bin/sh -c");
        assert_eq!(command(&[]), "");
    }

    #[test]
    fn fits_mounts() {
        let mount = Mount {
            kind: "bind".into(),
            source: "/srv/state/sndbx/squid.conf".into(),
            destination: "/etc/squid/squid.conf".into(),
            ..Mount::default()
        };
        let text = |width| {
            let shown = fit_mount(&mount, "/", width);
            shown.prefix + &shown.highlight + &shown.suffix
        };
        assert_eq!(
            text(100),
            "/srv/state/sndbx/squid.conf:/etc/squid/squid.conf:ro"
        );
        // The source gives way, the destination and :ro stay
        assert_eq!(text(40), "/srv/st…id.conf:/etc/squid/squid.conf:ro");
        // Too narrow for even that: as it is, for the line to be cut
        assert_eq!(
            text(30),
            "/srv/state/sndbx/squid.conf:/etc/squid/squid.conf:ro"
        );
        // The working directory stays highlighted
        let shown = fit_mount(&mount, "/etc/squid/squid.conf", 40);
        assert_eq!(shown.highlight, "/etc/squid/squid.conf");
        assert_eq!(shown.prefix.chars().count(), 16);
    }

    #[test]
    fn shortens_in_the_middle() {
        assert_eq!(shorten_middle("abcdefghij", 10), "abcdefghij");
        assert_eq!(shorten_middle("abcdefghij", 7), "abc…hij");
        assert_eq!(shorten_middle("abcdefghij", 6), "abc…ij");
        assert_eq!(shorten_middle("abcdefghij", 1), "…");
    }

    #[test]
    fn short_ids() {
        assert_eq!(
            short_id(&format!("sha256:{}", "a".repeat(64))),
            "aaaaaaaaaaaa"
        );
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn dates() {
        assert_eq!(local_date(""), "");
        assert_eq!(local_date("yesterday"), "yesterday");
        let date = local_date("2026-10-15T09:30:00Z");
        assert_eq!(date.len(), 16, "{date}");
    }

    #[test]
    fn sizes() {
        assert_eq!(compact_bytes(512), "512B");
        assert_eq!(compact_bytes(9_961_472), "9.5M");
        assert_eq!(compact_bytes(325 * 1024 * 1024), "325M");
        assert_eq!(compact_bytes(2 * 1024 * 1024 * 1024), "2.0G");
        assert_eq!(human_size(412_000_000), "412MB");
        assert_eq!(human_size(1_210_000_000), "1.21GB");
        assert_eq!(human_size(48_200_000), "48.2MB");
        assert_eq!(human_size(1_000_000), "1MB");
        assert_eq!(human_size(999), "999B");
        assert_eq!(parse_size("193MB"), Some(193_000_000));
        assert_eq!(parse_size("1.5KiB"), Some(1536));
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("--"), None);
        assert_eq!(parse_size_pair("1.2kB / 648B"), Some((1200, 648)));
    }

    #[test]
    fn sanitizes_log_lines() {
        let clean = |line: &str| sanitize(line.as_bytes(), 4096);
        assert_eq!(clean("plain text\n"), "plain text");
        assert_eq!(clean("\u{1b}[2J\u{1b}[Hcleared"), "cleared");
        assert_eq!(clean("\u{1b}[?1049halternate\u{1b}[?1049l"), "alternate");
        assert_eq!(
            clean("\u{1b}[31mred\u{1b}[0m \u{1b}[10;5Hmoved"),
            "red moved"
        );
        assert_eq!(clean("\u{1b}]0;title\u{7}after"), "after");
        assert_eq!(
            clean("\u{1b}]8;;http://x\u{1b}\\link\u{1b}]8;;\u{1b}\\"),
            "link"
        );
        assert_eq!(clean("10%\r50%\r100% done\r\n"), "100% done");
        assert_eq!(clean("a\tb"), "a       b");
        assert_eq!(clean("bell\u{7}\u{8}\u{0}"), "bell");
        assert_eq!(clean("\u{9b}2Jcsi"), "csi");
        assert_eq!(
            sanitize(b"bad \xff\xfe utf8", 100),
            "bad \u{fffd}\u{fffd} utf8"
        );
        assert_eq!(sanitize("ééé".as_bytes(), 3), "é");
        assert_eq!(sanitize(&[b'x'; 10_000], 4096).len(), 4096);
    }
}
