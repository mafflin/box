//! The settings file, `~/.config/box/settings.toml`

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

use crate::theme::Theme;
use crate::{Result, paths};

#[cfg(test)]
const EXAMPLE: &str = include_str!("example.toml");

pub fn path() -> PathBuf {
    paths::xdg_dir("XDG_CONFIG_HOME", ".config")
        .join("box")
        .join("settings.toml")
}

#[derive(Deserialize, Default, PartialEq, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub theme: Theme,
}

impl Settings {
    /// The defaults when there's no settings file
    pub fn load() -> Result<Self> {
        let path = path();
        if !path.exists() {
            return Ok(Self::default());
        }
        read(&path)
    }
}

fn read(path: &Path) -> Result<Settings> {
    let content = paths::read_file(path)?;
    parse(&content).with_context(|| format!("invalid {}", path.display()))
}

fn parse(content: &str) -> std::result::Result<Settings, toml::de::Error> {
    toml::from_str(content)
}

#[cfg(test)]
mod tests {
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
}
