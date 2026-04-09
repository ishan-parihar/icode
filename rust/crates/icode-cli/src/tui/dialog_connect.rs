use api::ProviderKind;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Modifier;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::popup_utils::{render_hint_bar, PopupConfig};
use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 48;
const MIN_HEIGHT: u16 = 8;

fn dialog_width(term_width: u16) -> u16 {
    term_width.saturating_sub(20).clamp(MIN_WIDTH, 60)
}

fn dialog_height(_term_height: u16) -> u16 {
    MIN_HEIGHT
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectStatus {
    Pending,
    Connecting,
    Success,
    Error(String),
}

#[derive(Debug)]
pub enum ConnectAction {
    None,
    Close,
    Submit(String),
}

#[derive(Debug, Clone)]
pub struct ConnectDialogState {
    pub open: bool,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    pub api_key_input: String,
    pub key_masked: bool,
    pub status: ConnectStatus,
}

impl ConnectDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            provider_name: String::new(),
            provider_kind: ProviderKind::Anthropic,
            api_key_input: String::new(),
            key_masked: true,
            status: ConnectStatus::Pending,
        }
    }

    pub fn open(&mut self, provider_name: String, kind: ProviderKind) {
        self.open = true;
        self.provider_name = provider_name;
        self.provider_kind = kind;
        self.api_key_input.clear();
        self.key_masked = true;
        self.status = ConnectStatus::Pending;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.provider_name.clear();
        self.api_key_input.clear();
        self.status = ConnectStatus::Pending;
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ConnectAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.close();
                ConnectAction::Close
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                ConnectAction::Close
            }
            (_, KeyCode::Enter) => {
                let trimmed = self.api_key_input.trim().to_string();
                if trimmed.is_empty() {
                    return ConnectAction::None;
                }
                self.status = ConnectStatus::Connecting;
                ConnectAction::Submit(trimmed)
            }
            (_, KeyCode::Backspace) => {
                self.api_key_input.pop();
                if matches!(self.status, ConnectStatus::Error(_)) {
                    self.status = ConnectStatus::Pending;
                }
                ConnectAction::None
            }
            (_, KeyCode::Char(c)) => {
                self.api_key_input.push(c);
                if matches!(self.status, ConnectStatus::Error(_)) {
                    self.status = ConnectStatus::Pending;
                }
                ConnectAction::None
            }
            _ => ConnectAction::None,
        }
    }

    pub fn mark_success(&mut self) {
        self.status = ConnectStatus::Success;
    }

    pub fn mark_error(&mut self, message: String) {
        self.status = ConnectStatus::Error(message);
    }
}

impl Default for ConnectDialogState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_connect_dialog(
    frame: &mut Frame,
    area: Rect,
    state: &mut ConnectDialogState,
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

    let block = PopupConfig::full(&format!("Connect to {}", state.provider_name)).to_block(theme);
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

    let prompt_text = Span::styled(
        format!("Enter API key for {}:", state.provider_name),
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    );
    frame.render_widget(Paragraph::new(prompt_text), chunks[0]);

    let input_line: Line<'static> = if state.api_key_input.is_empty() {
        Line::from(Span::styled(
            "sk-...\u{2588}".to_string(),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        ))
    } else if state.key_masked {
        let masked: String = state.api_key_input.chars().map(|_| '\u{2022}').collect();
        Line::from(vec![
            Span::styled(masked, Style::default().fg(theme.text)),
            Span::styled(
                "\u{2588}".to_string(),
                Style::default()
                    .fg(theme.background)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(state.api_key_input.clone(), Style::default().fg(theme.text)),
            Span::styled(
                "\u{2588}".to_string(),
                Style::default()
                    .fg(theme.background)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let input_block = Block::default().style(Style::default().bg(theme.background_element));
    frame.render_widget(input_block.clone(), chunks[1]);
    frame.render_widget(Paragraph::new(input_line), input_block.inner(chunks[1]));

    let status_text = match &state.status {
        ConnectStatus::Pending => Span::styled(
            "Paste your API key and press Enter",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        ),
        ConnectStatus::Connecting => Span::styled(
            "Validating key...",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        ConnectStatus::Success => Span::styled(
            "\u{2713} Connected successfully!",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ),
        ConnectStatus::Error(msg) => {
            Span::styled(format!("\u{2717} {msg}"), Style::default().fg(theme.error))
        }
    };
    frame.render_widget(Paragraph::new(status_text), chunks[2]);

    let hints = vec![("enter", "confirm"), ("esc", "cancel")];
    render_hint_bar(frame, chunks[3], &hints, theme);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_new_state_defaults() {
        let state = ConnectDialogState::new();
        assert!(!state.open);
        assert!(state.provider_name.is_empty());
        assert!(state.api_key_input.is_empty());
        assert!(state.key_masked);
        assert!(matches!(state.status, ConnectStatus::Pending));
    }

    #[test]
    fn test_open_initializes() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        assert!(state.open);
        assert_eq!(state.provider_name, "OpenAI");
        assert_eq!(state.provider_kind, ProviderKind::OpenAi);
    }

    #[test]
    fn test_esc_closes() {
        let mut state = ConnectDialogState::new();
        state.open("Anthropic".to_string(), ProviderKind::Anthropic);
        let action = state.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(action, ConnectAction::Close));
        assert!(!state.open);
    }

    #[test]
    fn test_ctrl_c_closes() {
        let mut state = ConnectDialogState::new();
        state.open("Gemini".to_string(), ProviderKind::Gemini);
        let action = state.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(action, ConnectAction::Close));
    }

    #[test]
    fn test_enter_submits_with_key() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        state.api_key_input = "sk-test-key".to_string();
        let action = state.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(action, ConnectAction::Submit(ref k) if k == "sk-test-key"));
        assert!(matches!(state.status, ConnectStatus::Connecting));
    }

    #[test]
    fn test_enter_empty_does_nothing() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        let action = state.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(action, ConnectAction::None));
    }

    #[test]
    fn test_char_appends_to_input() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        state.handle_key(key(KeyCode::Char('a'), KeyModifiers::NONE));
        state.handle_key(key(KeyCode::Char('b'), KeyModifiers::NONE));
        state.handle_key(key(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(state.api_key_input, "abc");
    }

    #[test]
    fn test_backspace_removes_last_char() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        state.api_key_input = "hello".to_string();
        state.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(state.api_key_input, "hell");
    }

    #[test]
    fn test_error_cleared_on_input() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        state.status = ConnectStatus::Error("Invalid key".to_string());
        state.handle_key(key(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(state.status, ConnectStatus::Pending));
    }

    #[test]
    fn test_error_cleared_on_backspace() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        state.status = ConnectStatus::Error("Invalid key".to_string());
        state.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(state.status, ConnectStatus::Pending));
    }

    #[test]
    fn test_mark_success() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        state.mark_success();
        assert!(matches!(state.status, ConnectStatus::Success));
    }

    #[test]
    fn test_mark_error() {
        let mut state = ConnectDialogState::new();
        state.open("OpenAI".to_string(), ProviderKind::OpenAi);
        state.mark_error("Rate limited".to_string());
        assert!(matches!(state.status, ConnectStatus::Error(ref msg) if msg == "Rate limited"));
    }
}
