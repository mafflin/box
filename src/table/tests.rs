use super::*;

#[test]
fn renders_cells_over_lines() {
    let columns = [
        Column::left("NAME", |theme| theme.name),
        Column::right("SIZE", |_| Color::Blue),
        Column::left("PORTS", |_| Color::Green),
    ];
    let rows = vec![
        vec![cell("web"), cell("1MB"), lines(["80/tcp", "443/tcp"])],
        vec![cell("database"), cell("12MB"), lines(Vec::<String>::new())],
    ];
    let plain = render(&columns, &rows, &Theme::default())
        .replace("\u{1b}[34m", "")
        .replace("\u{1b}[32m", "")
        .replace("\u{1b}[1m", "")
        .replace("\u{1b}[0m", "");
    assert_eq!(
        plain,
        "NAME      SIZE  PORTS\nweb        1MB  80/tcp\n                443/tcp\ndatabase  12MB\n"
    );
}

#[test]
fn highlights_the_workdir_in_a_cell() {
    let columns = [Column::left("MOUNTS", |_| Color::Blue)];
    let theme = Theme {
        workdir: Color::Magenta,
        ..Theme::default()
    };
    let highlighted = Highlighted {
        prefix: "/data:".into(),
        highlight: "/srv".into(),
        suffix: ":ro".into(),
    };
    let table = render(&columns, &[vec![vec![highlighted]]], &theme);
    assert!(
        table.ends_with("\u{1b}[34m/data:\u{1b}[0m\u{1b}[35m/srv\u{1b}[0m\u{1b}[34m:ro\u{1b}[0m\n")
    );
    assert_eq!(render(&columns, &[], &theme), "");
}

#[test]
fn sgr_codes() {
    assert_eq!(sgr(Color::Reset), None);
    assert_eq!(sgr(Color::Cyan).as_deref(), Some("36"));
    assert_eq!(sgr(Color::DarkGray).as_deref(), Some("90"));
    assert_eq!(sgr(Color::White).as_deref(), Some("97"));
    assert_eq!(sgr(Color::Indexed(239)).as_deref(), Some("38;5;239"));
    assert_eq!(sgr(Color::Rgb(1, 2, 3)).as_deref(), Some("38;2;1;2;3"));
}
