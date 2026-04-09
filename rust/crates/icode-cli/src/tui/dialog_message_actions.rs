use crate::tui::popup_utils;
use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageAction {
    Revert,
    Copy,
    Fork,
}

/// A single option in the message action dialog, matching DialogSelect styling.
pub struct DialogSelectOption {
    pub title: String,
    pub value: MessageAction,
    pub description: String,
}

pub struct MessageActionDialogState {
    pub open: bool,
    pub message_id: usize,
    pub message_content: String,
    pub selected: usize,
    pub filter: String,
    options: Vec<DialogSelectOption>,
}

impl MessageActionDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            message_id: 0,
            message_content: String::new(),
            selected: 0,
            filter: String::new(),
            options: vec![
                DialogSelectOption {
                    title: "Revert".to_string(),
                    value: MessageAction::Revert,
                    description: "undo messages and file changes".to_string(),
                },
                DialogSelectOption {
                    title: "Copy".to_string(),
                    value: MessageAction::Copy,
                    description: "message text to clipboard".to_string(),
                },
                DialogSelectOption {
                    title: "Fork".to_string(),
                    value: MessageAction::Fork,
                    description: "create a new session from here".to_string(),
                },
            ],
        }
    }

    fn filtered_options(&self) -> Vec<&DialogSelectOption> {
        if self.filter.is_empty() {
            self.options.iter().collect()
        } else {
            let lower = self.filter.to_lowercase();
            self.options
                .iter()
                .filter(|o| {
                    let title_lower = o.title.to_lowercase();
                    if title_lower.starts_with(&lower) {
                        return true;
                    }
                    if lower.len() >= 3 {
                        o.description
                            .to_lowercase()
                            .split_whitespace()
                            .any(|word| word.starts_with(&lower))
                    } else {
                        false
                    }
                })
                .collect()
        }
    }

    pub fn open(&mut self, message_id: usize, content: String) {
        self.open = true;
        self.message_id = message_id;
        self.message_content = content;
        self.selected = 0;
        self.filter.clear();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> Option<MessageAction> {
        use crossterm::event::KeyCode;
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.close();
                None
            }
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                None
            }
            KeyCode::Down => {
                let filtered = self.filtered_options();
                if self.selected < filtered.len().saturating_sub(1) {
                    self.selected += 1;
                }
                None
            }
            KeyCode::Enter => {
                let filtered = self.filtered_options();
                if let Some(opt) = filtered.get(self.selected) {
                    let action = opt.value.clone();
                    self.close();
                    Some(action)
                } else {
                    None
                }
            }
            KeyCode::Char('1') => {
                self.close();
                Some(MessageAction::Revert)
            }
            KeyCode::Char('2') => {
                self.close();
                Some(MessageAction::Copy)
            }
            KeyCode::Char('3') => {
                self.close();
                Some(MessageAction::Fork)
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
                None
            }
            KeyCode::Char(c) if !c.is_control() => {
                self.filter.push(c);
                self.selected = 0;
                None
            }
            _ => None,
        }
    }
}

pub fn render_message_action_dialog(
    frame: &mut Frame,
    state: &MessageActionDialogState,
    area: Rect,
    theme: Theme,
) {
    if !state.open {
        return;
    }

    let filtered = state.filtered_options();
    let option_count = filtered.len() as u16;
    let content_height = 2 + option_count * 2;

    let dialog_area = popup_utils::popup_dimensions(area, 0.5, 30, 60, 0.5, content_height);

    frame.render_widget(Clear, dialog_area);

    let block = popup_utils::left_border_block(
        theme,
        theme.warning,
        "Message Actions",
        Some(theme.background_panel),
    );

    frame.render_widget(block.clone(), dialog_area);

    let inner = block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(option_count * 2),
            Constraint::Length(1),
        ])
        .split(inner);

    let title_spans = vec![
        Span::styled(
            "Message Actions",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("esc", Style::default().fg(theme.text_muted)),
    ];
    frame.render_widget(Paragraph::new(Line::from(title_spans)), chunks[0]);

    if option_count > 0 {
        let option_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                (0..option_count)
                    .map(|_| Constraint::Length(2))
                    .collect::<Vec<_>>(),
            )
            .split(chunks[1]);

        for (i, opt) in filtered.iter().enumerate() {
            let is_selected = i == state.selected;

            let bullet_style = if is_selected {
                Style::default().fg(theme.primary)
            } else {
                Style::default().fg(theme.text_muted)
            };

            let title_style = if is_selected {
                Style::default()
                    .fg(theme.text_inverse)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
            };

            let desc_style = if is_selected {
                Style::default().fg(theme.text_inverse).bg(theme.primary)
            } else {
                Style::default().fg(theme.text_muted)
            };

            let line = Line::from(vec![
                Span::styled("● ", bullet_style),
                Span::styled(opt.title.clone(), title_style),
                Span::raw(" "),
                Span::styled(opt.description.clone(), desc_style),
            ]);

            frame.render_widget(Paragraph::new(line), option_chunks[i]);
        }
    }

    // Hint bar
    popup_utils::render_hint_bar(
        frame,
        chunks[2],
        &[("1/2/3", "select"), ("enter", "confirm"), ("esc", "close")],
        theme,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = MessageActionDialogState::new();
        assert!(!state.open);
        assert_eq!(state.selected, 0);
        assert!(state.filter.is_empty());
        assert_eq!(state.options.len(), 3);
    }

    #[test]
    fn test_open_and_close() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test content".to_string());
        assert!(state.open);
        assert_eq!(state.message_id, 1);
        assert_eq!(state.message_content, "test content");

        state.close();
        assert!(!state.open);
    }

    #[test]
    fn test_enter_selects_current() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        let result = state.handle_key(crossterm::event::KeyCode::Down);
        assert!(result.is_none());

        let result = state.handle_key(crossterm::event::KeyCode::Enter);
        assert_eq!(result, Some(MessageAction::Copy));
        assert!(!state.open);
    }

    #[test]
    fn test_numeric_shortcut_revert() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        let result = state.handle_key(crossterm::event::KeyCode::Char('1'));
        assert_eq!(result, Some(MessageAction::Revert));
        assert!(!state.open);
    }

    #[test]
    fn test_numeric_shortcut_copy() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        let result = state.handle_key(crossterm::event::KeyCode::Char('2'));
        assert_eq!(result, Some(MessageAction::Copy));
    }

    #[test]
    fn test_numeric_shortcut_fork() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        let result = state.handle_key(crossterm::event::KeyCode::Char('3'));
        assert_eq!(result, Some(MessageAction::Fork));
    }

    #[test]
    fn test_esc_closes() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        let result = state.handle_key(crossterm::event::KeyCode::Esc);
        assert!(result.is_none());
        assert!(!state.open);
    }

    #[test]
    fn test_filter_reduces_options() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        state.handle_key(crossterm::event::KeyCode::Char('c'));
        assert_eq!(state.filter, "c");

        let filtered = state.filtered_options();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, MessageAction::Copy);
    }

    #[test]
    fn test_filter_resets_selection() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        state.handle_key(crossterm::event::KeyCode::Down);
        state.handle_key(crossterm::event::KeyCode::Down);
        assert_eq!(state.selected, 2);

        // Filtering should reset selection
        state.handle_key(crossterm::event::KeyCode::Char('c'));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_backspace_removes_filter() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        state.handle_key(crossterm::event::KeyCode::Char('c'));
        assert_eq!(state.filter, "c");

        state.handle_key(crossterm::event::KeyCode::Backspace);
        assert_eq!(state.filter, "");
        assert_eq!(state.filtered_options().len(), 3);
    }

    #[test]
    fn test_filter_case_insensitive() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        state.handle_key(crossterm::event::KeyCode::Char('R'));
        let filtered = state.filtered_options();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, MessageAction::Revert);
    }

    #[test]
    fn test_up_down_navigation_with_filter() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        // Navigate normally
        state.handle_key(crossterm::event::KeyCode::Down);
        assert_eq!(state.selected, 1);
        state.handle_key(crossterm::event::KeyCode::Up);
        assert_eq!(state.selected, 0);

        // Up at top stays at top
        state.handle_key(crossterm::event::KeyCode::Up);
        assert_eq!(state.selected, 0);

        // Down at bottom stays at bottom
        state.handle_key(crossterm::event::KeyCode::Down);
        state.handle_key(crossterm::event::KeyCode::Down);
        state.handle_key(crossterm::event::KeyCode::Down);
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn test_control_chars_ignored() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        // Control characters should be ignored (not added to filter)
        state.handle_key(crossterm::event::KeyCode::Tab);
        assert!(state.filter.is_empty());
    }

    #[test]
    fn test_filter_by_description() {
        let mut state = MessageActionDialogState::new();
        state.open(1, "test".to_string());

        // "clipboard" appears in Copy's description
        let lower = "clipboard".to_string();
        state.filter = lower;
        let filtered = state.filtered_options();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, MessageAction::Copy);
    }
}
