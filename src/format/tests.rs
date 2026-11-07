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
