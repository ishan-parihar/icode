use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 70;
const MIN_HEIGHT: u16 = 16;

/// Operating modes for the workspace dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceDialogMode {
    /// Normal browsing / searching.
    Listing,
    /// Confirming deletion of the workspace at the given index (into `filtered`).
    ConfirmDelete(usize),
    /// Prompting the user for a new workspace path.
    EnterCreatePath,
}

fn dialog_width(term_width: u16) -> u16 {
    if term_width >= 128 {
        100
    } else if term_width >= 96 {
        86
    } else {
        MIN_WIDTH
    }
}

fn dialog_height(term_height: u16) -> u16 {
    (term_height / 2).saturating_sub(6).max(MIN_HEIGHT)
}

#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
    pub last_active: u128,
    pub session_count: usize,
}

#[derive(Debug, Clone)]
pub struct WorkspaceDialogState {
    pub open: bool,
    pub workspaces: Vec<WorkspaceEntry>,
    pub filtered: Vec<WorkspaceEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub search: String,
    pub mode: WorkspaceDialogMode,
    pub create_input: String,
}

impl WorkspaceDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            workspaces: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            search: String::new(),
            mode: WorkspaceDialogMode::Listing,
            create_input: String::new(),
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.scroll_offset = 0;
        self.search.clear();
        self.mode = WorkspaceDialogMode::Listing;
        self.create_input.clear();
        self.scan_workspaces();
        self.apply_filter();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.mode = WorkspaceDialogMode::Listing;
        self.create_input.clear();
    }

    pub fn scan_workspaces(&mut self) {
        self.workspaces.clear();

        let mut found: Vec<WorkspaceEntry> = Vec::new();

        let search_dirs = Self::search_locations();
        for dir in search_dirs {
            if !dir.is_dir() {
                continue;
            }

            for marker in &[".icode", ".opencode"] {
                let marker_path = dir.join(marker);
                if marker_path.is_dir() {
                    let session_count = Self::count_sessions(&marker_path);
                    let last_active = Self::last_active(&marker_path);

                    if let Some(existing) = found.iter_mut().find(|e| e.path == dir) {
                        existing.session_count += session_count;
                        if last_active > existing.last_active {
                            existing.last_active = last_active;
                        }
                    } else {
                        found.push(WorkspaceEntry {
                            path: dir.clone(),
                            last_active,
                            session_count,
                        });
                    }
                    break;
                }
            }
        }

        found.sort_by(|a, b| b.last_active.cmp(&a.last_active));
        self.workspaces = found;
    }

    fn search_locations() -> Vec<PathBuf> {
        let mut locations = Vec::new();

        if let Ok(cwd) = std::env::current_dir() {
            locations.push(cwd.clone());
            if let Some(parent) = cwd.parent() {
                locations.push(parent.to_path_buf());
            }
        }

        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(&home);
            if let Ok(entries) = std::fs::read_dir(&home_path) {
                for entry in entries.filter_map(std::result::Result::ok) {
                    let path = entry.path();
                    if path.is_dir() {
                        locations.push(path);
                    }
                }
            }
        }

        if let Ok(projects) = std::env::var("PROJECTS") {
            for dir in projects.split(':') {
                let path = PathBuf::from(dir);
                if path.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&path) {
                        for entry in entries.filter_map(std::result::Result::ok) {
                            let sub = entry.path();
                            if sub.is_dir() {
                                locations.push(sub);
                            }
                        }
                    }
                }
            }
        }

        locations.dedup();
        locations
    }

    fn count_sessions(marker_dir: &PathBuf) -> usize {
        let sessions_dir = marker_dir.join("sessions");
        if !sessions_dir.is_dir() {
            return 0;
        }
        std::fs::read_dir(&sessions_dir)
            .ok()
            .map_or(0, |entries| entries.filter_map(std::result::Result::ok).count())
    }

    fn last_active(marker_dir: &PathBuf) -> u128 {
        let sessions_dir = marker_dir.join("sessions");
        if !sessions_dir.is_dir() {
            return 0;
        }

        let mut latest = 0u128;
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.filter_map(std::result::Result::ok) {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                            if duration.as_millis() > latest {
                                latest = duration.as_millis();
                            }
                        }
                    }
                }
            }
        }
        latest
    }

    pub fn apply_filter(&mut self) {
        if self.search.is_empty() {
            self.filtered = self.workspaces.clone();
        } else {
            let q = self.search.to_lowercase();
            self.filtered = self
                .workspaces
                .iter()
                .filter(|w| fuzzy_match(&q, &w.path.to_string_lossy()))
                .cloned()
                .collect();
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> WorkspaceAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        if self.mode == WorkspaceDialogMode::EnterCreatePath {
            match key.code {
                KeyCode::Esc => {
                    self.mode = WorkspaceDialogMode::Listing;
                    self.create_input.clear();
                    return WorkspaceAction::None;
                }
                KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                    self.mode = WorkspaceDialogMode::Listing;
                    self.create_input.clear();
                    return WorkspaceAction::None;
                }
                KeyCode::Enter => {
                    let path = self.create_input.clone();
                    self.create_input.clear();
                    self.mode = WorkspaceDialogMode::Listing;
                    if path.is_empty() {
                        return WorkspaceAction::None;
                    }
                    return WorkspaceAction::Create(path);
                }
                KeyCode::Char(c) => {
                    self.create_input.push(c);
                    return WorkspaceAction::None;
                }
                KeyCode::Backspace => {
                    self.create_input.pop();
                    return WorkspaceAction::None;
                }
                _ => return WorkspaceAction::None,
            }
        }

        if let WorkspaceDialogMode::ConfirmDelete(idx) = self.mode {
            match key.code {
                KeyCode::Esc => {
                    self.mode = WorkspaceDialogMode::Listing;
                    return WorkspaceAction::None;
                }
                KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                    self.mode = WorkspaceDialogMode::Listing;
                    return WorkspaceAction::None;
                }
                KeyCode::Char('y') | KeyCode::Enter => {
                    if let Some(ws) = self.filtered.get(idx) {
                        let path = ws.path.to_string_lossy().to_string();
                        self.mode = WorkspaceDialogMode::Listing;
                        return WorkspaceAction::Delete(path);
                    }
                    self.mode = WorkspaceDialogMode::Listing;
                    return WorkspaceAction::None;
                }
                KeyCode::Char('n') => {
                    self.mode = WorkspaceDialogMode::Listing;
                    return WorkspaceAction::None;
                }
                _ => return WorkspaceAction::None,
            }
        }

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.close();
                WorkspaceAction::Close
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                WorkspaceAction::Close
            }
            (_, KeyCode::Enter) => {
                if let Some(workspace) = self.filtered.get(self.selected) {
                    let path = workspace.path.to_string_lossy().to_string();
                    self.close();
                    WorkspaceAction::Switch(path)
                } else {
                    WorkspaceAction::None
                }
            }
            (_, KeyCode::Up) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                WorkspaceAction::None
            }
            (_, KeyCode::Down) => {
                if self.selected < self.filtered.len().saturating_sub(1) {
                    self.selected += 1;
                }
                WorkspaceAction::None
            }
            (_, KeyCode::PageUp) => {
                self.selected = self.selected.saturating_sub(10);
                WorkspaceAction::None
            }
            (_, KeyCode::PageDown) => {
                self.selected = (self.selected + 10).min(self.filtered.len().saturating_sub(1));
                WorkspaceAction::None
            }
            (_, KeyCode::Home) => {
                self.selected = 0;
                WorkspaceAction::None
            }
            (_, KeyCode::End) => {
                self.selected = self.filtered.len().saturating_sub(1);
                WorkspaceAction::None
            }
            (_, KeyCode::Char('/')) => {
                self.search.clear();
                WorkspaceAction::StartSearch
            }
            (_, KeyCode::Delete) => {
                if !self.filtered.is_empty() {
                    self.mode = WorkspaceDialogMode::ConfirmDelete(self.selected);
                }
                WorkspaceAction::None
            }
            (_, KeyCode::Char('n')) => {
                self.mode = WorkspaceDialogMode::EnterCreatePath;
                self.create_input.clear();
                WorkspaceAction::None
            }
            (_, KeyCode::Backspace) => {
                if !self.search.is_empty() {
                    self.search.pop();
                    self.selected = 0;
                    self.apply_filter();
                }
                WorkspaceAction::None
            }
            (_, KeyCode::Char(c)) => {
                self.search.push(c);
                self.selected = 0;
                self.apply_filter();
                WorkspaceAction::None
            }
            _ => WorkspaceAction::None,
        }
    }
}

impl Default for WorkspaceDialogState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum WorkspaceAction {
    None,
    Close,
    Switch(String),
    StartSearch,
    Delete(String),
    Create(String),
}

fn format_relative_time(epoch_millis: u128) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let diff = now.saturating_sub(epoch_millis);
    let secs = diff / 1000;
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

fn fuzzy_match(query: &str, haystack: &str) -> bool {
    let query_chars = query.chars().peekable();
    let mut haystack_chars = haystack.chars().peekable();
    let mut last_match_idx = 0usize;
    let haystack_lower: Vec<char> = haystack.to_lowercase().chars().collect();

    for qc in query_chars {
        let mut found = false;
        for (i, hc) in haystack_chars.clone().enumerate() {
            if qc == hc {
                let abs_idx = last_match_idx + i + 1;
                if abs_idx > haystack_lower.len() {
                    return false;
                }
                last_match_idx = abs_idx;
                for _ in 0..=i {
                    haystack_chars.next();
                }
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

pub fn render_workspace_dialog(
    frame: &mut Frame,
    state: &mut WorkspaceDialogState,
    area: Rect,
    theme: Theme,
) {
    if !state.open {
        return;
    }

    let width = dialog_width(area.width).min(area.width.saturating_sub(4));
    let height = dialog_height(area.height).min(area.height.saturating_sub(4));
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(
            " Workspaces ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(block.clone(), dialog_area);

    let inner = block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let search_line = if state.search.is_empty() {
        Span::styled("Type to search...", Style::default().fg(theme.text_muted))
    } else {
        Span::styled(
            format!("> {}", state.search),
            Style::default().fg(theme.text),
        )
    };
    frame.render_widget(Paragraph::new(search_line), chunks[0]);

    let hint = Span::styled(
        "Enter: open  •  N: new  •  Del: delete  •  /: search  •  ↑↓: navigate  •  Esc: close",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    let list_area = chunks[2];
    let visible_lines = list_area.height as usize;

    if state.filtered.is_empty() && state.search.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No workspaces found. Press N to create one.",
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(empty, list_area);
    } else if state.filtered.is_empty() && !state.search.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No matches. Press N to create new.",
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(empty, list_area);
    } else {
        let scroll_offset = compute_scroll_offset(state, visible_lines);
        state.scroll_offset = scroll_offset;

        for (i, workspace) in state.filtered.iter().enumerate().skip(scroll_offset) {
            let line_idx = i - scroll_offset;
            if line_idx >= visible_lines {
                break;
            }

            let is_selected = i == state.selected;
            let time_str = if workspace.last_active > 0 {
                format_relative_time(workspace.last_active)
            } else {
                "never".to_string()
            };

            let display_path = truncate_path(&workspace.path, 42);

            let indicator = if is_selected { "\u{25b6} " } else { "  " };
            let sessions_label = format!("{} sess", workspace.session_count);

            let line_spans = vec![
                Span::styled(
                    indicator,
                    Style::default().fg(if is_selected {
                        theme.background
                    } else {
                        theme.primary
                    }),
                ),
                Span::styled(
                    &display_path,
                    Style::default().fg(if is_selected {
                        theme.background
                    } else {
                        theme.text
                    }),
                ),
                Span::styled(
                    format!(" ({sessions_label}) "),
                    Style::default().fg(if is_selected {
                        theme.background
                    } else {
                        theme.text_muted
                    }),
                ),
            ];

            let style = if is_selected {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let line_y = list_area.y + line_idx as u16;
            if line_y < list_area.bottom() {
                let line_area = Rect::new(list_area.x, line_y, list_area.width, 1);
                let line = Line::from(line_spans);
                frame.render_widget(Paragraph::new(line).style(style), line_area);

                if list_area.width > 14 {
                    let time_style = if is_selected {
                        Style::default().fg(theme.background).bg(theme.primary)
                    } else {
                        Style::default().fg(theme.secondary)
                    };
                    frame.render_widget(
                        Paragraph::new(time_str).style(time_style),
                        Rect::new(list_area.x + list_area.width - 10, line_y, 10, 1),
                    );
                }
            }
        }
    }

    let footer = Span::styled(
        format!(" {} workspace(s) ", state.filtered.len()),
        Style::default().fg(theme.text_muted),
    );
    frame.render_widget(Paragraph::new(footer), chunks[3]);
}

fn compute_scroll_offset(state: &WorkspaceDialogState, visible_lines: usize) -> usize {
    let pos = state.selected;
    if pos < state.scroll_offset {
        return pos;
    }
    let end = state.scroll_offset + visible_lines.saturating_sub(1);
    if pos >= end && visible_lines > 0 {
        return pos.saturating_sub(visible_lines.saturating_sub(1));
    }
    state.scroll_offset
}

fn truncate_path(path: &PathBuf, max_len: usize) -> String {
    let s = path.to_string_lossy();
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        "...".to_string()
    } else {
        let limit = max_len - 3;
        let safe_end = s
            .char_indices()
            .take_while(|(idx, _)| *idx < limit)
            .last()
            .map_or(0, |(idx, ch)| idx + ch.len_utf8());
        format!("...{}", &s[safe_end..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::fs;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn with_test_workspace<T>(f: impl FnOnce() -> T) -> T {
        let test_dir = std::env::temp_dir().join("icode-workspace-test");
        let _ = fs::remove_dir_all(&test_dir);
        let sessions = test_dir.join(".icode").join("sessions");
        fs::create_dir_all(&sessions).unwrap();

        let session_file = sessions.join("test.json");
        fs::write(&session_file, "{}").unwrap();

        std::env::set_current_dir(&test_dir).unwrap();
        let result = f();
        let _ = fs::remove_dir_all(&test_dir);
        result
    }

    #[test]
    fn test_new_state() {
        let state = WorkspaceDialogState::new();
        assert!(!state.open);
        assert!(state.workspaces.is_empty());
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_open_close() {
        let mut state = WorkspaceDialogState::new();
        state.open();
        assert!(state.open);
        state.close();
        assert!(!state.open);
    }

    #[test]
    fn test_scan_finds_current_workspace() {
        with_test_workspace(|| {
            let mut state = WorkspaceDialogState::new();
            state.scan_workspaces();
            assert!(!state.workspaces.is_empty());
            assert!(state.workspaces[0].session_count >= 1);
        });
    }

    #[test]
    fn test_esc_closes() {
        let mut state = WorkspaceDialogState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(action, WorkspaceAction::Close));
        assert!(!state.open);
    }

    #[test]
    fn test_ctrl_c_closes() {
        let mut state = WorkspaceDialogState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(action, WorkspaceAction::Close));
    }

    #[test]
    fn test_enter_switches() {
        with_test_workspace(|| {
            let mut state = WorkspaceDialogState::new();
            state.scan_workspaces();
            state.apply_filter();
            state.open();

            if !state.filtered.is_empty() {
                let action = state.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
                match action {
                    WorkspaceAction::Switch(path) => {
                        assert!(!path.is_empty());
                    }
                    _ => panic!("Expected Switch action, got {action:?}"),
                }
                assert!(!state.open);
            }
        });
    }

    #[test]
    fn test_navigation() {
        with_test_workspace(|| {
            let mut state = WorkspaceDialogState::new();
            state.scan_workspaces();
            state.open();

            if state.filtered.len() > 1 {
                state.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
                assert_eq!(state.selected, 1);
                state.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
                assert_eq!(state.selected, 0);
            }
        });
    }

    #[test]
    fn test_search_filters() {
        with_test_workspace(|| {
            let mut state = WorkspaceDialogState::new();
            state.scan_workspaces();
            state.open();

            let initial = state.filtered.len();
            state.search = "nonexistent_workspace_xyz".to_string();
            state.selected = 0;
            state.apply_filter();

            assert!(state.filtered.len() <= initial);
        });
    }

    #[test]
    fn test_format_relative_time() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let recent = now - 30_000;
        let result = format_relative_time(recent);
        assert_eq!(result, "just now");

        let hour_ago = now - 3_600_000;
        let result = format_relative_time(hour_ago);
        assert!(result.contains("h ago"));
    }

    #[test]
    fn test_truncate_path() {
        let path = PathBuf::from("/very/long/path/that/exceeds/limit/workspace");
        let result = truncate_path(&path, 20);
        assert!(result.starts_with("..."));
        assert!(result.ends_with("workspace"));
        assert!(result.len() < path.to_string_lossy().len());
    }

    #[test]
    fn test_truncate_path_short() {
        let path = PathBuf::from("/short");
        assert_eq!(truncate_path(&path, 20), "/short");
    }

    #[test]
    fn test_page_navigation() {
        let mut state = WorkspaceDialogState::new();
        state.open();
        state.handle_key(key(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(
            state.selected,
            10.min(state.filtered.len().saturating_sub(1))
        );
    }

    #[test]
    fn test_fuzzy_match_basic() {
        assert!(fuzzy_match("icode", "/home/user/icode/rust"));
        assert!(fuzzy_match("rust", "/home/user/icode/rust"));
        assert!(fuzzy_match("hm", "/home/user/icode"));
        assert!(!fuzzy_match("xyz", "/home/user/icode"));
    }

    #[test]
    fn test_fuzzy_match_empty() {
        assert!(fuzzy_match("", "/any/path"));
        assert!(!fuzzy_match("a", ""));
    }

    #[test]
    fn test_delete_opens_confirmation() {
        with_test_workspace(|| {
            let mut state = WorkspaceDialogState::new();
            state.scan_workspaces();
            state.apply_filter();
            state.open();

            let action = state.handle_key(key(KeyCode::Delete, KeyModifiers::NONE));
            assert!(matches!(action, WorkspaceAction::None));
            assert!(matches!(state.mode, WorkspaceDialogMode::ConfirmDelete(_)));
        });
    }

    #[test]
    fn test_n_opens_create_mode() {
        let mut state = WorkspaceDialogState::new();
        state.open();

        let action = state.handle_key(key(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(action, WorkspaceAction::None));
        assert!(matches!(state.mode, WorkspaceDialogMode::EnterCreatePath));
    }

    #[test]
    fn test_create_path_entry() {
        let mut state = WorkspaceDialogState::new();
        state.open();
        state.handle_key(key(KeyCode::Char('n'), KeyModifiers::NONE));

        state.handle_key(key(KeyCode::Char('/'), KeyModifiers::NONE));
        state.handle_key(key(KeyCode::Char('t'), KeyModifiers::NONE));
        state.handle_key(key(KeyCode::Char('e'), KeyModifiers::NONE));
        state.handle_key(key(KeyCode::Char('s'), KeyModifiers::NONE));
        state.handle_key(key(KeyCode::Char('t'), KeyModifiers::NONE));

        let action = state.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        match action {
            WorkspaceAction::Create(path) => assert_eq!(path, "/test"),
            _ => panic!("Expected Create action, got {action:?}"),
        }
    }

    #[test]
    fn test_esc_cancels_delete_confirmation() {
        with_test_workspace(|| {
            let mut state = WorkspaceDialogState::new();
            state.scan_workspaces();
            state.apply_filter();
            state.open();

            state.handle_key(key(KeyCode::Delete, KeyModifiers::NONE));
            assert!(matches!(state.mode, WorkspaceDialogMode::ConfirmDelete(_)));

            let action = state.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
            assert!(matches!(action, WorkspaceAction::None));
            assert!(matches!(state.mode, WorkspaceDialogMode::Listing));
        });
    }

    #[test]
    fn test_esc_cancels_create_mode() {
        let mut state = WorkspaceDialogState::new();
        state.open();
        state.handle_key(key(KeyCode::Char('n'), KeyModifiers::NONE));

        let action = state.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(action, WorkspaceAction::None));
        assert!(matches!(state.mode, WorkspaceDialogMode::Listing));
        assert!(state.create_input.is_empty());
    }
}
