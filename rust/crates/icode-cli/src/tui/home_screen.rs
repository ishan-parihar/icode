use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::tui::theme::Theme;
use crate::tui::theme::Theme;

#[derive(Debug, Clone)]
pub struct HomeSessionEntry {
    pub id: String,
    pub title: String,
    pub model: String,
    pub message_count: usize,
    pub last_modified: u64,
}

#[derive(Debug, Clone)]
pub enum HomeAction {
    OpenSession(String),
    NewSession,
    Quit,
}

pub enum HomeKeyResult {
    Action(HomeAction),
    TypeChar(char),
    None,
}

pub struct HomeScreenState {
    pub active: bool,
    pub sessions: Vec<HomeSessionEntry>,
    pub selected: usize,
    pub scroll: usize,
    pub logo_lines: Vec<&'static str>,
}

impl HomeScreenState {
    pub fn new() -> Self {
        let logo_lines = vec![
            "╔══════════════════════════════════════════════╗",
            "║     ██╗ ██████╗ ██████╗ ██████╗ ███████╗     ║",
            "║     ╚═╝██╔════╝██╔═══██╗██╔══██╗██╔════╝     ║",
            "║     ██╗██║     ██║   ██║██║  ██║█████╗       ║",
            "║     ██║██║     ██║   ██║██║  ██║██╔══╝       ║",
            "║     ██║╚██████╗╚██████╔╝██████╔╝███████╗     ║",
            "║     ╚═╝ ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝     ║",
            "╚══════════════════════════════════════════════╝",
        ];
        let mut state = Self {
            active: true,
            sessions: Vec::new(),
            selected: 0,
            scroll: 0,
            logo_lines,
        };
        state.load_sessions();
        state
    }

    pub fn load_sessions(&mut self) {
        self.sessions.clear();
        let dir = sessions_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(iter) => iter,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !is_session_file(&path) {
                continue;
            }
            let filename_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            if filename_stem.ends_with(".meta") {
                continue;
            }

            let session_id = filename_stem;

            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let title = load_session_title(&session_id, &dir)
                .or_else(|| load_title_from_session(&path))
                .unwrap_or_else(|| "Untitled Session".into());

            let message_count = runtime::Session::load_from_path(&path)
                .map(|s| s.messages.len())
                .unwrap_or(0);

            let model = load_session_model(&session_id, &dir).unwrap_or_else(|| "unknown".into());

            self.sessions.push(HomeSessionEntry {
                id: session_id,
                title,
                model,
                message_count,
                last_modified: modified,
            });
        }

        self.sessions
            .sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
        self.sessions.truncate(10);
        self.selected = 0;
        self.scroll = 0;
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> HomeKeyResult {
        match key {
            crossterm::event::KeyCode::Enter => {
                let action = if self.sessions.is_empty() {
                    HomeAction::NewSession
                } else if let Some(session) = self.sessions.get(self.selected) {
                    HomeAction::OpenSession(session.id.clone())
                } else {
                    HomeAction::NewSession
                };
                HomeKeyResult::Action(action)
            }
            crossterm::event::KeyCode::Char('n') => HomeKeyResult::Action(HomeAction::NewSession),
            crossterm::event::KeyCode::Char('q') => HomeKeyResult::Action(HomeAction::Quit),
            crossterm::event::KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                HomeKeyResult::None
            }
            crossterm::event::KeyCode::Down => {
                if !self.sessions.is_empty() {
                    self.selected = (self.selected + 1).min(self.sessions.len() - 1);
                }
                HomeKeyResult::None
            }
            crossterm::event::KeyCode::Char(c) => {
                self.active = false;
                HomeKeyResult::TypeChar(c)
            }
            _ => HomeKeyResult::None,
        }
    }
}

impl Default for HomeScreenState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_home_screen(frame: &mut Frame, area: Rect, state: &HomeScreenState, theme: Theme) {
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme.background)),
        area,
    );

    if area.width < 40 || area.height < 10 {
        let minimal = Paragraph::new(Line::from(Span::styled(
            "icode",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(minimal, area);
        return;
    }

    let logo_height = state.logo_lines.len() as u16;
    let session_rows = state.sessions.len().min(10);
    let session_list_height = if session_rows > 0 {
        1 + session_rows as u16 + 1
    } else {
        3
    };

    let total_content = logo_height + 1 + 2 + 1 + session_list_height;
    let top_spacer = if area.height > total_content + 2 {
        (area.height - total_content) / 2
    } else {
        1
    };

    let mut y = area.top() + top_spacer;

    for (i, line) in state.logo_lines.iter().enumerate() {
        let line_width = line.len() as u16;
        let x = area.x + (area.width.saturating_sub(line_width)) / 2;
        let spans = build_logo_spans(line, theme);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.background)),
            Rect {
                x,
                y: y + i as u16,
                width: line_width.min(area.width),
                height: 1,
            },
        );
    }
    y += logo_height + 1;

    let tagline = Line::from(vec![Span::styled(
        "AI Coding Assistant",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    )]);
    let tagline_width = 19u16;
    frame.render_widget(
        Paragraph::new(tagline).style(Style::default().bg(theme.background)),
        Rect {
            x: area.x + (area.width.saturating_sub(tagline_width)) / 2,
            y,
            width: tagline_width,
            height: 1,
        },
    );
    y += 2;

    let header = Line::from(vec![Span::styled(
        "Recent Sessions:",
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(
        Paragraph::new(header).style(Style::default().bg(theme.background)),
        Rect {
            x: area.x + 2,
            y,
            width: area.width.saturating_sub(4),
            height: 1,
        },
    );
    y += 1;

    if state.sessions.is_empty() {
        let no_sessions = Line::from(vec![Span::styled(
            "No recent sessions \u{2014} type to start a new conversation",
            Style::default().fg(theme.text_muted),
        )]);
        frame.render_widget(
            Paragraph::new(no_sessions).style(Style::default().bg(theme.background)),
            Rect {
                x: area.x + 4,
                y,
                width: area.width.saturating_sub(8),
                height: 1,
            },
        );
    } else {
        let list_width = area.width.saturating_sub(4);
        let max_visible = ((area.height.saturating_sub(y + 1)) as usize).min(state.sessions.len());

        for (i, session) in state.sessions.iter().take(max_visible).enumerate() {
            let is_selected = i == state.selected;
            let time_str = format_relative_time(session.last_modified);

            let title_max = list_width.saturating_sub(16) as usize;
            let truncated_title = if session.title.len() > title_max {
                format!("{}...", &session.title[..title_max.saturating_sub(3)])
            } else {
                session.title.clone()
            };

            let (marker_style, title_style, time_style) = if is_selected {
                (
                    Style::default()
                        .fg(theme.primary)
                        .bg(theme.background_element)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.text)
                        .bg(theme.background_element)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.text_muted)
                        .bg(theme.background_element),
                )
            } else {
                (
                    Style::default().fg(theme.text_muted).bg(theme.background),
                    Style::default().fg(theme.text_muted).bg(theme.background),
                    Style::default().fg(theme.text_muted).bg(theme.background),
                )
            };

            let marker = if is_selected { "\u{25b6} " } else { "  " };

            let row_line = Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(truncated_title, title_style),
                Span::styled(" ", title_style),
                Span::styled(time_str, time_style),
            ]);

            frame.render_widget(
                Paragraph::new(row_line).style(Style::default().bg(theme.background)),
                Rect {
                    x: area.x + 2,
                    y: y + i as u16,
                    width: list_width,
                    height: 1,
                },
            );
        }
    }

    let hints = build_key_hints(theme);
    let hints_y = area.bottom().saturating_sub(1);
    let hints_width = hints
        .iter()
        .map(|s| s.content.chars().count())
        .sum::<usize>() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(hints)).style(Style::default().bg(theme.background)),
        Rect {
            x: area.x + (area.width.saturating_sub(hints_width)) / 2,
            y: hints_y,
            width: hints_width.min(area.width),
            height: 1,
        },
    );
}

fn build_logo_spans(line: &str, theme: Theme) -> Vec<Span<'static>> {
    let i_color = tint_color(theme.background, theme.primary, 0.35);
    let fg = theme.text;
    let mut spans = Vec::new();
    let mut current_i = String::new();
    let mut current_other = String::new();
    for (idx, ch) in line.chars().enumerate() {
        let is_i = idx >= 5 && idx <= 7;
        if is_i {
            if !current_other.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut current_other),
                    Style::default().fg(fg),
                ));
            }
            current_i.push(ch);
        } else {
            if !current_i.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut current_i),
                    Style::default().fg(i_color),
                ));
            }
            current_other.push(ch);
        }
    }
    if !current_other.is_empty() {
        spans.push(Span::styled(current_other, Style::default().fg(fg)));
    }
    if !current_i.is_empty() {
        spans.push(Span::styled(current_i, Style::default().fg(i_color)));
    }
    spans
}

fn build_key_hints(theme: Theme) -> Vec<Span<'static>> {
    let bg = theme.background_element;
    fn kbd(
        label: &str,
        action: &str,
        bg: ratatui::style::Color,
        text_color: ratatui::style::Color,
    ) -> Vec<Span<'static>> {
        vec![
            Span::styled(
                format!("\u{250c}\u{2500}{label}\u{2500}\u{2510}"),
                Style::default().fg(text_color).bg(bg),
            ),
            Span::styled(format!(" {action}  "), Style::default().fg(text_color)),
        ]
    }

    let mut spans = Vec::new();
    spans.extend(kbd("\u{2191}/\u{2193}", "navigate", bg, theme.text_muted));
    spans.extend(kbd("Enter", "open", bg, theme.text_muted));
    spans.extend(kbd("n", "new", bg, theme.text_muted));
    spans.extend(kbd("q", "quit", bg, theme.text_muted));
    spans
}

fn sessions_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    let path = cwd.join(".icode").join("sessions");
    let _ = std::fs::create_dir_all(&path);
    path
}

fn is_session_file(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "jsonl" || ext == "json")
}

fn session_meta_path(id: &str, dir: &PathBuf) -> PathBuf {
    dir.join(format!("{id}.meta.json"))
}

fn load_session_title(id: &str, dir: &PathBuf) -> Option<String> {
    let meta_path = session_meta_path(id, dir);
    if meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                return json.get("title").and_then(|v| v.as_str()).map(String::from);
            }
        }
    }
    None
}

fn load_title_from_session(path: &PathBuf) -> Option<String> {
    if let Ok(session) = runtime::Session::load_from_path(path) {
        for msg in &session.messages {
            if matches!(msg.role, runtime::MessageRole::User) {
                for block in &msg.blocks {
                    if let runtime::ContentBlock::Text { text } = block {
                        if !text.is_empty() {
                            if text.len() > 60 {
                                return Some(format!("{}...", &text[..57]));
                            }
                            return Some(text.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

fn load_session_model(id: &str, dir: &PathBuf) -> Option<String> {
    let meta_path = session_meta_path(id, dir);
    if meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                return json.get("model").and_then(|v| v.as_str()).map(String::from);
            }
        }
    }
    None
}

fn format_relative_time(epoch_secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(epoch_secs);
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}
