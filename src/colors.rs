//! Each field's color, the same in `box list` and the dashboard, ANSI so the terminal's theme decides
//! how they look

use ratatui::style::Color;

pub const NAME: Color = Color::Reset;
pub const ID: Color = Color::Reset;
pub const IMAGE: Color = Color::Cyan;
pub const USER: Color = Color::Blue;
/// In `box list`, the dashboard colors it by state
pub const STATUS: Color = Color::Yellow;
pub const IP: Color = Color::Magenta;
pub const PORTS: Color = Color::Green;
/// A container's working directory, and its mount
pub const WORKDIR: Color = Color::Magenta;
pub const MOUNTS: Color = Color::Reset;
pub const SIZE: Color = Color::Blue;
pub const CREATED: Color = Color::Yellow;
pub const DRIVER: Color = Color::Cyan;
/// In `box list`, the dashboard shows the text color, dimmed for none
pub const COUNT: Color = Color::Magenta;
