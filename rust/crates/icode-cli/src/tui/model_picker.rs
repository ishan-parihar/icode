use api::providers::{list_all_models, ModelCapabilities, ProviderKind, RegistryEntry};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::model_state::ModelState;
use crate::tui::theme::Theme;

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
    (term_height / 2).saturating_sub(6).max(MIN_HEIGHT)
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub alias: String,
    pub canonical: String,
    pub provider: ProviderKind,
    pub capabilities: ModelCapabilities,
}

impl ModelEntry {
    fn display_name(&self) -> String {
        format!["{} ({})", self.alias, self.canonical]
    }

    fn search_text(&self) -> String {
        format!("{} {} {:?}", self.alias, self.canonical, self.provider).to_lowercase()
    }
}

pub struct ModelPickerState {
    pub open: bool,
    pub entries: Vec<ModelEntry>,
    pub filtered: Vec<usize>,
    pub search: String,
    pub cursor: usize,
    pub model_state: ModelState,
    pub selected: Option<String>,
    pub section_offsets: Vec<(String, usize)>,
}

impl ModelPickerState {
    pub fn new() -> Self {
        let model_state = ModelState::load();
        let entries: Vec<ModelEntry> = list_all_models()
            .map(|e| ModelEntry {
                alias: e.alias.to_string(),
                canonical: e.canonical.to_string(),
                provider: e.provider,
                capabilities: e.capabilities,
            })
            .collect();

        Self {
            open: false,
            entries,
            filtered: Vec::new(),
            search: String::new(),
            cursor: 0,
            model_state,
            selected: None,
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

    pub fn rebuild_filtered(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered.clear();
        self.section_offsets.clear();

        let query_fn = |e: &ModelEntry| -> bool {
            if query.is_empty() {
                return true;
            }
            e.search_text().contains(&query)
        };

        if self.search.is_empty() {
            let mut seen = std::collections::HashSet::new();

            // ★ Favorites
            let fav_start = 0;
            let favs: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| self.model_state.is_favorite(&e.canonical))
                .filter(|(_, e)| seen.insert(e.alias.clone()))
                .map(|(i, _)| i)
                .collect();
            if !favs.is_empty() {
                self.section_offsets
                    .push(("★ Favorites".to_string(), fav_start));
                self.filtered.extend(favs);
            }

            // ◷ Recent (last 8, not already in favorites)
            let recent_start = self.filtered.len();
            let recents: Vec<usize> = self
                .model_state
                .recent
                .iter()
                .filter_map(|r| {
                    self.entries
                        .iter()
                        .enumerate()
                        .find(|(_, e)| &e.canonical == r)
                })
                .filter(|(_, e)| !self.model_state.is_favorite(&e.canonical))
                .filter(|(_, e)| seen.insert(e.alias.clone()))
                .map(|(i, _)| i)
                .collect();
            if !recents.is_empty() {
                self.section_offsets
                    .push(("◷ Recent".to_string(), recent_start));
                self.filtered.extend(recents);
            }

            // Provider-grouped sections
            let providers: Vec<(ProviderKind, &str)> = vec![
                (ProviderKind::Anthropic, "Anthropic"),
                (ProviderKind::OpenAi, "OpenAI"),
                (ProviderKind::Xai, "xAI"),
                (ProviderKind::QwenProxy, "Qwen"),
            ];

            for (kind, label) in providers {
                let section_start = self.filtered.len();
                let models: Vec<usize> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.provider == kind)
                    .filter(|(_, e)| seen.insert(e.alias.clone()))
                    .map(|(i, _)| i)
                    .collect();
                if !models.is_empty() {
                    self.section_offsets
                        .push((label.to_string(), section_start));
                    self.filtered.extend(models);
                }
            }
        } else {
            let all_start = 0;
            let all: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| query_fn(e))
                .map(|(i, _)| i)
                .collect();
            self.section_offsets
                .push(("Search Results".to_string(), all_start));
            self.filtered.extend(all);
        }

        if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
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

    pub fn confirm(&mut self) {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            if let Some(entry) = self.entries.get(idx) {
                self.model_state.set_current(&entry.canonical);
                self.model_state.save();
                self.selected = Some(entry.canonical.clone());
                self.close();
            }
        }
    }

    pub fn toggle_favorite(&mut self) {
        if let Some(&idx) = self.filtered.get(self.cursor) {
            if let Some(entry) = self.entries.get(idx) {
                self.model_state.toggle_favorite(&entry.canonical);
                self.model_state.save();
                self.rebuild_filtered();
            }
        }
    }

    pub fn current_section(&self) -> &str {
        self.section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| self.cursor >= *offset)
            .map(|(name, _)| name.as_str())
            .unwrap_or("All Models")
    }
}

impl Default for ModelPickerState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_model_picker(
    frame: &mut Frame,
    state: &mut ModelPickerState,
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
        .title(" Select Model ");
    frame.render_widget(block, dialog_area);

    let inner = dialog_area.inner(ratatui::layout::Margin::new(1, 1));
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
    let cursor_pos = if state.search.is_empty() {
        0
    } else {
        state.search.len() as u16
    };
    let search_para = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent)),
        search_text,
    ]));
    frame.render_widget(search_para, chunks[0]);

    let scroll_offset = compute_scroll_offset(state, chunks[1].height as usize);
    let mut lines: Vec<Line> = Vec::new();
    let mut current_section = String::new();

    for (pos, &entry_idx) in state.filtered.iter().enumerate() {
        if pos < scroll_offset {
            continue;
        }
        if lines.len() >= chunks[1].height as usize {
            break;
        }

        let entry = &state.entries[entry_idx];
        let section = state
            .section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| pos >= *offset)
            .map(|(name, _)| name.as_str())
            .unwrap_or("All Models");

        if section != current_section {
            current_section = section.to_string();
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  {section}"),
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let is_selected = pos == state.cursor;
        let is_fav = state.model_state.is_favorite(&entry.canonical);
        let is_current = state.model_state.current.as_deref() == Some(&entry.canonical);

        let marker = if is_selected { "\u{25b6} " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(theme.text_inverse)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else if is_current {
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let provider_color = provider_color(entry.provider, theme);
        let cap_badge = capability_badge(entry.capabilities, theme);

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(&entry.alias, style.fg(provider_color)),
            Span::styled(
                format!(" ({})", entry.canonical),
                style.fg(theme.text_muted),
            ),
            Span::raw("  "),
            cap_badge,
            if is_fav {
                Span::styled(" \u{2605}", Style::default().fg(theme.accent))
            } else {
                Span::raw("")
            },
            if is_current {
                Span::styled(" [current]", Style::default().fg(theme.success))
            } else {
                Span::raw("")
            },
        ]));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    let help_text =
        " \u{2191}\u{2193} navigate  Enter: select  Esc: cancel  Ctrl+F: favorite  /: search ";
    let help = Span::styled(help_text, Style::default().fg(theme.text_muted));
    let help_para = Paragraph::new(help);
    frame.render_widget(help_para, chunks[2]);
}

fn compute_scroll_offset(state: &ModelPickerState, visible_lines: usize) -> usize {
    if state.cursor < visible_lines / 2 {
        return 0;
    }
    state.cursor.saturating_sub(visible_lines / 2)
}

fn provider_color(kind: ProviderKind, theme: Theme) -> Color {
    match kind {
        ProviderKind::Anthropic => Color::Rgb(218, 165, 32),
        ProviderKind::Xai => theme.text,
        ProviderKind::OpenAi => Color::Rgb(16, 163, 127),
        ProviderKind::QwenProxy => Color::Rgb(100, 149, 237),
    }
}

fn capability_badge(caps: ModelCapabilities, theme: Theme) -> Span<'static> {
    let mut badges = String::new();
    if caps.supports_reasoning {
        badges.push('\u{1f9e0}');
    }
    if caps.supports_tools {
        badges.push('\u{1f527}');
    }
    if caps.supports_images {
        badges.push('\u{1f4f7}');
    }
    if badges.is_empty() {
        Span::raw("")
    } else {
        Span::styled(badges, Style::default().fg(theme.text_muted))
    }
}
