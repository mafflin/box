//! The dashboard's settings file, `~/.config/box/settings.yml`

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

use crate::{Result, paths};

#[cfg(test)]
const EXAMPLE: &str = include_str!("example.yml");

pub fn path() -> PathBuf {
    paths::xdg_dir("XDG_CONFIG_HOME", ".config")
        .join("box")
        .join("settings.yml")
}

/// The dashboard's accent color, one of the terminal's ANSI colors
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Yellow,
    Red,
    Green,
    Blue,
    Magenta,
    Cyan,
}

#[derive(Deserialize, Default, PartialEq, Debug)]
#[serde(
    default,
    deny_unknown_fields,
    expecting = "a mapping like accent: yellow"
)]
pub struct Settings {
    #[serde(deserialize_with = "or_default")]
    pub accent: Accent,
}

/// The default for null, like `accent:` with nothing after it
fn or_default<'de, D: serde::Deserializer<'de>, T: Deserialize<'de> + Default>(
    deserializer: D,
) -> std::result::Result<T, D::Error> {
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
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

/// Only the first document, the defaults when there's none or it's empty
fn parse(content: &str) -> std::result::Result<Settings, serde_norway::Error> {
    let Some(document) = serde_norway::Deserializer::from_str(content).next() else {
        return Ok(Settings::default());
    };
    Ok(Option::<Settings>::deserialize(document)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(content: &str) -> String {
        parse(content).err().unwrap().to_string()
    }

    #[test]
    fn example_parses_to_the_defaults() {
        assert_eq!(parse(EXAMPLE).unwrap(), Settings::default());
    }

    #[test]
    fn empty_settings() {
        for empty in ["", "# comment\n", "~\n", "accent: ~\n", "accent:\n"] {
            assert_eq!(parse(empty).unwrap(), Settings::default(), "{empty:?}");
        }
    }

    #[test]
    fn accents() {
        assert_eq!(parse("accent: cyan\n").unwrap().accent, Accent::Cyan);
        assert_eq!(
            parse("accent: red\n---\naccent: 1\n").unwrap().accent,
            Accent::Red
        );
        let message = error("accent: pink\n");
        assert!(
            message.contains("accent: unknown variant `pink`, expected one of `yellow`"),
            "{message}"
        );
        assert!(
            error("accent: 5\n").contains("accent: "),
            "{}",
            error("accent: 5\n")
        );
    }

    #[test]
    fn unknown_keys() {
        let message = error("accnt: red\n");
        assert!(message.contains("unknown field `accnt`"), "{message}");
        let message = error("- red\n");
        assert!(
            message.contains("expected a mapping like accent: yellow"),
            "{message}"
        );
    }
}
