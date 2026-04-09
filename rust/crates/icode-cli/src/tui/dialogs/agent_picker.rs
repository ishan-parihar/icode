use orchestration::{AgentConfig, AgentMode};
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 16;
const MAX_VISIBLE: usize = 10;

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

fn mode_label(mode: &AgentMode) -> &'static str {
    match mode {
        AgentMode::Primary => "Primary",
        AgentMode::Subagent => "Subagent",
        AgentMode::All => "All",
    }
}

/// State for the agent picker overlay.
pub struct AgentPickerState {
    pub open: bool,
    pub cursor: usize,
    pub search: String,
    pub filtered: Vec<usize>,
}

impl AgentPickerState {
    pub fn new() -> Self {
        Self {
            open: false,
            cursor: 0,
            search: String::new(),
            filtered: Vec::new(),
        }
    }

    pub fn open(&mut self, agents: &[AgentConfig]) {
        self.open = true;
        self.search.clear();
        self.cursor = 0;
        self.rebuild_filtered(agents);
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn select(&mut self, agents: &[AgentConfig]) -> Option<String> {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            if let Some(agent) = agents.get(idx) {
                let name = agent.name.clone();
                self.close();
                return Some(name);
            }
        }
        None
    }

    pub fn cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_down(&mut self, agents: &[AgentConfig]) {
        if self.cursor + 1 < self.filtered.len() {
            self.cursor += 1;
        }
    }

    pub fn type_char(&mut self, c: char, agents: &[AgentConfig]) {
        self.search.push(c);
        self.cursor = 0;
        self.rebuild_filtered(agents);
    }

    pub fn backspace(&mut self, agents: &[AgentConfig]) {
        self.search.pop();
        self.cursor = 0;
        self.rebuild_filtered(agents);
    }

    pub fn rebuild_filtered(&mut self, agents: &[AgentConfig]) {
        let query = self.search.to_lowercase();
        self.filtered.clear();

        if query.is_empty() {
            for (i, _) in agents.iter().enumerate() {
                self.filtered.push(i);
            }
        } else {
            for (i, agent) in agents.iter().enumerate() {
                let search_text = format!("{} {}", agent.name, agent.description).to_lowercase();
                if search_text.contains(&query) {
                    self.filtered.push(i);
                }
            }
        }

        if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
        }
    }
}

impl Default for AgentPickerState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_agent_picker(
    frame: &mut Frame,
    state: &mut AgentPickerState,
    agents: &[AgentConfig],
    current_agent: &str,
    theme: Theme,
) {
    if !state.open {
        return;
    }

    let area = frame.area();
    let width = dialog_width(area.width).min(area.width.saturating_sub(4));
    let height = dialog_height(area.height).min(area.height.saturating_sub(4));
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Select Agent ")
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background_element))
        .title_style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD));
    frame.render_widget(block, dialog_area);

    let inner = dialog_area.inner(Margin::new(1, 1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Search bar
    let search_text = if state.search.is_empty() {
        Span::styled("Type to search...", Style::default().fg(theme.text_muted))
    } else {
        Span::raw(&state.search)
    };
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent)),
        search_text,
    ]));
    frame.render_widget(search_para, chunks[0]);

    // Agent list
    let scroll_offset = compute_scroll_offset(state, chunks[1].height as usize);
    let mut lines: Vec<Line> = Vec::new();

    for (pos, &agent_idx) in state.filtered.iter().enumerate() {
        if pos < scroll_offset {
            continue;
        }
        if lines.len() >= chunks[1].height as usize {
            break;
        }

        let agent = &agents[agent_idx];
        let is_selected = pos == state.cursor;
        let is_current = agent.name == current_agent;

        let marker = if is_selected { "\u{25b6} " } else { "  " };
        let title_fg = if is_selected {
            theme.text_inverse
        } else {
            theme.text
        };
        let mode_fg = if is_selected {
            theme.text_inverse
        } else {
            theme.accent
        };
        let desc_fg = if is_selected {
            theme.text_inverse
        } else {
            theme.text_muted
        };
        let bg = if is_selected {
            theme.primary
        } else {
            Color::Reset
        };

        let mode = mode_label(&agent.mode);
        let desc = if agent.description.chars().count() > 30 {
            format!(
                "{}...",
                agent.description.chars().take(27).collect::<String>()
            )
        } else {
            agent.description.clone()
        };

        let mut spans: Vec<Span> = vec![
            Span::styled(marker, Style::default().fg(title_fg).bg(bg)),
            Span::styled(&agent.name, Style::default().fg(title_fg).bg(bg)),
            Span::styled(format!("  [{mode}]"), Style::default().fg(mode_fg).bg(bg)),
            Span::styled(format!("  {desc}"), Style::default().fg(desc_fg).bg(bg)),
        ];

        if is_current {
            spans.push(Span::styled(
                " [current]",
                Style::default().fg(theme.success).bg(bg),
            ));
        }

        lines.push(Line::from(spans));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    // Help text
    let help_text = " \u{2191}\u{2193} navigate  Enter: select  Esc: cancel  /: search ";
    let help = Span::styled(help_text, Style::default().fg(theme.text_muted));
    let help_para = Paragraph::new(help);
    frame.render_widget(help_para, chunks[2]);
}

fn compute_scroll_offset(state: &AgentPickerState, visible_lines: usize) -> usize {
    if state.filtered.is_empty() {
        return 0;
    }

    let visible = visible_lines.min(MAX_VISIBLE);

    if state.cursor < visible / 2 {
        return 0;
    }

    state.cursor.saturating_sub(visible / 2)
}
