use super::*;

fn theme(toml: &str) -> Result<Theme, toml::de::Error> {
    toml::from_str(toml)
}

#[test]
fn colors() {
    let theme = theme(
        "user = \"Default\"\nimage = \"cyan\"\nports = \"bright-red\"\nid = \"dark-gray\"\nsize = \"grey\"\nborder = 240\nbar = \"#102030\"\n",
    )
    .unwrap();
    assert_eq!(theme.user, Color::Reset);
    assert_eq!(theme.image, Color::Cyan);
    assert_eq!(theme.ports, Color::LightRed);
    assert_eq!(theme.id, Color::DarkGray);
    assert_eq!(theme.size, Color::Gray);
    assert_eq!(theme.border, Color::Indexed(240));
    assert_eq!(theme.bar, Color::Rgb(0x10, 0x20, 0x30));
    // The rest keep their defaults
    assert_eq!(theme.accent, Color::Yellow);
    assert_eq!(theme.level(0.9, Color::Blue), Color::LightRed);
    assert_eq!(theme.level(0.6, Color::Blue), Color::Yellow);
    assert_eq!(theme.level(0.1, Color::Blue), Color::Blue);
}

#[test]
fn wrong_colors() {
    for (toml, message) in [
        ("image = \"pink\"\n", "unknown color pink"),
        ("border = 256\n", "a color like"),
        ("border = -1\n", "a color like"),
        ("image = [\"red\"]\n", "a color like"),
        ("red = \"blue\"\n", "unknown field `red`"),
    ] {
        let error = theme(toml).unwrap_err().to_string();
        assert!(error.contains(message), "{toml:?}: {error}");
    }
}
