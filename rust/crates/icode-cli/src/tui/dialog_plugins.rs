use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
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

#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub tool_count: usize,
    pub description: Option<String>,
}

impl PluginEntry {
    fn search_text(&self) -> String {
        let mut text = format!("{} {} {}", self.name, self.id, self.version).to_lowercase();
        if let Some(ref desc) = self.description {
            text.push(' ');
            text.push_str(&desc.to_lowercase());
        }
        text
    }
}

pub struct PluginsDialogState {
    pub open: bool,
    pub plugins: Vec<PluginEntry>,
    pub cursor: usize,
    pub search: String,
    pub filtered: Vec<usize>,
    pub section_offsets: Vec<(String, usize)>,
}

impl PluginsDialogState {
    pub fn new() -> Self {
        Self {
            open: false,
            plugins: Vec::new(),
            cursor: 0,
            search: String::new(),
            filtered: Vec::new(),
            section_offsets: Vec::new(),
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

    pub fn toggle_plugin(&mut self) {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            if let Some(plugin) = self.plugins.get_mut(idx) {
                plugin.enabled = !plugin.enabled;
                self.rebuild_filtered();
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
        self.section_offsets.clear();

        let matches_query = |e: &PluginEntry| -> bool {
            if query.is_empty() {
                return true;
            }
            e.search_text().contains(&query)
        };

        let enabled_start = 0;
        let enabled: Vec<usize> = self
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, p)| p.enabled)
            .filter(|(_, p)| matches_query(p))
            .map(|(i, _)| i)
            .collect();

        if !enabled.is_empty() {
            self.section_offsets
                .push(("Enabled".to_string(), enabled_start));
            self.filtered.extend(enabled);
        }

        let disabled_start = self.filtered.len();
        let disabled: Vec<usize> = self
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.enabled)
            .filter(|(_, p)| matches_query(p))
            .map(|(i, _)| i)
            .collect();

        if !disabled.is_empty() {
            self.section_offsets
                .push(("Disabled".to_string(), disabled_start));
            self.filtered.extend(disabled);
        }

        if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
        }
    }
}

impl Default for PluginsDialogState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_plugins_dialog(
    frame: &mut Frame,
    state: &mut PluginsDialogState,
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

    let block = Block::default().borders(Borders::ALL).title(" Plugins ");
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

    let scroll_offset = compute_scroll_offset(state, chunks[1].height as usize);
    let mut lines: Vec<Line> = Vec::new();
    let mut current_section = String::new();

    for (pos, &plugin_idx) in state.filtered.iter().enumerate() {
        if pos < scroll_offset {
            continue;
        }
        if lines.len() >= chunks[1].height as usize {
            break;
        }

        let plugin = &state.plugins[plugin_idx];
        let section = state
            .section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| pos >= *offset)
            .map(|(name, _)| name.as_str())
            .unwrap_or("Plugins");

        if section != current_section {
            current_section = section.to_string();
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  -- {section} --"),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let is_selected = pos == state.cursor;

        let marker = if is_selected { "\u{25b6} " } else { "  " };
        let status_icon = if plugin.enabled {
            Span::styled("\u{2713} ", Style::default().fg(theme.success))
        } else {
            Span::styled("\u{25cb} ", Style::default().fg(theme.text_muted))
        };
        let row_style = if is_selected {
            Style::default()
                .fg(theme.text_inverse)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let tool_badge = Span::styled(
            format!(
                "({} tool{})",
                plugin.tool_count,
                if plugin.tool_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(theme.text_muted),
        );

        let version_span = Span::styled(
            format!("v{}", plugin.version),
            Style::default().fg(theme.text_muted),
        );

        let mut spans = vec![
            Span::styled(marker, row_style),
            status_icon,
            Span::styled(&plugin.name, row_style),
            Span::raw("  "),
            version_span,
            Span::raw("  "),
            tool_badge,
        ];

        if let Some(ref desc) = plugin.description {
            spans.push(Span::raw("  "));
            let desc_style = if is_selected {
                row_style.fg(theme.text_muted)
            } else {
                Style::default().fg(theme.text_muted)
            };
            spans.push(Span::styled(desc, desc_style));
        }

        lines.push(Line::from(spans));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    let help_text = " \u{2191}\u{2193} navigate  Enter: toggle  Esc: close  /: search ";
    let help = Span::styled(help_text, Style::default().fg(theme.text_muted));
    let help_para = Paragraph::new(help);
    frame.render_widget(help_para, chunks[2]);
}

fn compute_scroll_offset(state: &PluginsDialogState, visible_lines: usize) -> usize {
    if state.cursor < visible_lines / 2 {
        return 0;
    }
    state.cursor.saturating_sub(visible_lines / 2)
}
