//! Box's colors: the ones settings.toml's `theme` can change, by default mostly the terminal's text color and
//! grays, and the ones that mean something, like a container's state, which it can't. ANSI colors unless a
//! number or `#rrggbb` is given, so the terminal's theme decides how they look

use ratatui::style::Color;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    // Containers, the same in `box list` and the dashboard
    /// Also a volume's
    #[serde(deserialize_with = "color")]
    pub name: Color,
    #[serde(deserialize_with = "color")]
    pub image: Color,
    #[serde(deserialize_with = "color")]
    pub user: Color,
    /// In `box list`, the dashboard colors it by state
    #[serde(deserialize_with = "color")]
    pub status: Color,
    #[serde(deserialize_with = "color")]
    pub ip: Color,
    #[serde(deserialize_with = "color")]
    pub ports: Color,
    /// A container's working directory, also where it shows among the mounts
    #[serde(deserialize_with = "color")]
    pub workdir: Color,
    #[serde(deserialize_with = "color")]
    pub mounts: Color,

    // Images and volumes
    #[serde(deserialize_with = "color")]
    pub id: Color,
    #[serde(deserialize_with = "color")]
    pub size: Color,
    #[serde(deserialize_with = "color")]
    pub driver: Color,
    #[serde(deserialize_with = "color")]
    pub created: Color,
    /// How many containers use an image or volume; the dashboard dims none
    #[serde(deserialize_with = "color")]
    pub count: Color,

    // The dashboard's frame
    /// The active tab, key hints, the sorted column, dialogs
    #[serde(deserialize_with = "color")]
    pub accent: Color,
    /// Labels and what keys do
    #[serde(deserialize_with = "color")]
    pub muted: Color,
    /// Stopped rows, empty values, disabled keys
    #[serde(deserialize_with = "color")]
    pub dim: Color,
    #[serde(deserialize_with = "color")]
    pub border: Color,
    /// Behind the header, tabs, footer and dialogs
    #[serde(deserialize_with = "color")]
    pub bar: Color,
    /// Behind every other row, graphs, and what's behind a dialog
    #[serde(deserialize_with = "color")]
    pub stripe: Color,
    #[serde(deserialize_with = "color")]
    pub selected: Color,

    // A container's graphs
    /// The CPU graph and its value
    #[serde(deserialize_with = "color")]
    pub cpu: Color,
    /// The memory graph, and its value below the warning levels
    #[serde(deserialize_with = "color")]
    pub memory: Color,

    // What colors mean, not for settings
    /// The terminal's own text color
    #[serde(skip)]
    pub text: Color,
    #[serde(skip)]
    pub red: Color,
    #[serde(skip)]
    pub green: Color,
    #[serde(skip)]
    pub yellow: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Containers
            name: Color::Reset,
            image: Color::Reset,
            user: Color::DarkGray,
            status: Color::Reset,
            ip: Color::DarkGray,
            ports: Color::Reset,
            workdir: Color::Reset,
            mounts: Color::DarkGray,

            // Images and volumes
            id: Color::DarkGray,
            size: Color::DarkGray,
            driver: Color::DarkGray,
            created: Color::DarkGray,
            count: Color::Reset,

            // The dashboard's frame
            accent: Color::Yellow,
            muted: Color::Gray,
            dim: Color::DarkGray,
            // Grays of the 256, for a dark terminal
            border: Color::Indexed(239),
            bar: Color::Indexed(237),
            stripe: Color::Indexed(236),
            selected: Color::Indexed(239),

            // A container's graphs
            cpu: Color::Green,
            memory: Color::Blue,

            text: Color::Reset,
            red: Color::Red,
            green: Color::Green,
            yellow: Color::Yellow,
        }
    }
}

impl Theme {
    /// `normal`, then yellow from half, then red from 80%
    pub fn level(&self, fraction: f64, normal: Color) -> Color {
        if fraction >= 0.8 {
            self.red
        } else if fraction >= 0.5 {
            self.yellow
        } else {
            normal
        }
    }
}

/// `"default"` for the terminal's text color, an ANSI name like `cyan` or `bright-red`, a number of the 256,
/// or `#rrggbb`
fn color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Number(u8),
        Name(String),
    }
    let expected = r##"a color like "default", "cyan", "bright-red", 239 or "#ff8800""##;
    match Value::deserialize(deserializer).map_err(|_| serde::de::Error::custom(expected))? {
        Value::Number(index) => Ok(Color::Indexed(index)),
        Value::Name(name) if name.eq_ignore_ascii_case("default") => Ok(Color::Reset),
        Value::Name(name) => name.parse().map_err(|_| {
            serde::de::Error::custom(format!("unknown color {name}, expected {expected}"))
        }),
    }
}

#[cfg(test)]
mod tests;
