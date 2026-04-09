use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::time::SystemTime;

use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 50;
const MIN_HEIGHT: u16 = 16;

fn dialog_width(term_width: u16) -> u16 {
    (term_width / 2)
        .max(MIN_WIDTH)
        .min(term_width.saturating_sub(4))
}

fn dialog_height(_term_height: u16) -> u16 {
    MIN_HEIGHT
}

const SHARE_BASE_URL: &str = "https://icode.example.com/share";

pub struct ShareDialogState {
    pub open: bool,
    pub is_shared: bool,
    pub share_id: Option<String>,
    pub share_url: Option<String>,
    pub include_thinking: bool,
    pub include_tool_details: bool,
    pub selected_button: usize,
    pub copied: bool,
}

impl ShareDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            is_shared: false,
            share_id: None,
            share_url: None,
            include_thinking: true,
            include_tool_details: true,
            selected_button: 0,
            copied: false,
        }
    }

    pub fn open(&mut self, session_id: &str) {
        self.open = true;
        self.copied = false;
        if !self.is_shared {
            self.share_id = None;
            self.share_url = None;
        }
        if let Some(ref sid) = self.share_id {
            if !sid.starts_with(session_id) {
                self.is_shared = false;
                self.share_id = None;
                self.share_url = None;
            }
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn share(&mut self, session_id: &str) {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        self.share_id = Some(format!("{session_id}-{timestamp}"));
        self.is_shared = true;
        self.update_url();
    }

    pub fn unshare(&mut self) {
        self.is_shared = false;
        self.share_id = None;
        self.share_url = None;
        self.copied = false;
    }

    pub fn get_url(&self) -> Option<String> {
        self.share_url.clone()
    }

    fn update_url(&mut self) {
        if let Some(ref share_id) = self.share_id {
            let mut url = format!("{SHARE_BASE_URL}/{share_id}");
            url.push_str(&format!(
                "?thinking={}&tools={}",
                self.include_thinking, self.include_tool_details
            ));
            self.share_url = Some(url);
        }
    }

    pub fn copy_url(&mut self) -> Option<String> {
        self.copied = true;
        self.share_url.clone()
    }

    pub fn toggle_thinking(&mut self) {
        self.include_thinking = !self.include_thinking;
        if self.is_shared {
            self.update_url();
        }
    }

    pub fn toggle_tool_details(&mut self) {
        self.include_tool_details = !self.include_tool_details;
        if self.is_shared {
            self.update_url();
        }
    }

    pub fn handle_key(
        &mut self,
        key: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> ShareAction {
        use crossterm::event::KeyCode::*;
        use crossterm::event::KeyModifiers;

        self.copied = false;

        match (modifiers, key) {
            (_, Esc) => {
                self.close();
                ShareAction::Close
            }
            (_, Enter) => match self.selected_button {
                0 => ShareAction::Share,
                1 => ShareAction::Copy,
                2 => ShareAction::Unshare,
                _ => ShareAction::Close,
            },
            (_, Left) => {
                if self.selected_button > 0 {
                    self.selected_button -= 1;
                }
                ShareAction::None
            }
            (_, Right) => {
                if self.selected_button < 2 {
                    self.selected_button += 1;
                }
                ShareAction::None
            }
            _ => ShareAction::None,
        }
    }

    pub fn toggle_option(&mut self, option_idx: usize) {
        match option_idx {
            0 => self.toggle_thinking(),
            1 => self.toggle_tool_details(),
            _ => {}
        }
    }

    pub const BUTTON_LABELS: &'static [&'static str] = &["Share", "Copy URL", "Unshare", "Close"];
}

impl Default for ShareDialogState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum ShareAction {
    None,
    Close,
    Share,
    Copy,
    Unshare,
}

pub fn render_share_dialog(
    frame: &mut Frame,
    state: &mut ShareDialogState,
    area: Rect,
    theme: Theme,
) {
    if !state.open {
        return;
    }

    let width = dialog_width(area.width);
    let height = dialog_height(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(
            " Share Session ",
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
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let session_label = state
        .share_id
        .as_ref()
        .map(|s| s.split('-').next().unwrap_or(""))
        .unwrap_or("current");
    let session_id_display = state.share_id.as_deref().unwrap_or("N/A");
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("Session: {session_label} (ID: {session_id_display})"),
            Style::default().fg(theme.text),
        )),
        chunks[0],
    );

    let status = if state.is_shared {
        Span::styled(
            "\u{25cf} Shared",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "\u{25cb} Not shared",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )
    };
    frame.render_widget(Paragraph::new(status), chunks[1]);

    if let Some(ref url) = state.share_url {
        let max_len = (width.saturating_sub(4)) as usize;
        let display_url = if url.len() > max_len {
            format!("{}...", &url[..max_len.saturating_sub(3)])
        } else {
            url.clone()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                display_url,
                Style::default()
                    .fg(theme.link)
                    .add_modifier(Modifier::UNDERLINED),
            )),
            chunks[2],
        );
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Share this session to generate a URL",
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )),
            chunks[2],
        );
    }

    let thinking_check = if state.include_thinking {
        "\u{2713}"
    } else {
        " "
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("[{thinking_check}] "),
                Style::default().fg(theme.primary),
            ),
            Span::styled("Include thinking", Style::default().fg(theme.text)),
        ])),
        chunks[3],
    );

    let tool_check = if state.include_tool_details {
        "\u{2713}"
    } else {
        " "
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("[{tool_check}] "),
                Style::default().fg(theme.primary),
            ),
            Span::styled("Include tool details", Style::default().fg(theme.text)),
        ])),
        chunks[4],
    );

    let buttons = render_buttons(state, theme);
    frame.render_widget(buttons, chunks[6]);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "\u{2190}\u{2192} navigate  \u{23ce} activate  \u{27f5} toggle options  Esc: close",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )),
        chunks[7],
    );
}

fn render_buttons(state: &ShareDialogState, theme: Theme) -> Paragraph<'static> {
    let mut spans = Vec::new();
    let labels = ShareDialogState::BUTTON_LABELS;

    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }

        let is_selected = state.selected_button == i;
        let is_unshare = i == 2;
        let is_copy = i == 1;

        let style = if is_selected {
            if is_unshare {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.error)
                    .add_modifier(Modifier::BOLD)
            } else if is_copy && state.copied {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            }
        } else if is_unshare {
            Style::default().fg(theme.error)
        } else if is_copy && state.copied {
            Style::default().fg(theme.success)
        } else {
            Style::default().fg(theme.text)
        };

        let display_label = if is_copy && state.copied {
            "Copied!"
        } else {
            label
        };

        spans.push(Span::styled(format!("[{display_label}]"), style));
    }

    Paragraph::new(Line::from(spans)).alignment(ratatui::layout::Alignment::Center)
}
