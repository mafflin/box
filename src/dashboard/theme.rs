//! ANSI colors, so the terminal's theme decides how they look, and a few grays of the 256 for
//! backgrounds, which assume a dark terminal

use ratatui::style::Color;

use crate::settings::Accent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// The terminal's own text color, and its background
    pub text: Color,
    /// Labels and key hints
    pub muted: Color,
    /// Stopped rows, empty values, disabled keys
    pub dim: Color,
    pub border: Color,
    /// Text on a colored background
    pub on_color: Color,
    /// Zebra rows, graphs, what's behind a dialog
    pub bg_alt: Color,
    /// The header, tabs, footer and dialogs
    pub bg_bar: Color,
    pub bg_selected: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub accent: Color,
}

impl Theme {
    pub fn new(accent: Accent) -> Self {
        Self {
            text: Color::Reset,
            muted: Color::Gray,
            dim: Color::DarkGray,
            border: Color::Indexed(239),
            on_color: Color::Black,
            bg_alt: Color::Indexed(236),
            bg_bar: Color::Indexed(237),
            bg_selected: Color::Indexed(239),
            red: Color::Red,
            green: Color::Green,
            yellow: Color::Yellow,
            blue: Color::Blue,
            magenta: Color::Magenta,
            cyan: Color::Cyan,
            accent: match accent {
                Accent::Yellow => Color::Yellow,
                Accent::Red => Color::Red,
                Accent::Green => Color::Green,
                Accent::Blue => Color::Blue,
                Accent::Magenta => Color::Magenta,
                Accent::Cyan => Color::Cyan,
            },
        }
    }

    /// The text color, then yellow from half, then red from 80%
    pub fn level(&self, fraction: f64) -> Color {
        if fraction >= 0.8 {
            self.red
        } else if fraction >= 0.5 {
            self.yellow
        } else {
            self.text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        let theme = Theme::new(Accent::Cyan);
        assert_eq!(theme.text, Color::Reset);
        assert_eq!(theme.accent, Color::Cyan);
        assert_eq!(Theme::new(Accent::Yellow).accent, Color::Yellow);
        assert_eq!(theme.level(0.9), Color::Red);
    }
}
