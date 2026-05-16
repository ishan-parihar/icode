use api::models_dev;
use api::providers::{
    check_provider_auth, list_all_models, provider_kind_for_id, ModelCapabilities, ProviderKind,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::tui::model_state::ModelState;
use crate::tui::popup_utils::PopupConfig;
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
    fn search_text(&self) -> String {
        format!("{} {} {:?}", self.alias, self.canonical, self.provider).to_lowercase()
    }
}

/// Action returned from model picker key handling.
#[derive(Debug)]
pub enum ModelPickerAction {
    /// No action.
    None,
    /// A model was selected; the canonical name is provided.
    Selected(String),
    /// User pressed L on an unconfigured provider — open the provider dialog.
    OpenProviderDialog(ProviderKind),
    /// Picker was closed without a selection.
    Closed,
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
    /// Track which providers are currently unconfigured so we can show hints.
    pub unconfigured_providers: std::collections::HashSet<String>,
}

impl ModelPickerState {
    pub fn new() -> Self {
        let model_state = ModelState::load();
        let entries = Self::load_entries();
        let unconfigured = Self::compute_unconfigured();
        Self {
            open: false,
            entries,
            filtered: Vec::new(),
            search: String::new(),
            cursor: 0,
            model_state,
            selected: None,
            section_offsets: Vec::new(),
            unconfigured_providers: unconfigured,
        }
    }

    fn load_entries() -> Vec<ModelEntry> {
        // Use the live catalog — list_all_models() reads models_dev::list_models() internally
        list_all_models()
            .map(|e| ModelEntry {
                alias: e.alias.clone(),
                canonical: e.canonical.clone(),
                provider: e.provider.clone(),
                capabilities: e.capabilities,
            })
            .collect()
    }

    fn compute_unconfigured() -> std::collections::HashSet<String> {
        // Ask the catalog which providers lack auth
        models_dev::catalog()
            .values()
            .filter(|p| !models_dev::provider_has_auth(p))
            .map(|p| p.name.clone())
            .collect()
    }

    pub fn open(&mut self) {
        self.open = true;
        self.search.clear();
        self.cursor = 0;
        self.entries = Self::load_entries();
        self.model_state = ModelState::load();
        self.unconfigured_providers = Self::compute_unconfigured();
        self.rebuild_filtered();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Provider sections derived from the catalog — sorted by name.
    fn provider_sections_from_entries(entries: &[ModelEntry]) -> Vec<(ProviderKind, String)> {
        let mut seen = std::collections::HashSet::new();
        let mut sections: Vec<(ProviderKind, String)> = Vec::new();
        for e in entries {
            let name = api::providers::provider_display_name(&e.provider).into_owned();
            if seen.insert(name.clone()) {
                sections.push((e.provider.clone(), name));
            }
        }
        sections.sort_by(|a, b| a.1.cmp(&b.1));
        sections
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

            // All provider-grouped sections (catalog-driven, sorted by name)
            for (kind, label) in Self::provider_sections_from_entries(&self.entries) {
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
                    self.section_offsets.push((label, section_start));
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

    /// Returns the provider kind for the currently highlighted entry, if any.
    pub fn current_entry_provider(&self) -> Option<ProviderKind> {
        self.filtered
            .get(self.cursor)
            .and_then(|&idx| self.entries.get(idx))
            .map(|e| e.provider.clone())
    }

    /// Returns true if the currently highlighted entry's provider is unconfigured.
    pub fn current_entry_unconfigured(&self) -> bool {
        if let Some(kind) = self.current_entry_provider() {
            !check_provider_auth(&kind)
        } else {
            false
        }
    }

    pub fn current_section(&self) -> &str {
        self.section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| self.cursor >= *offset)
            .map_or("All Models", |(name, _)| name.as_str())
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

    let config = PopupConfig::full("Models");
    let block = config.to_block(theme);
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
            .map_or("All Models", |(name, _)| name.as_str());

        if section != current_section {
            current_section = section.to_string();
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }

            // Check if this provider section is unconfigured
            let section_locked = state.unconfigured_providers.contains(section);

            let section_span = if section_locked {
                Line::from(vec![
                    Span::styled(
                        format!("  {section}"),
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " 🔒 not configured",
                        Style::default().fg(theme.warning).add_modifier(Modifier::ITALIC),
                    ),
                ])
            } else {
                Line::from(Span::styled(
                    format!("  {section}"),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ))
            };
            lines.push(section_span);
        }

        let is_selected = pos == state.cursor;
        let is_fav = state.model_state.is_favorite(&entry.canonical);
        let is_current = state.model_state.current.as_deref() == Some(&entry.canonical);
        let provider_configured = check_provider_auth(&entry.provider);

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
        } else if !provider_configured {
            Style::default().fg(theme.text_muted)
        } else {
            Style::default()
        };

        let provider_color = if !provider_configured && !is_selected {
            theme.text_muted
        } else {
            provider_color(entry.provider.clone(), theme)
        };

        let cap_badge = capability_badge(entry.capabilities, theme);

        let lock_span = if !provider_configured && !is_selected {
            Span::styled(" 🔒", Style::default().fg(theme.warning))
        } else {
            Span::raw("")
        };

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(&entry.alias, style.fg(provider_color)),
            Span::styled(
                format!(" ({})", entry.canonical),
                style.fg(theme.text_muted),
            ),
            Span::raw("  "),
            cap_badge,
            lock_span,
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

    // Show login hint when highlighted entry is from an unconfigured provider
    let help_text = if state.current_entry_unconfigured() {
        " \u{2191}\u{2193} navigate  Enter: select  L: login to provider  Esc: cancel  Ctrl+F: fav  /: search "
    } else {
        " \u{2191}\u{2193} navigate  Enter: select  Esc: cancel  Ctrl+F: favorite  /: search "
    };
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
        ProviderKind::Gemini => Color::Rgb(66, 133, 244),
        ProviderKind::Groq => Color::Rgb(248, 82, 32),
        ProviderKind::Mistral => Color::Rgb(255, 135, 0),
        ProviderKind::OpenRouter => Color::Rgb(100, 88, 216),
        ProviderKind::Bedrock => Color::Rgb(255, 153, 0),
        ProviderKind::Opencode => Color::Rgb(0, 200, 150),
        _ => theme.text_muted,
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
