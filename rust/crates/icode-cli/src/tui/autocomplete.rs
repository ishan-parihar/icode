use std::path::Path;

use crate::tui::file_picker::{fuzzy_match, scan_files};
use crate::tui::input::InputState;

/// What kind of autocomplete is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutocompleteMode {
    /// Triggered by `/` at position 0. Shows commands.
    #[default]
    Slash,
    /// Triggered by `@` at position 0 or after whitespace. Shows files.
    File,
    /// Triggered by `@` followed by mode cycling (future). Shows agents.
    Agent,
    /// Triggered by `@` followed by mode cycling (future). Shows MCP resources.
    Resource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Command,
    Agent,
    Resource,
}

#[derive(Debug, Clone)]
pub struct AutocompleteEntry {
    pub title: String,
    pub subtitle: String,
    pub kind: EntryKind,
}

/// Slash commands available in the TUI.
pub(crate) const SLASH_COMMANDS: &[&str] = &[
    "/help",
    "/status",
    "/cost",
    "/compact",
    "/clear",
    "/model",
    "/permissions",
    "/config",
    "/memory",
    "/diff",
    "/export",
    "/session",
    "/version",
    "/undo",
    "/redo",
];

#[derive(Debug, Default)]
pub struct AutocompleteState {
    pub open: bool,
    pub mode: AutocompleteMode,
    pub entries: Vec<AutocompleteEntry>,
    pub idx: usize,
    pub scroll: usize,
    pub trigger_pos: usize,
}

impl AutocompleteState {
    /// Create a new AutocompleteState with default values.
    pub fn new() -> Self {
        Self {
            open: false,
            mode: AutocompleteMode::Slash,
            entries: Vec::new(),
            idx: 0,
            scroll: 0,
            trigger_pos: 0,
        }
    }

    /// Open the autocomplete overlay in the given mode.
    pub fn open(&mut self, mode: AutocompleteMode, trigger_pos: usize) {
        self.open = true;
        self.mode = mode;
        self.trigger_pos = trigger_pos;
        self.idx = 0;
        self.scroll = 0;
        self.entries.clear();
    }

    /// Close the autocomplete overlay.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Rebuild entries based on current mode and input text.
    pub fn rebuild_entries(&mut self, input: &str, cwd: &Path) {
        match self.mode {
            AutocompleteMode::Slash => {
                let query = &input[self.trigger_pos..self.trigger_pos.max(input.len())];
                self.entries = SLASH_COMMANDS
                    .iter()
                    .filter(|cmd| cmd.starts_with(query))
                    .map(|cmd| AutocompleteEntry {
                        title: cmd.to_string(),
                        subtitle: command_help_text(cmd).to_string(),
                        kind: EntryKind::Command,
                    })
                    .collect();
            }
            AutocompleteMode::File => {
                let query = input.get(self.trigger_pos + 1..input.len()).unwrap_or("");
                let files = scan_files(cwd.to_str().unwrap_or("."));
                let matched = fuzzy_match(&files, query);
                self.entries = matched
                    .into_iter()
                    .map(|path| AutocompleteEntry {
                        title: path,
                        subtitle: String::new(),
                        kind: EntryKind::File,
                    })
                    .collect();
            }
            AutocompleteMode::Agent => {
                // Future: agent registry integration
                self.entries.clear();
            }
            AutocompleteMode::Resource => {
                // Future: MCP resource listing
                self.entries.clear();
            }
        }
        self.idx = 0;
        self.scroll = 0;
    }

    /// Move cursor up in the entry list.
    pub fn cursor_up(&mut self) {
        if self.idx > 0 {
            self.idx -= 1;
        }
    }

    /// Move cursor down in the entry list.
    pub fn cursor_down(&mut self) {
        if !self.entries.is_empty() && self.idx + 1 < self.entries.len() {
            self.idx += 1;
        }
    }

    /// Splice the selected entry into the input and close the overlay.
    ///
    /// For Slash mode: replaces input from `trigger_pos` to cursor with
    /// the selected command plus a trailing space.
    ///
    /// For File mode: replaces input from `trigger_pos` to cursor with
    /// `@<selected_path> `.
    pub fn select(&mut self, input: &mut InputState) {
        if self.entries.is_empty() {
            self.close();
            return;
        }

        let selected = &self.entries[self.idx];

        match self.mode {
            AutocompleteMode::Slash => {
                let replacement = format!("{} ", selected.title);
                let byte_start = char_to_byte(&input.value, self.trigger_pos);
                let byte_end = char_to_byte(&input.value, input.cursor);
                input
                    .value
                    .replace_range(byte_start..byte_end, &replacement);
                input.cursor = self.trigger_pos + replacement.chars().count();
            }
            AutocompleteMode::File => {
                let replacement = format!("@{} ", selected.title);
                let byte_start = char_to_byte(&input.value, self.trigger_pos);
                let byte_end = char_to_byte(&input.value, input.cursor);
                input
                    .value
                    .replace_range(byte_start..byte_end, &replacement);
                input.cursor = self.trigger_pos + replacement.chars().count();
            }
            AutocompleteMode::Agent => {
                // Future
            }
            AutocompleteMode::Resource => {
                // Future
            }
        }

        self.close();
    }

    /// Handle character insertion to detect trigger characters and close conditions.
    ///
    /// - `/` at cursor position 1 → opens Slash mode at position 0
    /// - `@` at cursor position 1 or after whitespace → opens File mode at cursor-1
    /// - Space → closes the overlay
    /// - Backspace at trigger_pos → closes the overlay (handled by caller)
    pub fn on_char_insert(&mut self, c: char, cursor: usize, input: &str) {
        if c == ' ' {
            self.close();
            return;
        }

        if c == '/' && cursor == 1 {
            self.open(AutocompleteMode::Slash, 0);
            return;
        }

        if c == '@' && (cursor == 1 || char_before_is_whitespace(input, cursor - 1)) {
            self.open(AutocompleteMode::File, cursor - 1);
        }
    }

    /// Handle backspace — close if cursor reaches the trigger position.
    pub fn on_backspace(&mut self, cursor: usize) {
        if self.open && cursor <= self.trigger_pos {
            self.close();
        }
    }
}

/// Return help text for a slash command.
fn command_help_text(cmd: &str) -> &'static str {
    match cmd {
        "/help" => "Show available slash commands",
        "/status" => "Show current session status",
        "/cost" => "Show cumulative token usage",
        "/compact" => "Compact conversation context",
        "/clear" => "Clear the current conversation",
        "/model" => "Show or switch current model",
        "/permissions" => "Show or switch permission mode",
        "/config" => "Inspect configuration",
        "/memory" => "Inspect project memory files",
        "/diff" => "Show git diff",
        "/export" => "Export conversation to file",
        "/session" => "List or switch sessions",
        "/version" => "Show CLI version",
        "/undo" => "Undo the last action",
        "/redo" => "Redo the last undone action",
        _ => "",
    }
}

/// Convert a character offset to a byte offset in the string.
fn char_to_byte(s: &str, char_offset: usize) -> usize {
    s.char_indices()
        .nth(char_offset)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

/// Check if the character before the given cursor position is whitespace.
fn char_before_is_whitespace(input: &str, cursor: usize) -> bool {
    if cursor == 0 {
        return false;
    }
    input
        .char_indices()
        .nth(cursor - 1)
        .map(|(_, ch)| ch.is_whitespace())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_is_closed() {
        let state = AutocompleteState::new();
        assert!(!state.open);
        assert_eq!(state.idx, 0);
        assert_eq!(state.scroll, 0);
        assert_eq!(state.trigger_pos, 0);
    }

    #[test]
    fn open_sets_mode_and_resets() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        assert!(state.open);
        assert_eq!(state.mode, AutocompleteMode::Slash);
        assert_eq!(state.trigger_pos, 0);
        assert_eq!(state.idx, 0);
        assert_eq!(state.scroll, 0);
        assert!(state.entries.is_empty());
    }

    #[test]
    fn close_sets_open_false() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.close();
        assert!(!state.open);
    }

    #[test]
    fn cursor_up_decrements_idx() {
        let mut state = AutocompleteState::new();
        state.entries = vec![
            AutocompleteEntry {
                title: "/help".into(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/status".into(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
        ];
        state.idx = 1;
        state.cursor_up();
        assert_eq!(state.idx, 0);
        state.cursor_up();
        assert_eq!(state.idx, 0); // should not go below 0
    }

    #[test]
    fn cursor_down_increments_idx() {
        let mut state = AutocompleteState::new();
        state.entries = vec![
            AutocompleteEntry {
                title: "/help".into(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
            AutocompleteEntry {
                title: "/status".into(),
                subtitle: String::new(),
                kind: EntryKind::Command,
            },
        ];
        state.cursor_down();
        assert_eq!(state.idx, 1);
        state.cursor_down();
        assert_eq!(state.idx, 1); // should not exceed len-1
    }

    #[test]
    fn on_char_insert_triggers_slash() {
        let mut state = AutocompleteState::new();
        state.on_char_insert('/', 1, "/");
        assert!(state.open);
        assert_eq!(state.mode, AutocompleteMode::Slash);
        assert_eq!(state.trigger_pos, 0);
    }

    #[test]
    fn on_char_insert_triggers_file_at_start() {
        let mut state = AutocompleteState::new();
        state.on_char_insert('@', 1, "@");
        assert!(state.open);
        assert_eq!(state.mode, AutocompleteMode::File);
        assert_eq!(state.trigger_pos, 0);
    }

    #[test]
    fn on_char_insert_triggers_file_after_whitespace() {
        let mut state = AutocompleteState::new();
        state.on_char_insert('@', 5, "foo @");
        assert!(state.open);
        assert_eq!(state.mode, AutocompleteMode::File);
        assert_eq!(state.trigger_pos, 4);
    }

    #[test]
    fn on_char_insert_space_closes() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.on_char_insert(' ', 2, "/h ");
        assert!(!state.open);
    }

    #[test]
    fn rebuild_entries_filters_slash_commands() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.rebuild_entries("/h", Path::new("."));
        assert!(!state.entries.is_empty());
        assert!(state.entries.iter().all(|e| e.title.starts_with("/h")));
        assert!(state.entries.iter().all(|e| e.kind == EntryKind::Command));
    }

    #[test]
    fn command_help_text_returns_descriptions() {
        assert!(!command_help_text("/help").is_empty());
        assert!(!command_help_text("/model").is_empty());
        assert_eq!(command_help_text("/unknown"), "");
    }
}
