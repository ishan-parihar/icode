use crate::tui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone)]
pub struct ForkResult {
    pub session_id: String,
}

pub struct ForkDialogState {
    pub open: bool,
    pub message_idx: usize,
    pub message_preview: String,
    pub selected: usize,
}

impl ForkDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            message_idx: 0,
            message_preview: String::new(),
            selected: 0,
        }
    }

    pub fn open(&mut self, msg_idx: usize, msg_preview: String) {
        self.open = true;
        self.message_idx = msg_idx;
        self.message_preview = msg_preview;
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> Option<ForkResult> {
        use crossterm::event::KeyCode;
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.close();
                None
            }
            KeyCode::Up | KeyCode::Left => {
                self.selected = 0;
                None
            }
            KeyCode::Down | KeyCode::Right => {
                self.selected = 1;
                None
            }
            KeyCode::Enter => {
                if self.selected == 0 {
                    let session_id = generate_session_id();
                    self.close();
                    Some(ForkResult { session_id })
                } else {
                    self.close();
                    None
                }
            }
            KeyCode::Char('1') => {
                let session_id = generate_session_id();
                self.close();
                Some(ForkResult { session_id })
            }
            KeyCode::Char('2') => {
                self.close();
                None
            }
            _ => None,
        }
    }
}

fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now();
    let ts = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let millis = ts.as_millis();
    let micros = ts.as_micros();
    let rand_part = (micros ^ (micros >> 16)) as u64 % 10000;
    format!("{millis}-{rand_part:04}")
}

fn dialog_width(screen_width: u16) -> u16 {
    let w = (screen_width as f32 * 0.45) as u16;
    w.clamp(36, 56)
}

fn dialog_height(_screen_height: u16) -> u16 {
    14
}

pub fn render_fork_dialog(frame: &mut Frame, state: &ForkDialogState, area: Rect, theme: Theme) {
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
        .border_style(Style::default().fg(theme.primary))
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " Fork Session ",
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
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Branching from message:",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        chunks[0],
    );

    let max_preview = (width as usize).saturating_sub(8);
    let preview_text = if state.message_preview.len() > max_preview {
        let truncated = truncate_to_words(&state.message_preview, max_preview.saturating_sub(4));
        format!("  \"{truncated}...\"")
    } else {
        format!("  \"{}\"", state.message_preview)
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            preview_text,
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )),
        chunks[1],
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            "This creates a new session sharing history up to this point.",
            Style::default().fg(theme.text_muted),
        )),
        chunks[2],
    );

    let btn_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[3]);

    render_button(
        frame,
        btn_chunks[0],
        "[Confirm]",
        state.selected == 0,
        theme,
    );
    render_button(frame, btn_chunks[1], "[Cancel]", state.selected == 1, theme);

    let hint = Span::styled(
        "1/Enter: fork  \u{2022}  2/Esc: cancel  \u{2022}  \u{2190}/\u{2192}: navigate",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[4]);
}

fn render_button(frame: &mut Frame, area: Rect, label: &str, selected: bool, theme: Theme) {
    let style = if selected {
        Style::default()
            .fg(theme.background)
            .bg(theme.primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_muted)
    };
    let btn = Paragraph::new(Line::from(Span::styled(label, style))).alignment(Alignment::Center);
    frame.render_widget(btn, area);
}

fn truncate_to_words(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let mut result = String::with_capacity(max_chars);
    for word in text.split_whitespace() {
        if result.len() + word.len() + 1 > max_chars {
            break;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    if result.is_empty() && !text.is_empty() {
        return text.chars().take(max_chars).collect();
    }
    result
}
