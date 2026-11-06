//! Home and XDG directories

use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::Result;

pub fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

/// `$variable` if absolute, else `~/fallback`, per the XDG spec
pub fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

pub fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("can't read {}", path.display()))
}
