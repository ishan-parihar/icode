use api::{list_all_models, provider_display_name, scan_provider_auth_status, ProviderKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;

use crate::tui::dialog_connect::{ConnectAction, ConnectDialogState};
use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 56;
const MIN_HEIGHT: u16 = 14;

fn dialog_width(term_width: u16) -> u16 {
    if term_width >= 128 {
        80
    } else if term_width >= 96 {
        72
    } else {
        MIN_WIDTH
    }
}

fn dialog_height(term_height: u16) -> u16 {
    (term_height / 2).saturating_sub(6).max(MIN_HEIGHT)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStatus {
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Clone)]
pub struct ProviderEntry {
    pub name: String,
    pub kind: ProviderKind,
    pub status: ProviderStatus,
    pub configured: bool,
    pub models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderDialogState {
    pub open: bool,
    pub providers: Vec<ProviderEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub search: String,
    pub filtered: Vec<usize>,
    pub connect_dialog: ConnectDialogState,
}

impl ProviderDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            providers: Self::detect_providers(),
            selected: 0,
            scroll_offset: 0,
            search: String::new(),
            filtered: Vec::new(),
            connect_dialog: ConnectDialogState::new(),
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.scroll_offset = 0;
        self.search.clear();
        self.providers = Self::detect_providers();
        self.apply_filter();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.connect_dialog.close();
    }

    fn detect_providers() -> Vec<ProviderEntry> {
        let auth_statuses = scan_provider_auth_status();

        let mut models_by_provider: HashMap<ProviderKind, Vec<String>> = HashMap::new();
        for entry in list_all_models() {
            models_by_provider
                .entry(entry.provider)
                .or_default()
                .push(entry.alias.to_string());
        }

        auth_statuses
            .into_iter()
            .map(|status| {
                let models = models_by_provider
                    .get(&status.kind)
                    .cloned()
                    .unwrap_or_default();
                ProviderEntry {
                    name: status.display_name.to_string(),
                    kind: status.kind,
                    status: if status.has_auth {
                        ProviderStatus::Connected
                    } else {
                        ProviderStatus::Disconnected
                    },
                    configured: status.has_auth,
                    models,
                }
            })
            .collect()
    }

    fn apply_filter(&mut self) {
        self.filtered.clear();
        for (idx, provider) in self.providers.iter().enumerate() {
            if self.search.is_empty()
                || provider
                    .name
                    .to_lowercase()
                    .contains(&self.search.to_lowercase())
            {
                self.filtered.push(idx);
            }
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ProviderAction {
        if self.connect_dialog.open {
            match self.connect_dialog.handle_key(key) {
                ConnectAction::Submit(api_key) => {
                    let kind = self.connect_dialog.provider_kind;
                    let display = self.connect_dialog.provider_name.clone();
                    return ProviderAction::ConnectProvider(kind, display, api_key);
                }
                ConnectAction::Close => {
                    self.connect_dialog.close();
                    return ProviderAction::None;
                }
                ConnectAction::None => return ProviderAction::None,
            }
        }

        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.close();
                ProviderAction::Close
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                ProviderAction::Close
            }
            (_, KeyCode::Enter) => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    let provider = &self.providers[idx];
                    if provider.configured {
                        ProviderAction::Toggle(idx)
                    } else {
                        self.connect_dialog
                            .open(provider.name.clone(), provider.kind);
                        ProviderAction::None
                    }
                } else {
                    ProviderAction::None
                }
            }
            (_, KeyCode::Up) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                ProviderAction::None
            }
            (_, KeyCode::Down) => {
                if self.selected < self.filtered.len().saturating_sub(1) {
                    self.selected += 1;
                }
                ProviderAction::None
            }
            (_, KeyCode::PageUp) => {
                self.selected = self.selected.saturating_sub(10);
                ProviderAction::None
            }
            (_, KeyCode::PageDown) => {
                self.selected = (self.selected + 10).min(self.filtered.len().saturating_sub(1));
                ProviderAction::None
            }
            (_, KeyCode::Char('/')) => {
                self.search.clear();
                ProviderAction::None
            }
            (_, KeyCode::Backspace) => {
                if !self.search.is_empty() {
                    self.search.pop();
                    self.selected = 0;
                    self.apply_filter();
                }
                ProviderAction::None
            }
            (_, KeyCode::Char('d')) => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    ProviderAction::ViewDocs(idx)
                } else {
                    ProviderAction::None
                }
            }
            (_, KeyCode::Char(c)) => {
                self.search.push(c);
                self.selected = 0;
                self.apply_filter();
                ProviderAction::None
            }
            _ => ProviderAction::None,
        }
    }

    pub fn refresh_providers(&mut self) {
        self.providers = Self::detect_providers();
        self.apply_filter();
    }
}

impl Default for ProviderDialogState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum ProviderAction {
    None,
    Close,
    Toggle(usize),
    ViewDocs(usize),
    ConnectProvider(ProviderKind, String, String),
}

fn status_icon(status: &ProviderStatus) -> &'static str {
    match status {
        ProviderStatus::Connected => "\u{2713}",
        ProviderStatus::Disconnected => "\u{25cb}",
        ProviderStatus::Error => "\u{2717}",
    }
}

fn status_color(status: &ProviderStatus, theme: Theme) -> ratatui::style::Color {
    match status {
        ProviderStatus::Connected => theme.success,
        ProviderStatus::Disconnected => theme.text_muted,
        ProviderStatus::Error => theme.error,
    }
}

pub fn render_provider_dialog(
    frame: &mut Frame,
    state: &mut ProviderDialogState,
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
            " Provider Connections ",
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

    let hint = if state.connect_dialog.open {
        Span::styled(
            "Connect to provider...",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(
            "Enter: connect  •  d: docs  •  /: search  •  Esc: close",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )
    };
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    let list_area = chunks[2];
    let visible_lines = list_area.height as usize;

    if state.connect_dialog.open {
        use crate::tui::dialog_connect::render_connect_dialog;
        render_connect_dialog(frame, area, &mut state.connect_dialog, theme);
    } else if state.filtered.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No providers match search",
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(empty, list_area);
    } else {
        let scroll_offset = compute_scroll_offset(state, visible_lines);
        state.scroll_offset = scroll_offset;

        for (i, &provider_idx) in state.filtered.iter().enumerate().skip(scroll_offset) {
            let line_idx = i - scroll_offset;
            if line_idx >= visible_lines {
                break;
            }

            let provider = &state.providers[provider_idx];
            let is_selected = i == state.selected;

            let icon = status_icon(&provider.status);
            let color = status_color(&provider.status, theme);

            let configured_label = if provider.configured {
                "configured"
            } else {
                "connect"
            };

            let model_count = provider.models.len();

            let mut spans = vec![Span::raw(" ")];
            spans.push(Span::styled(
                format!("{icon} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                &provider.name,
                Style::default().fg(if is_selected {
                    theme.background
                } else {
                    theme.text
                }),
            ));
            spans.push(Span::styled(
                format!(" ({configured_label}) "),
                Style::default().fg(if is_selected {
                    theme.background
                } else {
                    theme.text_muted
                }),
            ));
            spans.push(Span::styled(
                format!("{model_count} models"),
                Style::default().fg(if is_selected {
                    theme.background
                } else {
                    theme.info
                }),
            ));

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
                let line = Line::from(spans);
                frame.render_widget(Paragraph::new(line).style(style), line_area);
            }
        }
    }

    let footer = Span::styled(
        format!(" {} provider(s) ", state.filtered.len()),
        Style::default().fg(theme.text_muted),
    );
    frame.render_widget(Paragraph::new(footer), chunks[3]);
}

fn compute_scroll_offset(state: &ProviderDialogState, visible_lines: usize) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_new_state() {
        let state = ProviderDialogState::new();
        assert!(!state.open);
        assert_eq!(state.selected, 0);
        assert!(state.providers.len() >= 3);
    }

    #[test]
    fn test_open_close() {
        let mut state = ProviderDialogState::new();
        state.open();
        assert!(state.open);
        state.close();
        assert!(!state.open);
    }

    #[test]
    fn test_detect_providers() {
        let providers = ProviderDialogState::detect_providers();
        assert!(providers.len() >= 3);
        let names: Vec<&str> = providers.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Anthropic"));
        assert!(names.contains(&"OpenAI"));
    }

    #[test]
    fn test_esc_closes() {
        let mut state = ProviderDialogState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(action, ProviderAction::Close));
        assert!(!state.open);
    }

    #[test]
    fn test_ctrl_c_closes() {
        let mut state = ProviderDialogState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(action, ProviderAction::Close));
    }

    #[test]
    fn test_enter_opens_connect_for_disconnected() {
        let mut state = ProviderDialogState::new();
        state.open();

        let disconnected_idx = state
            .providers
            .iter()
            .position(|p| !p.configured)
            .unwrap_or(0);
        state.selected = state
            .filtered
            .iter()
            .position(|&f| f == disconnected_idx)
            .unwrap_or(0);

        let action = state.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));

        if state.providers[disconnected_idx].configured {
            assert!(matches!(action, ProviderAction::Toggle(_)));
        } else {
            assert!(state.connect_dialog.open);
            assert!(matches!(action, ProviderAction::None));
        }
    }

    #[test]
    fn test_navigation() {
        let mut state = ProviderDialogState::new();
        state.open();

        assert_eq!(state.selected, 0);
        state.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.selected, 1);
        state.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.selected, 2);
        state.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_search_filters() {
        let mut state = ProviderDialogState::new();
        state.open();

        let initial_count = state.filtered.len();

        state.search = "anthropic".to_string();
        state.selected = 0;
        state.apply_filter();

        assert!(state.filtered.len() <= initial_count);
        assert_eq!(state.providers[state.filtered[0]].name, "Anthropic");
    }

    #[test]
    fn test_d_opens_docs() {
        let mut state = ProviderDialogState::new();
        state.open();

        let action = state.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(action, ProviderAction::ViewDocs(0)));
    }

    #[test]
    fn test_page_navigation() {
        let mut state = ProviderDialogState::new();
        state.open();

        state.handle_key(key(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(
            state.selected,
            10.min(state.filtered.len().saturating_sub(1))
        );
    }

    #[test]
    fn test_status_icon() {
        assert_eq!(status_icon(&ProviderStatus::Connected), "\u{2713}");
        assert_eq!(status_icon(&ProviderStatus::Disconnected), "\u{25cb}");
        assert_eq!(status_icon(&ProviderStatus::Error), "\u{2717}");
    }

    #[test]
    fn test_provider_entry_has_models() {
        let providers = ProviderDialogState::detect_providers();
        for provider in &providers {
            assert!(!provider.models.is_empty());
        }
    }

    #[test]
    fn test_provider_has_kind() {
        let providers = ProviderDialogState::detect_providers();
        for provider in &providers {
            match provider.kind {
                ProviderKind::Anthropic
                | ProviderKind::OpenAi
                | ProviderKind::Xai
                | ProviderKind::QwenProxy
                | ProviderKind::Azure
                | ProviderKind::Gemini
                | ProviderKind::Bedrock
                | ProviderKind::OpenRouter
                | ProviderKind::Mistral
                | ProviderKind::Groq
                | ProviderKind::Unconfigured => {}
            }
        }
    }
}
