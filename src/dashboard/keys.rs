//! The dashboard's keys, which the handlers, the footer's hints and the help all read, so they can't disagree

// Moving
pub const DOWN: char = 'j';
pub const UP: char = 'k';
pub const TOP: char = 'g';
pub const BOTTOM: char = 'G';
/// A view's own key, `1` for the first one
pub const VIEW_KEYS: [char; 3] = ['1', '2', '3'];

// The list
pub const FILTER: char = '/';
pub const SHOW_ALL: char = 'a';
pub const DETAILS: char = 'i';
pub const PRUNE: char = 'p';
pub const HELP: char = '?';
pub const QUIT: char = 'q';
/// With Ctrl, quits from anywhere
pub const INTERRUPT: char = 'c';

// Sorting; time is when containers started, when images and volumes were created
pub const SORT_NAME: char = 'N';
pub const SORT_CPU: char = 'P';
pub const SORT_MEMORY: char = 'M';
pub const SORT_SIZE: char = 'S';
pub const SORT_TIME: char = 'T';

// The selection
pub const STOP: char = 's';
pub const RESTART: char = 'r';
pub const REMOVE: char = 'd';
pub const LOGS: char = 'l';
pub const SHELL: char = 'e';

// Logs and dialogs
/// Pauses logs, toggles a prune target
pub const SPACE: char = ' ';
pub const YES: char = 'y';
pub const NO: char = 'n';

// How hints and the help name the keys that aren't characters
pub const ARROWS: &str = "↑↓";
pub const TAB: &str = "tab";
pub const ENTER: &str = "enter";
pub const ESC: &str = "esc";
pub const SPACE_NAME: &str = "space";

/// Keys as hints and the help show them, a space between
pub fn names(keys: &[char]) -> String {
    keys.iter()
        .map(char::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}
