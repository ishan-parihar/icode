use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 14;

fn dialog_width(term_width: u16) -> u16 {
    if term_width >= 128 {
        100
    } else if term_width >= 96 {
        80
    } else {
        MIN_WIDTH
    }
}

fn dialog_height(term_height: u16) -> u16 {
    (term_height / 2).saturating_sub(6).max(MIN_HEIGHT)
}

#[derive(Debug, Clone)]
pub struct BranchEntry {
    pub id: String,
    pub path: PathBuf,
    pub parent_id: Option<String>,
    pub branch_name: Option<String>,
    pub message_count: usize,
    pub modified: u128,
    pub is_current: bool,
    pub sub_agent_count: usize,
}

#[derive(Debug, Clone)]
pub struct SessionBranchingState {
    pub open: bool,
    pub branches: Vec<BranchEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub delete_confirm: Option<String>,
    pub current_session_id: String,
}

impl SessionBranchingState {
    pub fn new() -> Self {
        Self {
            open: false,
            branches: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            delete_confirm: None,
            current_session_id: String::new(),
        }
    }

    pub fn open(&mut self, current_session_id: &str) {
        self.open = true;
        self.current_session_id = current_session_id.to_string();
        self.selected = 0;
        self.scroll_offset = 0;
        self.delete_confirm = None;
        self.load_branches();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn load_branches(&mut self) {
        self.branches.clear();

        let Ok(dir) = sessions_dir() else {
            return;
        };

        let mut all_sessions = Vec::new();

        for entry in std::fs::read_dir(&dir).ok().into_iter().flatten() {
            let Ok(entry) = entry else { continue };
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

            let (msg_count, parent_id, branch_name, sub_agent_count) =
                match runtime::Session::load_from_path(&path) {
                    Ok(session) => (
                        session.messages.len(),
                        session.fork.as_ref().map(|f| f.parent_session_id.clone()),
                        session.fork.as_ref().and_then(|f| f.branch_name.clone()),
                        0,
                    ),
                    Err(_) => (0, None, None, 0),
                };

            let is_current = id == self.current_session_id;

            all_sessions.push(BranchEntry {
                id,
                path,
                parent_id,
                branch_name,
                message_count: msg_count,
                modified,
                is_current,
                sub_agent_count,
            });
        }

        all_sessions.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| b.id.cmp(&a.id)));

        self.branches = all_sessions;
        self.selected = self.selected.min(self.branches.len().saturating_sub(1));
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> BranchingAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) | (_, KeyCode::Char('q')) => {
                self.close();
                BranchingAction::Close
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                BranchingAction::Close
            }
            (_, KeyCode::Enter) => {
                if let Some(branch) = self.branches.get(self.selected) {
                    if let Some(ref confirm_id) = self.delete_confirm {
                        if confirm_id == &branch.id {
                            let _ = std::fs::remove_file(&branch.path);
                            self.delete_confirm = None;
                            self.load_branches();
                            return BranchingAction::Close;
                        }
                    }
                    let path = branch.path.clone();
                    self.close();
                    BranchingAction::Switch(path)
                } else {
                    BranchingAction::None
                }
            }
            (_, KeyCode::Up) => {
                self.delete_confirm = None;
                if self.selected > 0 {
                    self.selected -= 1;
                }
                BranchingAction::None
            }
            (_, KeyCode::Down) => {
                self.delete_confirm = None;
                if self.selected < self.branches.len().saturating_sub(1) {
                    self.selected += 1;
                }
                BranchingAction::None
            }
            (_, KeyCode::PageUp) => {
                self.delete_confirm = None;
                self.selected = self.selected.saturating_sub(10);
                BranchingAction::None
            }
            (_, KeyCode::PageDown) => {
                self.delete_confirm = None;
                self.selected = (self.selected + 10).min(self.branches.len().saturating_sub(1));
                BranchingAction::None
            }
            (_, KeyCode::Delete) | (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                if let Some(branch) = self.branches.get(self.selected) {
                    if branch.is_current {
                        return BranchingAction::None;
                    }
                    if let Some(ref confirm_id) = self.delete_confirm {
                        if confirm_id == &branch.id {
                            let _ = std::fs::remove_file(&branch.path);
                            self.delete_confirm = None;
                            self.load_branches();
                            return BranchingAction::Close;
                        }
                    }
                    self.delete_confirm = Some(branch.id.clone());
                }
                BranchingAction::None
            }
            (_, KeyCode::Char('n')) => {
                self.delete_confirm = None;
                BranchingAction::NewBranch
            }
            _ => BranchingAction::None,
        }
    }
}

#[derive(Debug)]
pub enum BranchingAction {
    None,
    Close,
    Switch(PathBuf),
    NewBranch,
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

pub fn render_session_branching(
    frame: &mut Frame,
    state: &mut SessionBranchingState,
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
            " Session Branches ",
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
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let hint = Span::styled(
        "Enter: switch  •  n: new branch  •  Ctrl+D: delete  •  Esc: close",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[0]);

    let list_area = chunks[1];
    let visible_lines = list_area.height as usize;

    if state.branches.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "No branches found",
            Style::default().fg(theme.text_muted),
        ));
        frame.render_widget(empty, list_area);
    } else {
        let scroll_offset = compute_scroll_offset(state, visible_lines);
        state.scroll_offset = scroll_offset;

        for (i, branch) in state.branches.iter().enumerate().skip(scroll_offset) {
            let line_idx = i - scroll_offset;
            if line_idx >= visible_lines {
                break;
            }

            let is_selected = i == state.selected;
            let is_deleting = state.delete_confirm.as_ref() == Some(&branch.id);
            let time_str = format_relative_time(branch.modified);

            let prefix = if branch.parent_id.is_some() {
                "  \u{2514}\u{2500} "
            } else {
                "\u{25cf} "
            };

            let branch_label = branch.branch_name.as_deref().unwrap_or(&branch.id);

            let title = if is_deleting {
                format!("{}Press Ctrl+D again to delete {}", prefix, branch.id)
            } else {
                let current_marker = if branch.is_current { " [current]" } else { "" };
                let sub_agents_info = if branch.sub_agent_count > 0 {
                    format!(
                        " [{} agent{}]",
                        branch.sub_agent_count,
                        if branch.sub_agent_count == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                };
                format!(
                    "{}{} ({} msgs){}{}",
                    prefix, branch_label, branch.message_count, sub_agents_info, current_marker
                )
            };

            let style = if is_deleting {
                Style::default()
                    .fg(theme.error)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default()
                    .fg(theme.background)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else if branch.is_current {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let line_y = list_area.y + line_idx as u16;
            if line_y < list_area.bottom() {
                let line_area = Rect::new(list_area.x, line_y, list_area.width, 1);
                frame.render_widget(Paragraph::new(title).style(style), line_area);

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
        format!(" {} branch(es) ", state.branches.len()),
        Style::default().fg(theme.text_muted),
    );
    frame.render_widget(Paragraph::new(footer), chunks[2]);
}

fn compute_scroll_offset(state: &SessionBranchingState, visible_lines: usize) -> usize {
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
