use crate::tui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageAction {
    Revert,
    Copy,
    Fork,
}

pub struct MessageActionDialogState {
    pub open: bool,
    pub message_id: usize,
    pub message_content: String,
    pub selected: usize,
    pub options: Vec<(MessageAction, &'static str, &'static str)>,
}

impl MessageActionDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            message_id: 0,
            message_content: String::new(),
            selected: 0,
            options: vec![
                (
                    MessageAction::Revert,
                    "Revert",
                    "undo messages and file changes",
                ),
                (MessageAction::Copy, "Copy", "message text to clipboard"),
                (
                    MessageAction::Fork,
                    "Fork",
                    "create a new session from here",
                ),
            ],
        }
    }

    pub fn open(&mut self, message_id: usize, content: String) {
        self.open = true;
        self.message_id = message_id;
        self.message_content = content;
        self.selected = 0;
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
                if self.selected < self.options.len().saturating_sub(1) {
                    self.selected += 1;
                }
                None
            }
            KeyCode::Enter => {
                let action = self.options[self.selected].0.clone();
                self.close();
                Some(action)
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
            _ => None,
        }
    }
}

fn dialog_width(screen_width: u16) -> u16 {
    let w = (screen_width as f32 * 0.5) as u16;
    w.clamp(30, 60)
}

fn dialog_height(_screen_height: u16) -> u16 {
    12
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

    let width = dialog_width(area.width).min(area.width.saturating_sub(4));
    let height = dialog_height(area.height).min(area.height.saturating_sub(4));
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_active))
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " Message Actions ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center);

    frame.render_widget(block.clone(), dialog_area);

    let inner = block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let preview = if state.message_content.len() > (width as usize).saturating_sub(6) {
        format!(
            "  \"{}...\"",
            &state.message_content[..(width as usize).saturating_sub(10)]
        )
    } else {
        format!("  \"{}\"", state.message_content)
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            preview,
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )),
        chunks[0],
    );

    let option_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            state
                .options
                .iter()
                .map(|_| Constraint::Length(1))
                .collect::<Vec<_>>(),
        )
        .split(chunks[1]);

    for (i, (action, label, desc)) in state.options.iter().enumerate() {
        let _ = action;
        let is_selected = i == state.selected;
        let num = i + 1;
        let line = if is_selected {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{num}. "),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    label.to_string(),
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" - {desc}"), Style::default().fg(theme.text_muted)),
            ])
        } else {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{num}. "), Style::default().fg(theme.text_muted)),
                Span::styled(label.to_string(), Style::default().fg(theme.text)),
                Span::styled(format!(" - {desc}"), Style::default().fg(theme.text_muted)),
            ])
        };
        let line_area = Rect {
            x: option_chunks[i].x,
            y: option_chunks[i].y,
            width: option_chunks[i].width,
            height: 1,
        };
        if is_selected {
            frame.render_widget(
                Paragraph::new(line).style(Style::default().bg(theme.background_element)),
                line_area,
            );
        } else {
            frame.render_widget(Paragraph::new(line), line_area);
        }
    }

    let hint = Span::styled(
        "1/2/3: select  •  Enter: confirm  •  Esc: close",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[2]);
}
