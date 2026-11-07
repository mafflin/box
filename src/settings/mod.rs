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
mod tests;
