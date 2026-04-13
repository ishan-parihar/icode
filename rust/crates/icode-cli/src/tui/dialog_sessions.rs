use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::popup_utils::PopupConfig;
use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 16;

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

#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub modified: u128,
    pub message_count: usize,
    pub parent_id: Option<String>,
    pub branch_name: Option<String>,
    pub usage_summary: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionsDialogState {
    pub open: bool,
    pub sessions: Vec<SessionEntry>,
    pub filtered: Vec<SessionEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub search: String,
    pub delete_confirm: Option<String>,
}

impl SessionsDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            sessions: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            search: String::new(),
            delete_confirm: None,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.scroll_offset = 0;
        self.search.clear();
        self.delete_confirm = None;
        self.apply_filter();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn load_sessions(&mut self) {
        self.sessions.clear();
        if let Ok(dir) = sessions_dir() {
            let entries: Vec<_> = std::fs::read_dir(&dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(std::result::Result::ok)
                .collect();
            let usage_budget = 5usize;
            for (idx, entry) in entries.iter().enumerate() {
                let path = entry.path();
                if !is_session_file(&path) {
                    continue;
                }
                let metadata = entry.metadata().ok();
                let modified = metadata
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis())
                    .unwrap_or_default();
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let (msg_count, parent_id, branch_name, usage_summary, title) =
                    match runtime::Session::load_from_path(&path) {
                        Ok(session) => {
                            let parent_id =
                                session.fork.as_ref().map(|f| f.parent_session_id.clone());
                            let branch_name =
                                session.fork.as_ref().and_then(|f| f.branch_name.clone());
                            let mc = session.messages.len();
                            let title = session.title.clone();
                            let usage_summary = if idx < usage_budget {
                                compute_usage_summary(&session)
                            } else {
                                Some(format!("{mc} msgs"))
                            };
                            (mc, parent_id, branch_name, usage_summary, title)
                        }
                        Err(_) => (0, None, None, None, id.clone()),
                    };
                self.sessions.push(SessionEntry {
                    id,
                    title,
                    path,
                    modified,
                    message_count: msg_count,
                    parent_id,
                    branch_name,
                    usage_summary,
                });
            }
        }
        self.sessions
            .sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| b.id.cmp(&a.id)));
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        if self.search.is_empty() {
            self.filtered = self.sessions.clone();
        } else {
            let q = self.search.to_lowercase();
            self.filtered = self
                .sessions
                .iter()
                .filter(|s| {
                    s.id.to_lowercase().contains(&q)
                        || s.title.to_lowercase().contains(&q)
                        || s.branch_name
                            .as_ref()
                            .is_some_and(|b| b.to_lowercase().contains(&q))
                })
                .cloned()
                .collect();
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> SessionAction {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.close();
                SessionAction::Close
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                SessionAction::Close
            }
            (_, KeyCode::Char('/')) => {
                self.search.clear();
                SessionAction::StartSearch
            }
            (_, KeyCode::Enter) => {
                if let Some(session) = self.filtered.get(self.selected) {
                    if let Some(ref confirm_id) = self.delete_confirm {
                        if confirm_id == &session.id {
                            let _ = std::fs::remove_file(&session.path);
                            self.delete_confirm = None;
                            self.load_sessions();
                            return SessionAction::Close;
                        }
                    }
                    let path = session.path.clone();
                    self.close();
                    SessionAction::Switch(path)
                } else {
                    SessionAction::None
                }
            }
            (_, KeyCode::Up) => {
                self.delete_confirm = None;
                if self.selected > 0 {
                    self.selected -= 1;
                }
                SessionAction::None
            }
            (_, KeyCode::Down) => {
                self.delete_confirm = None;
                if self.selected < self.filtered.len().saturating_sub(1) {
                    self.selected += 1;
                }
                SessionAction::None
            }
            (_, KeyCode::PageUp) => {
                self.delete_confirm = None;
                self.selected = self.selected.saturating_sub(10);
                SessionAction::None
            }
            (_, KeyCode::PageDown) => {
                self.delete_confirm = None;
                self.selected = (self.selected + 10).min(self.filtered.len().saturating_sub(1));
                SessionAction::None
            }
            (_, KeyCode::Home) => {
                self.delete_confirm = None;
                self.selected = 0;
                SessionAction::None
            }
            (_, KeyCode::End) => {
                self.delete_confirm = None;
                self.selected = self.filtered.len().saturating_sub(1);
                SessionAction::None
            }
            (_, KeyCode::Backspace) => {
                if !self.search.is_empty() {
                    self.search.pop();
                    self.selected = 0;
                    self.apply_filter();
                }
                SessionAction::None
            }
            (_, KeyCode::Delete) | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                if let Some(session) = self.filtered.get(self.selected) {
                    if let Some(ref confirm_id) = self.delete_confirm {
                        if confirm_id == &session.id {
                            let _ = std::fs::remove_file(&session.path);
                            self.delete_confirm = None;
                            self.load_sessions();
                            return SessionAction::Close;
                        }
                    }
                    self.delete_confirm = Some(session.id.clone());
                }
                SessionAction::None
            }
            (_, KeyCode::Char(c)) => {
                self.delete_confirm = None;
                self.search.push(c);
                self.selected = 0;
                self.apply_filter();
                SessionAction::None
            }
            _ => SessionAction::None,
        }
    }
}

#[derive(Debug)]
pub enum SessionAction {
    None,
    Close,
    StartSearch,
    Switch(PathBuf),
}

fn sessions_dir() -> std::io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let path = cwd.join(".icode").join("sessions");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn is_session_file(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "jsonl" || ext == "json")
}

fn compute_usage_summary(session: &runtime::Session) -> Option<String> {
    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;
    let mut total_cache_create: u64 = 0;
    let mut total_cache_read: u64 = 0;
    let mut has_usage = false;

    for msg in &session.messages {
        if let Some(usage) = msg.usage {
            has_usage = true;
            total_input += usage.input_tokens as u64;
            total_output += usage.output_tokens as u64;
            total_cache_create += usage.cache_creation_input_tokens as u64;
            total_cache_read += usage.cache_read_input_tokens as u64;
        }
    }

    if !has_usage {
        return None;
    }

    let total_tokens = total_input + total_output + total_cache_create + total_cache_read;
    let pricing = runtime::pricing_for_model("sonnet")
        .unwrap_or_else(runtime::ModelPricing::default_sonnet_tier);
    let cost = (total_input as f64 * pricing.input_cost_per_million
        + total_output as f64 * pricing.output_cost_per_million
        + total_cache_create as f64 * pricing.cache_creation_cost_per_million
        + total_cache_read as f64 * pricing.cache_read_cost_per_million)
        / 1_000_000.0;

    if cost > 0.01 {
        Some(format!("{:.0}k tok, ${:.2}", total_tokens / 1000, cost))
    } else {
        Some(format!("{:.0}k tok", total_tokens / 1000))
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

fn category_label(epoch_millis: u128) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let diff = now.saturating_sub(epoch_millis);
    let days = diff / 86_400_000;
    if days == 0 {
        "Today".to_string()
    } else if days == 1 {
        "Yesterday".to_string()
    } else if days < 7 {
        format!("{days} days ago")
    } else {
        format!("{} weeks ago", days / 7)
    }
}

pub fn render_sessions_dialog(
    frame: &mut Frame,
    state: &mut SessionsDialogState,
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

    let config = PopupConfig::full("Sessions");
    let block = config.to_block(theme);

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
        "Enter: switch  •  Ctrl+D: delete  •  /: search  •  Esc: close",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    let list_area = chunks[2];
    let visible_lines = list_area.height as usize;

    if state.filtered.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No sessions found",
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(empty, list_area);
    } else {
        let scroll_offset = compute_scroll_offset(state, visible_lines);
        state.scroll_offset = scroll_offset;

        let mut current_category = String::new();
        for (i, session) in state.filtered.iter().enumerate().skip(scroll_offset) {
            let line_idx = i - scroll_offset;
            if line_idx >= visible_lines {
                break;
            }

            let cat = category_label(session.modified);
            if cat != current_category {
                current_category = cat.clone();
                let line_y = list_area.y + line_idx as u16;
                if line_y < list_area.bottom() {
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!("── {cat} ──"),
                            Style::default().fg(theme.border),
                        )),
                        Rect::new(list_area.x, line_y, list_area.width, 1),
                    );
                }
                continue;
            }

            let is_selected = i == state.selected;
            let is_deleting = state.delete_confirm.as_ref() == Some(&session.id);
            let time_str = format_relative_time(session.modified);

            let mut title_spans = vec![Span::raw(" ")];

            if is_deleting {
                title_spans.push(Span::styled(
                    format!("⚠ Delete {}? (Ctrl+D to confirm) ", session.id),
                    Style::default()
                        .fg(theme.error)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                let primary_color = if is_selected {
                    theme.background
                } else {
                    theme.text
                };
                let muted_color = if is_selected {
                    theme.background
                } else {
                    theme.text_muted
                };

                if session.title.starts_with("New session - ") {
                    title_spans.push(Span::styled(
                        truncate(&session.id, 20),
                        Style::default().fg(primary_color),
                    ));
                } else {
                    title_spans.push(Span::styled(
                        truncate(&session.title, 32),
                        Style::default().fg(primary_color),
                    ));
                    title_spans.push(Span::styled(
                        format!("({}) ", truncate(&session.id, 12)),
                        Style::default().fg(muted_color),
                    ));
                }
                title_spans.push(Span::styled(
                    format!(" ({} msgs) ", session.message_count),
                    Style::default().fg(muted_color),
                ));
                if let Some(ref branch) = session.branch_name {
                    title_spans.push(Span::styled(
                        format!("on {} ", truncate(branch, 16)),
                        Style::default().fg(if is_selected {
                            theme.background
                        } else {
                            Color::Yellow
                        }),
                    ));
                }
                if let Some(ref parent) = session.parent_id {
                    title_spans.push(Span::styled(
                        format!("↪{} ", truncate(parent, 12)),
                        Style::default().fg(if is_selected {
                            theme.background
                        } else {
                            Color::DarkGray
                        }),
                    ));
                }
                if let Some(ref usage) = session.usage_summary {
                    title_spans.push(Span::styled(
                        format!("[{usage}] "),
                        Style::default().fg(if is_selected {
                            theme.background
                        } else {
                            theme.text_muted
                        }),
                    ));
                }
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
                let footer_style = if is_deleting {
                    Style::default().fg(theme.error)
                } else if is_selected {
                    Style::default().fg(theme.background).bg(theme.primary)
                } else {
                    Style::default().fg(theme.text_muted)
                };
                if list_area.width > 16 {
                    frame.render_widget(
                        Paragraph::new(time_str).style(footer_style),
                        Rect::new(list_area.x + list_area.width - 12, line_y, 12, 1),
                    );
                }
            }
        }
    }

    let footer = Span::styled(
        format!(" {} session(s) ", state.filtered.len()),
        Style::default().fg(theme.text_muted),
    );
    frame.render_widget(Paragraph::new(footer), chunks[3]);
}

fn compute_scroll_offset(state: &SessionsDialogState, visible_lines: usize) -> usize {
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

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        "...".to_string()
    } else {
        format!("{}...", s.chars().take(max_len - 3).collect::<String>())
    }
}
