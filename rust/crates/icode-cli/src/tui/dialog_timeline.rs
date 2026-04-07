use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::app::{AppState, Message, MessagePart, MessageRole};
use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 50;
const MIN_HEIGHT: u16 = 12;

fn dialog_width(term_width: u16) -> u16 {
    (term_width / 2).clamp(MIN_WIDTH, 80)
}

fn dialog_height(term_height: u16) -> u16 {
    ((term_height as f64 * 0.6) as u16).clamp(MIN_HEIGHT, 30)
}

#[derive(Debug, Clone)]
pub struct TimelineEntry {
    pub idx: usize,
    pub role: String,
    pub agent: String,
    pub preview: String,
    pub tool_calls: usize,
    pub has_thinking: bool,
}

#[derive(Debug, Clone)]
pub struct TimelineDialogState {
    pub open: bool,
    pub entries: Vec<TimelineEntry>,
    pub selected: usize,
    pub scroll: usize,
}

impl TimelineDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
        }
    }

    pub fn open(&mut self, messages: &[Message]) {
        self.entries = build_entries(messages);
        self.selected = 0;
        self.scroll = 0;
        self.open = true;
    }

    pub fn open_with_entries(&mut self, entries: Vec<TimelineEntry>) {
        self.entries = entries;
        self.selected = 0;
        self.scroll = 0;
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> TimelineAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) | (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                TimelineAction::Cancel
            }
            (_, KeyCode::Enter) => {
                let selected_idx = self.entries.get(self.selected).map(|e| e.idx);
                self.close();
                if let Some(idx) = selected_idx {
                    TimelineAction::Jump(idx)
                } else {
                    TimelineAction::Cancel
                }
            }
            (_, KeyCode::Up) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                TimelineAction::None
            }
            (_, KeyCode::Down) => {
                if self.selected < self.entries.len().saturating_sub(1) {
                    self.selected += 1;
                }
                TimelineAction::None
            }
            (_, KeyCode::PageUp) => {
                self.selected = self.selected.saturating_sub(10);
                TimelineAction::None
            }
            (_, KeyCode::PageDown) => {
                self.selected = (self.selected + 10).min(self.entries.len().saturating_sub(1));
                TimelineAction::None
            }
            (_, KeyCode::Home) => {
                self.selected = 0;
                TimelineAction::None
            }
            (_, KeyCode::End) => {
                self.selected = self.entries.len().saturating_sub(1);
                TimelineAction::None
            }
            _ => TimelineAction::None,
        }
    }
}

impl Default for TimelineDialogState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum TimelineAction {
    None,
    Cancel,
    Jump(usize),
}

pub fn build_entries(messages: &[Message]) -> Vec<TimelineEntry> {
    messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            let (role, agent) = match &msg.role {
                MessageRole::User => ("User".to_string(), String::new()),
                MessageRole::Assistant => ("Assistant".to_string(), msg.agent.clone()),
                MessageRole::Tool { name } => ("Tool".to_string(), name.clone()),
            };

            let text = msg.full_text();
            let preview: String = text.chars().take(80).collect();

            let tool_calls = msg
                .parts
                .iter()
                .filter(|p| matches!(p, MessagePart::ToolCall { .. }))
                .count();

            let has_thinking = msg
                .parts
                .iter()
                .any(|p| matches!(p, MessagePart::Thinking { .. }));

            TimelineEntry {
                idx,
                role,
                agent,
                preview,
                tool_calls,
                has_thinking,
            }
        })
        .collect()
}

pub fn render_timeline_dialog(
    frame: &mut Frame,
    state: &mut TimelineDialogState,
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

    let total = state.entries.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(
            format!(" Timeline \u{2014} {total} message(s) "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(block.clone(), dialog_area);

    let inner = block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let list_area = chunks[0];
    let hint_area = chunks[1];
    let visible_lines = list_area.height as usize;

    // Ensure selected is visible
    if state.selected < state.scroll {
        state.scroll = state.selected;
    } else if visible_lines > 0 && state.selected >= state.scroll + visible_lines {
        state.scroll = state
            .selected
            .saturating_sub(visible_lines.saturating_sub(1));
    }

    if state.entries.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No messages yet",
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(empty, list_area);
    } else {
        for (i, entry) in state.entries.iter().enumerate().skip(state.scroll) {
            let line_idx = i - state.scroll;
            if line_idx >= visible_lines {
                break;
            }

            let is_selected = i == state.selected;
            let line_y = list_area.y + line_idx as u16;
            if line_y >= list_area.bottom() {
                break;
            }

            let line_area = Rect::new(list_area.x, line_y, list_area.width, 1);

            // Build the entry line
            let mut spans = Vec::new();

            // Selection marker
            let marker = if is_selected { "\u{25b6} " } else { "  " };
            let marker_style = if is_selected {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            spans.push(Span::styled(marker, marker_style));

            // Message number
            let num_style = if is_selected {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_muted)
            };
            spans.push(Span::styled(format!("#{:<3}", entry.idx + 1), num_style));

            // Role icon
            let role_icon = match entry.role.as_str() {
                "User" => "\u{1F464}",
                "Assistant" => "\u{1F916}",
                "Tool" => "\u{2699}",
                _ => "\u{2022}",
            };
            let role_style = if is_selected {
                Style::default().fg(theme.background).bg(theme.primary)
            } else if entry.role == "User" {
                Style::default().fg(theme.primary)
            } else if entry.role == "Assistant" {
                Style::default().fg(theme.success)
            } else {
                Style::default().fg(theme.text_muted)
            };
            spans.push(Span::styled(format!("{role_icon} "), role_style));

            // Agent name (for assistant)
            if !entry.agent.is_empty() && entry.role == "Assistant" {
                let agent_style = if is_selected {
                    Style::default().fg(theme.background).bg(theme.primary)
                } else {
                    Style::default().fg(theme.accent)
                };
                spans.push(Span::styled(format!("[{}] ", entry.agent), agent_style));
            }

            // Thinking indicator
            if entry.has_thinking {
                let think_style = if is_selected {
                    Style::default().fg(theme.background).bg(theme.primary)
                } else {
                    Style::default().fg(theme.warning)
                };
                spans.push(Span::styled("\u{1F4AD} ", think_style));
            }

            // Preview text
            let preview_style = if is_selected {
                Style::default().fg(theme.background).bg(theme.primary)
            } else {
                Style::default().fg(theme.text)
            };
            let preview = if entry.preview.is_empty() {
                if entry.role == "Tool" {
                    "(tool message)".to_string()
                } else {
                    "(empty)".to_string()
                }
            } else {
                let truncated: String = entry.preview.chars().take(60).collect();
                if entry.preview.chars().count() > 60 {
                    format!("{truncated}...")
                } else {
                    truncated
                }
            };
            spans.push(Span::styled(preview, preview_style));

            // Tool call badge
            if entry.tool_calls > 0 {
                let badge_style = if is_selected {
                    Style::default().fg(theme.background).bg(theme.primary)
                } else {
                    Style::default().fg(theme.warning)
                };
                spans.push(Span::styled(
                    format!(" \u{26A1}{}", entry.tool_calls),
                    badge_style,
                ));
            }

            frame.render_widget(Paragraph::new(Line::from(spans)), line_area);
        }
    }

    // Bottom hint
    let hint_text = " \u{2191}\u{2193} Navigate  \u{21B5} Jump  Esc Close  PgUp/PgDn Scroll  ";
    let hint = Span::styled(
        hint_text,
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), hint_area);
}
