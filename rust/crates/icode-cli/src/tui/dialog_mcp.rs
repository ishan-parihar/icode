use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

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

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum McpServerStatus {
    Connected,
    Disconnected,
    Error(String),
    Starting,
}

#[derive(Debug, Clone)]
pub struct McpServerEntry {
    pub name: String,
    pub transport: String, // "stdio", "sse", "http", "ws"
    pub scope: String,     // "project", "user", "global"
    pub status: McpServerStatus,
    pub tool_count: usize,
}

// ---------------------------------------------------------------------------
// Dialog state
// ---------------------------------------------------------------------------

pub struct McpDialogState {
    pub open: bool,
    pub servers: Vec<McpServerEntry>,
    pub cursor: usize,
    pub search: String,
    pub filtered: Vec<usize>,
}

impl McpDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            servers: Vec::new(),
            cursor: 0,
            search: String::new(),
            filtered: Vec::new(),
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.search.clear();
        self.cursor = 0;
        self.rebuild_filtered();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Toggle the selected server's connected/disconnected state.
    pub fn toggle_server(&mut self) {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            if let Some(server) = self.servers.get_mut(idx) {
                server.status = match &server.status {
                    McpServerStatus::Connected => McpServerStatus::Disconnected,
                    _ => McpServerStatus::Connected,
                };
            }
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor + 1 < self.filtered.len() {
            self.cursor += 1;
        }
    }

    pub fn type_char(&mut self, c: char) {
        self.search.push(c);
        self.cursor = 0;
        self.rebuild_filtered();
    }

    pub fn backspace(&mut self) {
        self.search.pop();
        self.cursor = 0;
        self.rebuild_filtered();
    }

    pub fn rebuild_filtered(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered.clear();

        if query.is_empty() {
            self.filtered = (0..self.servers.len()).collect();
        } else {
            self.filtered = self
                .servers
                .iter()
                .enumerate()
                .filter(|(_, s)| server_matches(s, &query))
                .map(|(i, _)| i)
                .collect();
        }

        if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
        }
    }
}

impl Default for McpDialogState {
    fn default() -> Self {
        Self::new()
    }
}

fn server_matches(server: &McpServerEntry, query: &str) -> bool {
    format!("{} {} {}", server.name, server.transport, server.scope)
        .to_lowercase()
        .contains(query)
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

pub fn render_mcp_dialog(frame: &mut Frame, state: &McpDialogState, area: Rect, theme: Theme) {
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
        .title(" MCP Servers ")
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(block, dialog_area);

    let inner = dialog_area.inner(Margin::new(1, 1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search bar
            Constraint::Min(1),    // server list
            Constraint::Length(1), // help footer
        ])
        .split(inner);

    // --- Search bar ---
    let search_prompt = Span::styled("/ ", Style::default().fg(theme.accent));
    let search_text = if state.search.is_empty() {
        Span::styled("Filter servers...", Style::default().fg(theme.text_muted))
    } else {
        Span::styled(&state.search, Style::default().fg(theme.text))
    };
    let search_para = Paragraph::new(Line::from(vec![search_prompt, search_text]));
    frame.render_widget(search_para, chunks[0]);

    // --- Server list ---
    let scroll_offset = compute_scroll_offset(state, chunks[1].height as usize);
    let mut lines: Vec<Line> = Vec::new();

    for (pos, &server_idx) in state.filtered.iter().enumerate() {
        if pos < scroll_offset {
            continue;
        }
        if lines.len() >= chunks[1].height as usize {
            break;
        }

        let server = &state.servers[server_idx];
        let is_selected = pos == state.cursor;

        let status_icon = status_icon(&server.status);
        let status_style = status_style(&server.status, &theme);
        let name_style = if is_selected {
            Style::default()
                .bg(theme.background_hover)
                .fg(theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            status_style
        };

        let meta_style = if is_selected {
            Style::default()
                .bg(theme.background_hover)
                .fg(theme.text_muted)
        } else {
            Style::default().fg(theme.text_muted)
        };

        let cursor_marker = if is_selected {
            Span::styled("\u{25b6} ", Style::default().fg(theme.accent))
        } else {
            Span::raw("  ")
        };

        lines.push(Line::from(vec![
            cursor_marker,
            Span::styled(status_icon, status_style),
            Span::raw(" "),
            Span::styled(&server.name, name_style),
            Span::raw("  "),
            Span::styled(&server.transport, meta_style),
            Span::raw("  "),
            Span::styled(&server.scope, meta_style),
            Span::raw("  ("),
            Span::styled(
                format!(
                    "{} tool{}",
                    server.tool_count,
                    if server.tool_count == 1 { "" } else { "s" }
                ),
                meta_style,
            ),
            Span::raw(")"),
            if let McpServerStatus::Error(msg) = &server.status {
                Span::styled(format!(" {msg}"), Style::default().fg(theme.error))
            } else {
                Span::raw("")
            },
        ]));
    }

    if state.filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No servers match your filter",
            Style::default().fg(theme.text_muted),
        )));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    // --- Help footer ---
    let help_text = " \u{2191}\u{2193} navigate  Enter: toggle  Esc: close  /: search ";
    let help = Span::styled(help_text, Style::default().fg(theme.text_muted));
    let help_para = Paragraph::new(help);
    frame.render_widget(help_para, chunks[2]);
}

fn compute_scroll_offset(state: &McpDialogState, visible_lines: usize) -> usize {
    if state.cursor < visible_lines / 2 {
        return 0;
    }
    state.cursor.saturating_sub(visible_lines / 2)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn status_icon(status: &McpServerStatus) -> &'static str {
    match status {
        McpServerStatus::Connected => "\u{2713}",    // ✓
        McpServerStatus::Disconnected => "\u{25cb}", // ○
        McpServerStatus::Error(_) => "\u{2717}",     // ✗
        McpServerStatus::Starting => "\u{27f3}",     // ⟳
    }
}

fn status_style(status: &McpServerStatus, theme: &Theme) -> Style {
    match status {
        McpServerStatus::Connected => Style::default().fg(theme.success),
        McpServerStatus::Disconnected => Style::default().fg(theme.text_muted),
        McpServerStatus::Error(_) => Style::default().fg(theme.error),
        McpServerStatus::Starting => Style::default().fg(theme.warning),
    }
}
