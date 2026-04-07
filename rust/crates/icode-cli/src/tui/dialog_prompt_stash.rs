use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_WIDTH: u16 = 50;
const MIN_HEIGHT: u16 = 14;

fn dialog_width(term_width: u16) -> u16 {
    if term_width >= 128 {
        116
    } else if term_width >= 96 {
        88
    } else {
        MIN_WIDTH
    }
}

fn dialog_height(term_height: u16) -> u16 {
    (term_height / 2).saturating_sub(6).max(MIN_HEIGHT)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptStashEntry {
    pub name: String,
    pub content: String,
    pub created_at: u128,
    pub updated_at: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StashMode {
    Browsing,
    SearchInput,
    CreateInput,
    ConfirmDelete(usize),
}

#[derive(Debug, Clone)]
pub enum StashAction {
    None,
    Close,
    Select(String),
    SaveNew(String, String),
    Delete(usize),
    StartSearch,
}

#[derive(Debug, Clone)]
pub struct PromptStashState {
    pub open: bool,
    pub entries: Vec<PromptStashEntry>,
    pub filtered: Vec<PromptStashEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub search: String,
    pub new_name_input: String,
    pub mode: StashMode,
}

impl PromptStashState {
    pub fn new() -> Self {
        Self {
            open: false,
            entries: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            search: String::new(),
            new_name_input: String::new(),
            mode: StashMode::Browsing,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.scroll_offset = 0;
        self.search.clear();
        self.new_name_input.clear();
        self.mode = StashMode::Browsing;
        self.apply_filter();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    fn stash_dir() -> PathBuf {
        let dir = if let Ok(path) = std::env::var("ICODE_STASH_TEST_DIR") {
            PathBuf::from(path)
        } else {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".icode").join("stash")
        };
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn stash_file_path() -> PathBuf {
        Self::stash_dir().join("stash_index.json")
    }

    pub fn load(&mut self) {
        self.entries.clear();
        let path = Self::stash_file_path();
        if !path.exists() {
            return;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(names) = serde_json::from_str::<Vec<String>>(&content) {
                for name in &names {
                    let entry_path = Self::stash_dir().join(format!("stash_{}.json", name));
                    if let Ok(entry_content) = fs::read_to_string(&entry_path) {
                        if let Ok(entry) = serde_json::from_str::<PromptStashEntry>(&entry_content)
                        {
                            self.entries.push(entry);
                        }
                    }
                }
            }
        }
        self.entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        self.apply_filter();
    }

    pub fn persist(&self) {
        let names: Vec<String> = self.entries.iter().map(|e| e.name.clone()).collect();
        if let Ok(json) = serde_json::to_string_pretty(&names) {
            let _ = fs::write(Self::stash_file_path(), json);
        }
        for entry in &self.entries {
            let entry_path = Self::stash_dir().join(format!("stash_{}.json", entry.name));
            if let Ok(json) = serde_json::to_string_pretty(entry) {
                let _ = fs::write(&entry_path, json);
            }
        }
    }

    pub fn delete_entry(&mut self, idx: usize) {
        if idx < self.entries.len() {
            let entry = &self.entries[idx];
            let entry_path = Self::stash_dir().join(format!("stash_{}.json", entry.name));
            let _ = fs::remove_file(&entry_path);
            self.entries.remove(idx);
            self.persist();
            self.apply_filter();
        }
    }

    pub fn save_new(&mut self, name: &str, content: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        if let Some(existing) = self.entries.iter_mut().find(|e| e.name == name) {
            existing.content = content.to_string();
            existing.updated_at = now;
        } else {
            let entry = PromptStashEntry {
                name: name.to_string(),
                content: content.to_string(),
                created_at: now,
                updated_at: now,
            };
            self.entries.push(entry);
        }
        self.persist();
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        if self.search.is_empty() {
            self.filtered = self.entries.clone();
        } else {
            let q = self.search.to_lowercase();
            self.filtered = self
                .entries
                .iter()
                .filter(|e| {
                    e.name.to_lowercase().contains(&q) || e.content.to_lowercase().contains(&q)
                })
                .cloned()
                .collect();
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> StashAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.close();
                StashAction::Close
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                StashAction::Close
            }

            (_, KeyCode::Enter) => match &self.mode {
                StashMode::ConfirmDelete(original_idx) => {
                    let idx = *original_idx;
                    self.mode = StashMode::Browsing;
                    StashAction::Delete(idx)
                }
                StashMode::CreateInput => {
                    let name = self.new_name_input.trim().to_string();
                    if name.is_empty() {
                        StashAction::None
                    } else {
                        StashAction::SaveNew(name, String::new())
                    }
                }
                StashMode::Browsing | StashMode::SearchInput => {
                    if let Some(entry) = self.filtered.get(self.selected) {
                        let content = entry.content.clone();
                        self.close();
                        StashAction::Select(content)
                    } else {
                        StashAction::None
                    }
                }
            },

            (_, KeyCode::Up) => {
                if matches!(self.mode, StashMode::Browsing | StashMode::SearchInput) {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                StashAction::None
            }
            (_, KeyCode::Down) => {
                if matches!(self.mode, StashMode::Browsing | StashMode::SearchInput) {
                    if self.selected < self.filtered.len().saturating_sub(1) {
                        self.selected += 1;
                    }
                }
                StashAction::None
            }
            (_, KeyCode::PageUp) => {
                if matches!(self.mode, StashMode::Browsing | StashMode::SearchInput) {
                    self.selected = self.selected.saturating_sub(10);
                }
                StashAction::None
            }
            (_, KeyCode::PageDown) => {
                if matches!(self.mode, StashMode::Browsing | StashMode::SearchInput) {
                    self.selected = (self.selected + 10).min(self.filtered.len().saturating_sub(1));
                }
                StashAction::None
            }

            (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                self.new_name_input.clear();
                self.mode = StashMode::CreateInput;
                StashAction::None
            }

            (_, KeyCode::Delete) | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                if let StashMode::ConfirmDelete(existing_idx) = &self.mode {
                    let idx = *existing_idx;
                    self.mode = StashMode::Browsing;
                    return StashAction::Delete(idx);
                } else if let Some(entry) = self.filtered.get(self.selected) {
                    if let Some(original_idx) =
                        self.entries.iter().position(|e| e.name == entry.name)
                    {
                        self.mode = StashMode::ConfirmDelete(original_idx);
                    }
                }
                StashAction::None
            }

            (_, KeyCode::Char('/')) => {
                if matches!(self.mode, StashMode::Browsing) {
                    self.search.clear();
                    self.mode = StashMode::SearchInput;
                    StashAction::StartSearch
                } else {
                    StashAction::None
                }
            }

            (_, KeyCode::Backspace) => match &self.mode {
                StashMode::SearchInput => {
                    if !self.search.is_empty() {
                        self.search.pop();
                        self.selected = 0;
                        self.apply_filter();
                    }
                    StashAction::None
                }
                StashMode::CreateInput => {
                    self.new_name_input.pop();
                    StashAction::None
                }
                StashMode::Browsing => {
                    if !self.search.is_empty() {
                        self.search.clear();
                        self.selected = 0;
                        self.apply_filter();
                    }
                    StashAction::None
                }
                StashMode::ConfirmDelete(_) => StashAction::None,
            },

            (_, KeyCode::Char(c)) => match &self.mode {
                StashMode::SearchInput => {
                    self.search.push(c);
                    self.selected = 0;
                    self.apply_filter();
                    StashAction::None
                }
                StashMode::CreateInput => {
                    self.new_name_input.push(c);
                    StashAction::None
                }
                StashMode::Browsing => {
                    self.search.clear();
                    self.search.push(c);
                    self.selected = 0;
                    self.mode = StashMode::SearchInput;
                    self.apply_filter();
                    StashAction::None
                }
                StashMode::ConfirmDelete(_) => StashAction::None,
            },

            _ => StashAction::None,
        }
    }
}

impl Default for PromptStashState {
    fn default() -> Self {
        Self::new()
    }
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

fn truncate(s: &str, max_len: usize) -> String {
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
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(0);
        format!("{}...", &s[..safe_end])
    }
}

fn compute_scroll_offset(state: &PromptStashState, visible_lines: usize) -> usize {
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

pub fn render_prompt_stash_dialog(
    frame: &mut Frame,
    state: &PromptStashState,
    area: Rect,
    theme: crate::tui::Theme,
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
            " Prompt Stash ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(block.clone(), dialog_area);

    let inner = block.inner(dialog_area);

    let is_creating = matches!(state.mode, StashMode::CreateInput);

    let constraints = if is_creating {
        vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ]
    } else {
        vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let input_line = match &state.mode {
        StashMode::SearchInput => {
            if state.search.is_empty() {
                Span::styled("Type to search...", Style::default().fg(theme.text_muted))
            } else {
                Span::styled(
                    format!("> {}", state.search),
                    Style::default().fg(theme.text),
                )
            }
        }
        StashMode::CreateInput => {
            if state.new_name_input.is_empty() {
                Span::styled(
                    "Name for this prompt...",
                    Style::default().fg(theme.text_muted),
                )
            } else {
                Span::styled(
                    format!("> {}", state.new_name_input),
                    Style::default().fg(theme.accent),
                )
            }
        }
        StashMode::Browsing => {
            if state.search.is_empty() {
                Span::styled("Type to search...", Style::default().fg(theme.text_muted))
            } else {
                Span::styled(
                    format!("> {}", state.search),
                    Style::default().fg(theme.text),
                )
            }
        }
        StashMode::ConfirmDelete(_) => Span::styled(
            "\u{26a0} Delete confirmation pending...",
            Style::default().fg(theme.error),
        ),
    };
    frame.render_widget(Paragraph::new(input_line), chunks[0]);

    let hint_text = match &state.mode {
        StashMode::CreateInput => "Enter: save  •  Esc: cancel  •  Type name for this prompt",
        StashMode::ConfirmDelete(_) => "Enter: confirm delete  •  Esc: cancel  •  Ctrl+D: confirm",
        _ => "Enter: use  •  Ctrl+N: new  •  Ctrl+D: delete  •  /: search  •  Esc: close",
    };
    let hint = Span::styled(
        hint_text,
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    let list_area = chunks[2];
    let visible_lines = list_area.height as usize;

    if state.filtered.is_empty() && !is_creating {
        let empty_msg = if !state.search.is_empty() {
            "No matching stashed prompts"
        } else {
            "No stashed prompts yet. Press Ctrl+N to create one."
        };
        let empty = Paragraph::new(Span::styled(
            empty_msg,
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(empty, list_area);
    } else {
        let scroll_offset = compute_scroll_offset(state, visible_lines);

        for (i, entry) in state.filtered.iter().enumerate().skip(scroll_offset) {
            let line_idx = i - scroll_offset;
            if line_idx >= visible_lines {
                break;
            }

            let is_selected = i == state.selected;
            let is_deleting = matches!(&state.mode, StashMode::ConfirmDelete(idx) if state.entries.get(*idx).map(|e| &e.name) == Some(&entry.name));
            let time_str = format_relative_time(entry.updated_at);
            let preview = truncate(&entry.content.lines().next().unwrap_or(""), 40);

            let mut title_spans = vec![Span::raw(" ")];

            if is_deleting {
                title_spans.push(Span::styled(
                    format!("\u{26a0} Delete '{}'? (Enter to confirm) ", entry.name),
                    Style::default()
                        .fg(theme.error)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                title_spans.push(Span::styled(
                    truncate(&entry.name, 20),
                    Style::default().fg(if is_selected {
                        theme.background
                    } else {
                        theme.text
                    }),
                ));
                title_spans.push(Span::styled(
                    format!(" - {} ", preview),
                    Style::default().fg(if is_selected {
                        theme.background
                    } else {
                        theme.text_muted
                    }),
                ));
            }

            let style = if is_deleting {
                Style::default()
                    .fg(theme.error)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
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
                let line = Line::from(title_spans);
                frame.render_widget(Paragraph::new(line).style(style), line_area);

                if list_area.width > 12 {
                    let time_style = if is_deleting {
                        Style::default().fg(theme.error)
                    } else if is_selected {
                        Style::default().fg(theme.background).bg(theme.primary)
                    } else {
                        Style::default().fg(theme.text_muted)
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
        format!(" {} prompt(s) ", state.filtered.len()),
        Style::default().fg(theme.text_muted),
    );
    frame.render_widget(Paragraph::new(footer), chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    fn test_stash_dir() -> PathBuf {
        std::env::temp_dir().join("icode-stash-test")
    }

    fn with_test_stash<T>(f: impl FnOnce() -> T) -> T {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = test_stash_dir();
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        std::env::set_var("ICODE_STASH_TEST_DIR", &dir);
        let result = f();
        let _ = fs::remove_dir_all(&dir);
        std::env::remove_var("ICODE_STASH_TEST_DIR");
        result
    }

    #[test]
    fn test_new_state() {
        let state = PromptStashState::new();
        assert!(!state.open);
        assert!(state.entries.is_empty());
        assert!(state.filtered.is_empty());
        assert_eq!(state.selected, 0);
        assert_eq!(state.mode, StashMode::Browsing);
    }

    #[test]
    fn test_open_close() {
        let mut state = PromptStashState::new();
        state.open();
        assert!(state.open);
        state.close();
        assert!(!state.open);
    }

    #[test]
    fn test_save_and_load() {
        with_test_stash(|| {
            let mut state = PromptStashState::new();
            state.save_new("test_prompt", "Hello, this is a test prompt content");
            state.load();
            assert_eq!(state.entries.len(), 1);
            assert_eq!(state.entries[0].name, "test_prompt");
            assert_eq!(
                state.entries[0].content,
                "Hello, this is a test prompt content"
            );
        });
    }

    #[test]
    fn test_delete_entry() {
        with_test_stash(|| {
            let mut state = PromptStashState::new();
            state.save_new("to_delete", "Content to delete");
            state.load();
            assert_eq!(state.entries.len(), 1);
            state.delete_entry(0);
            state.load();
            assert_eq!(state.entries.len(), 0);
        });
    }

    #[test]
    fn test_filter_by_name() {
        with_test_stash(|| {
            let mut state = PromptStashState::new();
            state.save_new("hello_prompt", "Hello world");
            state.save_new("goodbye_prompt", "Goodbye world");
            state.load();
            assert_eq!(state.entries.len(), 2);

            state.search = "hello".to_string();
            state.apply_filter();
            assert_eq!(state.filtered.len(), 1);
            assert_eq!(state.filtered[0].name, "hello_prompt");
        });
    }

    #[test]
    fn test_filter_by_content() {
        with_test_stash(|| {
            let mut state = PromptStashState::new();
            state.save_new("prompt_a", "Contains secret API key");
            state.save_new("prompt_b", "Contains regular text");
            state.load();

            state.search = "secret".to_string();
            state.apply_filter();
            assert_eq!(state.filtered.len(), 1);
            assert_eq!(state.filtered[0].name, "prompt_a");
        });
    }

    #[test]
    fn test_update_existing_entry() {
        with_test_stash(|| {
            let mut state = PromptStashState::new();
            state.save_new("my_prompt", "Original content");
            state.save_new("my_prompt", "Updated content");
            state.load();
            assert_eq!(state.entries.len(), 1);
            assert_eq!(state.entries[0].content, "Updated content");
        });
    }

    #[test]
    fn test_handle_key_esc_closes() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut state = PromptStashState::new();
        state.open();
        let action = state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(action, StashAction::Close));
        assert!(!state.open);
    }

    #[test]
    fn test_handle_key_enter_selects() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        with_test_stash(|| {
            let mut state = PromptStashState::new();
            state.save_new("test", "Test content");
            state.load();
            state.open();

            let action = state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            match action {
                StashAction::Select(content) => {
                    assert_eq!(content, "Test content");
                }
                _ => panic!("Expected Select action, got {:?}", action),
            }
        });
    }

    #[test]
    fn test_handle_key_navigation() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        with_test_stash(|| {
            let mut state = PromptStashState::new();
            state.save_new("a", "Content A");
            state.save_new("b", "Content B");
            state.save_new("c", "Content C");
            state.load();
            state.open();
            assert_eq!(state.selected, 0);

            state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
            assert_eq!(state.selected, 1);

            state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
            assert_eq!(state.selected, 2);

            state.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            assert_eq!(state.selected, 1);
        });
    }

    #[test]
    fn test_persist_across_instances() {
        with_test_stash(|| {
            {
                let mut state = PromptStashState::new();
                state.save_new("persistent", "This should persist");
            }
            {
                let mut state = PromptStashState::new();
                state.load();
                assert_eq!(state.entries.len(), 1);
                assert_eq!(state.entries[0].name, "persistent");
                assert_eq!(state.entries[0].content, "This should persist");
            }
        });
    }

    #[test]
    fn test_truncate_ascii() {
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 10), "hi");
        assert_eq!(truncate("abcdefg", 3), "...");
    }

    #[test]
    fn test_truncate_utf8_safe() {
        // Multi-byte chars: 日本語 (each char is 3 bytes)
        let japanese = "日本語テスト";
        let result = truncate(japanese, 10);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 13); // 10 chars worth + "..."
                                     // Should not panic and should produce valid UTF-8
        assert!(result.is_char_boundary(result.len()));
    }
}
