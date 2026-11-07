use super::*;

fn error(content: &str) -> String {
    parse(content).err().unwrap().to_string()
}

#[test]
fn example_lists_the_defaults() {
    assert_eq!(parse(EXAMPLE).unwrap(), Settings::default());
    // Its commented-out theme, uncommented
    let start = EXAMPLE.find("# [theme]").unwrap();
    let theme: Vec<&str> = EXAMPLE[start..]
        .lines()
        .map(|line| line.strip_prefix("# ").unwrap_or(line))
        .collect();
    assert!(theme.iter().any(|line| line.starts_with("memory = ")));
    assert_eq!(parse(&theme.join("\n")).unwrap(), Settings::default());
}

#[test]
fn empty_settings() {
    for empty in ["", "# comment\n", "[theme]\n"] {
        assert_eq!(parse(empty).unwrap(), Settings::default(), "{empty:?}");
    }
}

#[test]
fn themes() {
    let settings = parse("[theme]\nimage = \"cyan\"\n").unwrap();
    assert_eq!(settings.theme.image, ratatui::style::Color::Cyan);
    // Shows the line, with the value marked
    let message = error("[theme]\nname = \"default\"\nimage = \"pink\"\n");
    assert!(message.contains("line 3"), "{message}");
    assert!(message.contains("image = \"pink\""), "{message}");
    assert!(message.contains("unknown color pink"), "{message}");
}

#[test]
fn unknown_keys() {
    let message = error("accent = \"red\"\n");
    assert!(message.contains("unknown field `accent`"), "{message}");
    let message = error("theme = \"red\"\n");
    assert!(message.contains("invalid type"), "{message}");
}
