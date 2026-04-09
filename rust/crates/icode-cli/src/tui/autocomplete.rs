use std::path::Path;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};
use ratatui::Frame;

use crate::tui::file_picker::{fuzzy_match, scan_files};
use crate::tui::frecency::FrecencyStore;
use crate::tui::input::InputState;
use crate::tui::popup_utils::{anchored_popup, clear_area, left_border_block};
use crate::tui::theme::Theme;

const KNOWN_AGENTS: &[&str] = &["build", "plan", "debug", "review", "test"];

fn agent_help_text(agent: &str) -> &'static str {
    match agent {
        "build" => "Implement features, fix bugs, refactor",
        "plan" => "Explore codebase and create implementation plans",
        "debug" => "Diagnose and fix issues systematically",
        "review" => "Review code for quality and security",
        "test" => "Write and run tests",
        _ => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutocompleteMode {
    #[default]
    Slash,
    File,
    Agent,
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

fn char_to_byte(s: &str, char_offset: usize) -> usize {
    s.char_indices()
        .nth(char_offset)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

fn char_before_is_whitespace(input: &str, cursor: usize) -> bool {
    if cursor == 0 {
        return false;
    }
    input
        .char_indices()
        .nth(cursor - 1)
        .is_some_and(|(_, ch)| ch.is_whitespace())
}

fn fuzzy_match_single(query: &str, target: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    let mut qi = 0;
    for &tc in &target_lower {
        if tc == query_lower[qi] {
            qi += 1;
            if qi == query_lower.len() {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Default)]
pub struct AutocompleteState {
    pub open: bool,
    pub mode: AutocompleteMode,
    pub entries: Vec<AutocompleteEntry>,
    pub idx: usize,
    pub scroll: usize,
    pub trigger_pos: usize,
    pub anchor_x: u16,
    pub anchor_y: u16,
    pub anchor_width: u16,
    pub max_items: usize,
    pub mouse_hover: Option<usize>,
}

impl AutocompleteState {
    pub fn new() -> Self {
        Self {
            open: false,
            mode: AutocompleteMode::Slash,
            entries: Vec::new(),
            idx: 0,
            scroll: 0,
            trigger_pos: 0,
            anchor_x: 0,
            anchor_y: 0,
            anchor_width: 40,
            max_items: 10,
            mouse_hover: None,
        }
    }

    pub fn open(&mut self, mode: AutocompleteMode, trigger_pos: usize) {
        self.open = true;
        self.mode = mode;
        self.trigger_pos = trigger_pos;
        self.idx = 0;
        self.scroll = 0;
        self.entries.clear();
        self.mouse_hover = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.mouse_hover = None;
    }

    pub fn set_anchor(&mut self, anchor_x: u16, anchor_y: u16, anchor_width: u16) {
        self.anchor_x = anchor_x;
        self.anchor_y = anchor_y;
        self.anchor_width = anchor_width;
    }

    pub fn cycle_mode(&mut self) {
        self.mode = match self.mode {
            AutocompleteMode::Slash => AutocompleteMode::File,
            AutocompleteMode::File => AutocompleteMode::Agent,
            AutocompleteMode::Agent => AutocompleteMode::Resource,
            AutocompleteMode::Resource => AutocompleteMode::Slash,
        };
        self.idx = 0;
        self.scroll = 0;
        self.mouse_hover = None;
    }

    pub fn record_selection(&mut self, text: &str, frecency: &mut FrecencyStore) {
        frecency.record(text);
    }

    pub fn rebuild_entries(&mut self, input: &str, cwd: &Path, frecency: Option<&FrecencyStore>) {
        match self.mode {
            AutocompleteMode::Slash => {
                let query = input.get(self.trigger_pos..).unwrap_or("");
                let filtered: Vec<&&str> = SLASH_COMMANDS
                    .iter()
                    .filter(|cmd| fuzzy_match_single(query, cmd))
                    .collect();

                let sorted = if query.is_empty() {
                    if let Some(store) = frecency {
                        let mut with_scores: Vec<&&str> = filtered;
                        with_scores.sort_by(|a, b| {
                            let score_a = store.get_score(a);
                            let score_b = store.get_score(b);
                            score_b
                                .partial_cmp(&score_a)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        with_scores
                    } else {
                        filtered
                    }
                } else {
                    filtered
                };

                self.entries = sorted
                    .into_iter()
                    .map(|cmd| AutocompleteEntry {
                        title: cmd.to_string(),
                        subtitle: command_help_text(cmd).to_string(),
                        kind: EntryKind::Command,
                    })
                    .collect();
            }
            AutocompleteMode::File => {
                let query = input.get(self.trigger_pos + 1..).unwrap_or("");
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
                let query = input.get(self.trigger_pos + 1..).unwrap_or("");
                self.entries = KNOWN_AGENTS
                    .iter()
                    .filter(|agent| fuzzy_match_single(query, agent))
                    .map(|agent| AutocompleteEntry {
                        title: agent.to_string(),
                        subtitle: agent_help_text(agent).to_string(),
                        kind: EntryKind::Agent,
                    })
                    .collect();
            }
            AutocompleteMode::Resource => {
                self.entries.clear();
            }
        }
        self.idx = 0;
        self.scroll = 0;
        self.mouse_hover = None;
    }

    pub fn rebuild_entries_legacy(&mut self, input: &str, cwd: &Path) {
        self.rebuild_entries(input, cwd, None);
    }

    pub fn cursor_up(&mut self) {
        if self.idx > 0 {
            self.idx -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if !self.entries.is_empty() && self.idx + 1 < self.entries.len() {
            self.idx += 1;
        }
    }

    pub fn on_mouse_move(&mut self, y: u16, popup_top: u16) {
        if y < popup_top {
            self.mouse_hover = None;
            return;
        }
        let offset = (y - popup_top) as usize;
        if offset < self.entries.len() {
            self.mouse_hover = Some(offset);
            self.idx = offset;
        } else {
            self.mouse_hover = None;
        }
    }

    pub fn on_click(&self) -> Option<String> {
        self.mouse_hover
            .and_then(|i| self.entries.get(i))
            .map(|e| e.title.clone())
    }

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
                let replacement = format!("@{} ", selected.title);
                let byte_start = char_to_byte(&input.value, self.trigger_pos);
                let byte_end = char_to_byte(&input.value, input.cursor);
                input
                    .value
                    .replace_range(byte_start..byte_end, &replacement);
                input.cursor = self.trigger_pos + replacement.chars().count();
            }
            AutocompleteMode::Resource => {
                let replacement = format!("{} ", selected.title);
                let byte_start = char_to_byte(&input.value, self.trigger_pos);
                let byte_end = char_to_byte(&input.value, input.cursor);
                input
                    .value
                    .replace_range(byte_start..byte_end, &replacement);
                input.cursor = self.trigger_pos + replacement.chars().count();
            }
        }

        self.close();
    }

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

    pub fn on_backspace(&mut self, cursor: usize) {
        if self.open && cursor <= self.trigger_pos {
            self.close();
        }
    }
}

pub fn render_autocomplete_overlay(
    frame: &mut Frame,
    state: &AutocompleteState,
    area: Rect,
    theme: Theme,
) {
    if !state.open {
        return;
    }

    let popup_rect = anchored_popup(
        area,
        state.anchor_x,
        state.anchor_y,
        state.anchor_width,
        state.entries.len() as u16,
        state.max_items as u16,
    );

    clear_area(frame, popup_rect);

    let visible_count = popup_rect.height.saturating_sub(2) as usize;
    if visible_count == 0 {
        return;
    }

    let block = left_border_block(theme, theme.border, "", Some(theme.background_panel));

    let scroll_start = state.scroll;
    let items: Vec<ListItem> = state
        .entries
        .iter()
        .skip(scroll_start)
        .take(visible_count)
        .enumerate()
        .map(|(offset, entry)| {
            let global_idx = scroll_start + offset;
            let is_selected = global_idx == state.idx;

            let mut spans = vec![Span::styled(
                entry.title.clone(),
                if is_selected {
                    Style::default()
                        .bg(theme.primary)
                        .fg(theme.text_inverse)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                },
            )];

            if !entry.subtitle.is_empty() {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    entry.subtitle.clone(),
                    if is_selected {
                        Style::default().bg(theme.primary).fg(theme.text_inverse)
                    } else {
                        Style::default().fg(theme.text_muted)
                    },
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    if state.entries.is_empty() {
        let no_items = vec![ListItem::new(Line::from(Span::styled(
            "No matching items",
            Style::default().fg(theme.text_muted),
        )))];
        let list = List::new(no_items).block(block);
        frame.render_widget(list, popup_rect);
        return;
    }

    let list = List::new(items).block(block);
    frame.render_widget(list, popup_rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frecency_store() -> FrecencyStore {
        let path = std::env::temp_dir().join(format!(
            "ac-test-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut store = FrecencyStore::new(path);
        for _ in 0..5 {
            store.record("/help");
        }
        store.record("/status");
        store.record("/clear");
        store
    }

    #[test]
    fn fuzzy_match_single_basic() {
        assert!(fuzzy_match_single("hel", "/help"));
        assert!(fuzzy_match_single("hp", "/help"));
        assert!(!fuzzy_match_single("xyz", "/help"));
        assert!(fuzzy_match_single("", "/help"));
        assert!(fuzzy_match_single("/help", "/help"));
    }

    #[test]
    fn fuzzy_match_single_case_insensitive() {
        assert!(fuzzy_match_single("HELP", "/help"));
        assert!(fuzzy_match_single("HelP", "/help"));
    }

    #[test]
    fn fuzzy_match_subsequence_matching() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.rebuild_entries("/h", Path::new("."), None);
        assert!(!state.entries.is_empty());
        assert!(state
            .entries
            .iter()
            .all(|e| fuzzy_match_single("/h", &e.title)));
    }

    #[test]
    fn frecency_sorts_when_query_empty() {
        let frecency = make_frecency_store();
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.rebuild_entries("", Path::new("."), Some(&frecency));

        assert!(!state.entries.is_empty());
        assert_eq!(state.entries[0].title, "/help");
    }

    #[test]
    fn frecency_no_sort_when_query_present() {
        let frecency = make_frecency_store();
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.rebuild_entries("/stat", Path::new("."), Some(&frecency));

        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].title, "/status");
    }

    #[test]
    fn anchor_position_set_correctly() {
        let mut state = AutocompleteState::new();
        state.set_anchor(5, 20, 50);
        assert_eq!(state.anchor_x, 5);
        assert_eq!(state.anchor_y, 20);
        assert_eq!(state.anchor_width, 50);
    }

    #[test]
    fn default_anchor_is_zero() {
        let state = AutocompleteState::new();
        assert_eq!(state.anchor_x, 0);
        assert_eq!(state.anchor_y, 0);
        assert_eq!(state.anchor_width, 40);
    }

    #[test]
    fn default_max_items_is_ten() {
        let state = AutocompleteState::new();
        assert_eq!(state.max_items, 10);
    }

    #[test]
    fn cycle_mode_sequence() {
        let mut state = AutocompleteState::new();
        assert_eq!(state.mode, AutocompleteMode::Slash);

        state.cycle_mode();
        assert_eq!(state.mode, AutocompleteMode::File);

        state.cycle_mode();
        assert_eq!(state.mode, AutocompleteMode::Agent);

        state.cycle_mode();
        assert_eq!(state.mode, AutocompleteMode::Resource);

        state.cycle_mode();
        assert_eq!(state.mode, AutocompleteMode::Slash);
    }

    #[test]
    fn agent_mode_lists_known_agents() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Agent, 0);
        state.rebuild_entries("@", Path::new("."), None);

        assert_eq!(state.entries.len(), 5);
        assert!(state.entries.iter().all(|e| e.kind == EntryKind::Agent));
        let titles: Vec<&str> = state.entries.iter().map(|e| e.title.as_str()).collect();
        assert!(titles.contains(&"build"));
        assert!(titles.contains(&"plan"));
        assert!(titles.contains(&"debug"));
        assert!(titles.contains(&"review"));
        assert!(titles.contains(&"test"));
    }

    #[test]
    fn agent_mode_fuzzy_filters() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Agent, 0);
        state.rebuild_entries("@bui", Path::new("."), None);

        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].title, "build");
    }

    #[test]
    fn agent_mode_has_help_text() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Agent, 0);
        state.rebuild_entries("@", Path::new("."), None);

        for entry in &state.entries {
            assert!(!entry.subtitle.is_empty());
        }
    }

    #[test]
    fn resource_mode_is_empty_stub() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Resource, 0);
        state.rebuild_entries("@", Path::new("."), None);

        assert!(state.entries.is_empty());
    }

    #[test]
    fn mouse_move_updates_hover_and_idx() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.rebuild_entries("/", Path::new("."), None);

        state.on_mouse_move(5, 5);
        assert_eq!(state.mouse_hover, Some(0));
        assert_eq!(state.idx, 0);

        state.on_mouse_move(6, 5);
        assert_eq!(state.mouse_hover, Some(1));
        assert_eq!(state.idx, 1);
    }

    #[test]
    fn mouse_move_outside_popup_clears_hover() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.rebuild_entries("/", Path::new("."), None);

        state.on_mouse_move(4, 5);
        assert_eq!(state.mouse_hover, None);
    }

    #[test]
    fn mouse_click_returns_selected_text() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Slash, 0);
        state.rebuild_entries("/", Path::new("."), None);

        state.on_mouse_move(5, 5);
        let clicked = state.on_click();
        assert!(clicked.is_some());
        assert_eq!(clicked.unwrap(), state.entries[0].title);
    }

    #[test]
    fn record_selection_updates_frecency() {
        let mut frecency = make_frecency_store();
        let initial_score = frecency.get_score("/status");

        frecency.record("/status");
        let new_score = frecency.get_score("/status");
        assert!(new_score > initial_score);
    }

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
        assert_eq!(state.idx, 0);
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
        assert_eq!(state.idx, 1);
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
        state.rebuild_entries_legacy("/h", Path::new("."));
        assert!(!state.entries.is_empty());
        assert!(state
            .entries
            .iter()
            .all(|e| fuzzy_match_single("/h", &e.title)));
        assert!(state.entries.iter().all(|e| e.kind == EntryKind::Command));
    }

    #[test]
    fn command_help_text_returns_descriptions() {
        assert!(!command_help_text("/help").is_empty());
        assert!(!command_help_text("/model").is_empty());
        assert_eq!(command_help_text("/unknown"), "");
    }

    #[test]
    fn empty_entries_when_filter_active() {
        let mut state = AutocompleteState::new();
        state.open(AutocompleteMode::Agent, 0);
        state.rebuild_entries("@xyz", Path::new("."), None);
        assert!(state.entries.is_empty());
    }
}
