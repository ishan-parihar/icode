use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::tui::popup_utils::PopupConfig;
use crate::tui::theme::Theme;
use crate::tui::theme_loader::{list_themes_by_category, ThemeCategory, ThemeEntry};

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 20;

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
    (term_height / 2).saturating_sub(4).max(MIN_HEIGHT)
}

#[derive(Debug, Clone)]
pub struct ThemeListEntry {
    pub id: String,
    pub display_name: String,
    pub category: ThemeCategory,
    pub theme: Theme,
}

pub struct ThemeListDialogState {
    pub open: bool,
    pub all_themes: Vec<ThemeListEntry>,
    pub filtered: Vec<usize>,
    pub section_offsets: Vec<(String, usize)>,
    pub cursor: usize,
    pub scroll: usize,
    pub search: String,
    pub selected_id: String,
}

impl ThemeListDialogState {
    pub fn new(current_theme_id: &str) -> Self {
        let all_themes = build_theme_list();
        let selected_id = current_theme_id.to_string();
        let mut state = Self {
            open: false,
            all_themes,
            filtered: Vec::new(),
            section_offsets: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: String::new(),
            selected_id,
        };
        state.rebuild_filtered();
        state
    }

    pub fn open(&mut self) {
        self.open = true;
        self.search.clear();
        self.cursor = 0;
        self.scroll = 0;
        self.rebuild_filtered();
    }

    pub fn close(&mut self) {
        self.open = false;
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
        self.scroll = 0;
        self.rebuild_filtered();
    }

    pub fn backspace(&mut self) {
        self.search.pop();
        self.cursor = 0;
        self.scroll = 0;
        self.rebuild_filtered();
    }

    pub fn selected_theme_id(&self) -> Option<&str> {
        self.filtered
            .get(self.cursor)
            .map(|&idx| self.all_themes[idx].id.as_str())
    }

    fn rebuild_filtered(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered.clear();
        self.section_offsets.clear();

        let matches_query = |entry: &ThemeListEntry| -> bool {
            if query.is_empty() {
                return true;
            }
            entry.id.to_lowercase().contains(&query)
                || entry.display_name.to_lowercase().contains(&query)
        };

        for (cat, entries) in list_themes_by_category() {
            let cat_entries: Vec<usize> = entries
                .iter()
                .filter_map(|e| {
                    let idx = self.all_themes.iter().position(|t| t.id == e.id)?;
                    if matches_query(&self.all_themes[idx]) {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect();

            if !cat_entries.is_empty() {
                self.section_offsets
                    .push((cat.label().to_string(), self.filtered.len()));
                self.filtered.extend(cat_entries);
            }
        }

        // Restore cursor to the selected theme if visible
        if let Some(pos) = self
            .filtered
            .iter()
            .position(|&idx| self.all_themes[idx].id == self.selected_id)
        {
            self.cursor = pos;
        } else if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
        }
    }
}

fn build_theme_list() -> Vec<ThemeListEntry> {
    let mut result = Vec::new();
    for (_, entries) in list_themes_by_category() {
        for e in entries {
            result.push(ThemeListEntry {
                id: e.id.to_string(),
                display_name: e.display_name.to_string(),
                category: e.category,
                theme: *e.theme,
            });
        }
    }
    result
}

pub fn render_theme_list_dialog(
    frame: &mut Frame,
    state: &ThemeListDialogState,
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

    let config = PopupConfig::full("Themes");
    let block = config.to_block(theme);
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
    let search_prompt = if state.search.is_empty() {
        Span::styled("Type to search... ", Style::default().fg(theme.text_muted))
    } else {
        Span::styled(
            format!("{} ", state.search),
            Style::default().fg(theme.text),
        )
    };
    let cursor_pos = state.search.len() + 1;
    let search_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.primary)),
        search_prompt,
    ]);
    let search_para = Paragraph::new(search_line);
    frame.render_widget(search_para, chunks[0]);

    if state.open && chunks[0].width > 0 {
        frame.set_cursor_position((
            chunks[0].x + (2 + cursor_pos.min(chunks[0].width.saturating_sub(3) as usize)) as u16,
            chunks[0].y,
        ));
    }

    // Theme list
    let list_height = chunks[1].height as usize;
    let mut scroll = state.scroll;
    if state.cursor < scroll {
        scroll = state.cursor;
    }
    if state.cursor >= scroll + list_height {
        scroll = state.cursor.saturating_sub(list_height) + 1;
    }

    let mut lines: Vec<Line> = Vec::new();
    let mut current_section = String::new();

    for (i, &idx) in state.filtered.iter().enumerate().skip(scroll) {
        if lines.len() >= list_height {
            break;
        }

        let entry = &state.all_themes[idx];

        // Show section header if changed
        for (section_name, offset) in &state.section_offsets {
            if *offset == i && section_name != &current_section {
                current_section = section_name.clone();
                lines.push(Line::from(Span::styled(
                    format!(" {section_name}"),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                )));
                if lines.len() >= list_height {
                    break;
                }
            }
        }

        if lines.len() >= list_height {
            break;
        }

        let is_selected = entry.id == state.selected_id;
        let is_cursor = i == state.cursor;

        let prefix = if is_cursor { "> " } else { "  " };
        let check = if is_selected { "✓ " } else { "  " };

        // Color swatch
        let swatch = Span::styled(
            "██",
            Style::default()
                .fg(entry.theme.background)
                .bg(entry.theme.primary),
        );

        let name_style = if is_cursor {
            Style::default()
                .fg(theme.text)
                .bg(theme.background_hover)
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        let name_span = Span::styled(&entry.display_name, name_style);
        let id_span = Span::styled(
            format!(" ({})", entry.id),
            Style::default().fg(theme.text_muted),
        );

        lines.push(Line::from(vec![
            Span::raw(prefix),
            swatch,
            Span::raw(" "),
            Span::raw(check),
            name_span,
            id_span,
        ]));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    // Footer
    let footer_text = format!(
        " {} themes | ↑↓ navigate | Enter select | Esc close | / search",
        state.filtered.len()
    );
    let footer = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(footer, chunks[2]);
}
