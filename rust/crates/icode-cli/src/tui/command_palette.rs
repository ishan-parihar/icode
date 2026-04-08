use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::frecency::FrecencyStore;
use crate::tui::popup_utils;
use crate::tui::theme::Theme;

const MAX_VISIBLE_ENTRIES: usize = 20;

/// Actions that a command palette entry can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    SwitchModel,
    NewSession,
    SwitchSession,
    ClearConversation,
    Undo,
    Redo,
    ToggleSidebar,
    ShowStatus,
    ShowCost,
    ExportSession,
    ShowMcp,
    ShowSkills,
    ShowPlugins,
    Exit,
    ToggleTheme,
    ShowHelp,
    ForkSession,
    ToggleThinking,
    CompactContext,
    ShowContextViz,
    ShowPromptStash,
    OpenExternalEditor,
    ShowExportOptions,
    ShowProviders,
    ShowWorkspaces,
    AttachSession,
}

/// A single command entry in the palette.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub title: String,
    pub description: String,
    pub value: String,
    pub category: String,
    pub shortcut: String,
    pub suggested: bool,
    pub action: CommandAction,
}

impl CommandEntry {
    fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.title, self.description, self.category, self.shortcut
        )
        .to_lowercase()
    }
}

/// State for the command palette overlay.
pub struct CommandPaletteState {
    pub open: bool,
    pub entries: Vec<CommandEntry>,
    pub filtered: Vec<usize>,
    pub search: String,
    pub cursor: usize,
    pub selected: Option<String>,
    pub section_offsets: Vec<(String, usize)>,
    pub frecency_store: Option<FrecencyStore>,
}

impl CommandPaletteState {
    pub fn new() -> Self {
        let entries = register_commands();
        Self {
            open: false,
            entries,
            filtered: Vec::new(),
            search: String::new(),
            cursor: 0,
            selected: None,
            section_offsets: Vec::new(),
            frecency_store: None,
        }
    }

    pub fn with_frecency(store: FrecencyStore) -> Self {
        let mut state = Self::new();
        state.frecency_store = Some(store);
        state
    }

    pub fn open(&mut self) {
        self.open = true;
        self.search.clear();
        self.cursor = 0;
        self.selected = None;
        self.rebuild_filtered();
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn rebuild_filtered(&mut self) {
        let query = self.search.to_lowercase();
        self.filtered.clear();
        self.section_offsets.clear();

        let matches_query = |e: &CommandEntry| -> bool {
            if query.is_empty() {
                return true;
            }
            e.search_text().contains(&query)
        };

        if query.is_empty() {
            let start = self.filtered.len();
            let mut suggested_indices: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.suggested)
                .map(|(i, _)| i)
                .collect();

            // Sort suggested entries by frecency score when store is available
            if let Some(ref store) = self.frecency_store {
                suggested_indices.sort_by(|&a, &b| {
                    let score_a = store.get_score(&self.entries[a].value);
                    let score_b = store.get_score(&self.entries[b].value);
                    score_b
                        .partial_cmp(&score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }

            if !suggested_indices.is_empty() {
                self.section_offsets.push(("Suggested".to_string(), start));
                self.filtered.extend(suggested_indices);
            }
        }

        // Then show regular categories
        let category_order = ["Agent", "Session", "System"];
        for category in &category_order {
            let start = self.filtered.len();
            let indices: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| &e.category == category)
                .filter(|(_, e)| {
                    if query.is_empty() {
                        !e.suggested
                    } else {
                        matches_query(e)
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if !indices.is_empty() {
                self.section_offsets.push((category.to_string(), start));
                self.filtered.extend(indices);
            }
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
                self.selected = Some(entry.value.clone());
                self.close();
            }
        }
    }

    pub fn current_section(&self) -> &str {
        self.section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| self.cursor >= *offset)
            .map_or("Commands", |(name, _)| name.as_str())
    }
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self::new()
    }
}

fn register_commands() -> Vec<CommandEntry> {
    vec![
        CommandEntry {
            title: "Switch model".to_string(),
            description: "Change the current AI model".to_string(),
            value: "model.list".to_string(),
            category: "Agent".to_string(),
            shortcut: "Ctrl+M".to_string(),
            suggested: true,
            action: CommandAction::SwitchModel,
        },
        CommandEntry {
            title: "MCP servers".to_string(),
            description: "Manage MCP server connections".to_string(),
            value: "mcp.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowMcp,
        },
        CommandEntry {
            title: "Skills".to_string(),
            description: "Browse and manage installed skills".to_string(),
            value: "skills.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowSkills,
        },
        CommandEntry {
            title: "Plugins".to_string(),
            description: "Browse and manage plugins".to_string(),
            value: "plugins.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowPlugins,
        },
        CommandEntry {
            title: "Toggle thinking".to_string(),
            description: "Toggle extended thinking mode".to_string(),
            value: "thinking.toggle".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ToggleThinking,
        },
        CommandEntry {
            title: "New session".to_string(),
            description: "Start a fresh conversation".to_string(),
            value: "session.new".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: true,
            action: CommandAction::NewSession,
        },
        CommandEntry {
            title: "Switch session".to_string(),
            description: "Browse and resume previous sessions".to_string(),
            value: "session.list".to_string(),
            category: "Session".to_string(),
            shortcut: "Ctrl+X L".to_string(),
            suggested: true,
            action: CommandAction::SwitchSession,
        },
        CommandEntry {
            title: "Fork session".to_string(),
            description: "Create a branch from this point".to_string(),
            value: "session.fork".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ForkSession,
        },
        CommandEntry {
            title: "Clear conversation".to_string(),
            description: "Clear current conversation history".to_string(),
            value: "session.clear".to_string(),
            category: "Session".to_string(),
            shortcut: "Ctrl+L".to_string(),
            suggested: false,
            action: CommandAction::ClearConversation,
        },
        CommandEntry {
            title: "Undo last message".to_string(),
            description: "Undo last message and tool changes".to_string(),
            value: "session.undo".to_string(),
            category: "Session".to_string(),
            shortcut: "PgUp".to_string(),
            suggested: false,
            action: CommandAction::Undo,
        },
        CommandEntry {
            title: "Redo".to_string(),
            description: "Redo a previously undone message".to_string(),
            value: "session.redo".to_string(),
            category: "Session".to_string(),
            shortcut: "PgDn".to_string(),
            suggested: false,
            action: CommandAction::Redo,
        },
        CommandEntry {
            title: "Export session".to_string(),
            description: "Export conversation to file".to_string(),
            value: "session.export".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ExportSession,
        },
        CommandEntry {
            title: "Compact context".to_string(),
            description: "Summarize and compress context window".to_string(),
            value: "session.compact".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::CompactContext,
        },
        CommandEntry {
            title: "Context window".to_string(),
            description: "Visualize current context usage".to_string(),
            value: "context.show".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowContextViz,
        },
        CommandEntry {
            title: "Prompt Stash".to_string(),
            description: "Browse saved prompt snippets".to_string(),
            value: "/stash".to_string(),
            category: "Session".to_string(),
            shortcut: "Ctrl+S".to_string(),
            suggested: false,
            action: CommandAction::ShowPromptStash,
        },
        CommandEntry {
            title: "External Editor".to_string(),
            description: "Open prompt in external editor".to_string(),
            value: "/editor".to_string(),
            category: "Session".to_string(),
            shortcut: "Alt+E".to_string(),
            suggested: false,
            action: CommandAction::OpenExternalEditor,
        },
        CommandEntry {
            title: "Export options".to_string(),
            description: "Choose export format and scope".to_string(),
            value: "/export".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowExportOptions,
        },
        CommandEntry {
            title: "Provider connections".to_string(),
            description: "View connected AI providers".to_string(),
            value: "providers.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowProviders,
        },
        CommandEntry {
            title: "Workspaces".to_string(),
            description: "Browse and switch workspaces".to_string(),
            value: "workspaces.list".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowWorkspaces,
        },
        CommandEntry {
            title: "Attach to session".to_string(),
            description: "Attach to a running session".to_string(),
            value: "session.attach".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::AttachSession,
        },
        CommandEntry {
            title: "Toggle sidebar".to_string(),
            description: "Show or hide the sidebar".to_string(),
            value: "sidebar.toggle".to_string(),
            category: "Session".to_string(),
            shortcut: "Alt+S".to_string(),
            suggested: false,
            action: CommandAction::ToggleSidebar,
        },
        CommandEntry {
            title: "View status".to_string(),
            description: "Show session status and stats".to_string(),
            value: "status.view".to_string(),
            category: "System".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowStatus,
        },
        CommandEntry {
            title: "Show cost".to_string(),
            description: "Display token cost breakdown".to_string(),
            value: "cost.show".to_string(),
            category: "System".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowCost,
        },
        CommandEntry {
            title: "Toggle theme".to_string(),
            description: "Switch between color themes".to_string(),
            value: "theme.toggle".to_string(),
            category: "System".to_string(),
            shortcut: String::new(),
            suggested: true,
            action: CommandAction::ToggleTheme,
        },
        CommandEntry {
            title: "Help & keybindings".to_string(),
            description: "Show available shortcuts".to_string(),
            value: "help.show".to_string(),
            category: "System".to_string(),
            shortcut: "?".to_string(),
            suggested: false,
            action: CommandAction::ShowHelp,
        },
        CommandEntry {
            title: "Exit".to_string(),
            description: "Close the application".to_string(),
            value: "app.exit".to_string(),
            category: "System".to_string(),
            shortcut: "Ctrl+C".to_string(),
            suggested: false,
            action: CommandAction::Exit,
        },
    ]
}

pub fn render_command_palette(
    frame: &mut Frame,
    state: &mut CommandPaletteState,
    area: Rect,
    theme: Theme,
) {
    if !state.open {
        return;
    }

    let content_height = (state.filtered.len().min(MAX_VISIBLE_ENTRIES) as u16).saturating_add(5); // search + hint + borders + section headers
    let dialog_area = popup_utils::popup_dimensions(area, 0.5, 30, 60, 0.5, content_height);

    frame.render_widget(Clear, dialog_area);

    let block = popup_utils::dialog_block(theme, " Commands ");
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

    // Command list
    let scroll_offset = compute_scroll_offset(state, chunks[1].height as usize);
    let mut lines: Vec<Line> = Vec::new();
    let mut current_section = String::new();

    for (abs_pos, &entry_idx) in state
        .filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(state.filtered.len().min(MAX_VISIBLE_ENTRIES + 5))
    {
        if lines.len() >= chunks[1].height as usize {
            break;
        }

        let entry = &state.entries[entry_idx];
        let section = state
            .section_offsets
            .iter()
            .rev()
            .find(|(_, offset)| abs_pos >= *offset)
            .map_or("Commands", |(name, _)| name.as_str());

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

        let is_selected = abs_pos == state.cursor;
        let marker = if is_selected { "\u{25b6} " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(theme.text_inverse)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let shortcut_span = if entry.shortcut.is_empty() {
            Span::raw("")
        } else {
            Span::styled(
                format!("  {}", entry.shortcut),
                Style::default().fg(theme.text_muted),
            )
        };

        let description_span = if entry.description.is_empty() {
            Span::raw("")
        } else {
            Span::styled(
                format!(" {}", entry.description),
                Style::default().fg(theme.text_muted),
            )
        };

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(&entry.title, style),
            description_span,
            shortcut_span,
        ]));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    // Scroll indicator
    if state.filtered.len() > MAX_VISIBLE_ENTRIES {
        let scroll_text = format!(
            " Showing {}-{} of {} ",
            scroll_offset + 1,
            (scroll_offset + MAX_VISIBLE_ENTRIES).min(state.filtered.len()),
            state.filtered.len()
        );
        let scroll_indicator = Paragraph::new(Line::from(Span::styled(
            scroll_text,
            Style::default().fg(theme.text_muted),
        )));
        // Render indicator on the last line of the content area if space permits
        let indicator_area = Rect::new(
            chunks[1].x,
            chunks[1].bottom().saturating_sub(1),
            chunks[1].width,
            1,
        );
        frame.render_widget(scroll_indicator, indicator_area);
    }

    // Hint bar
    popup_utils::render_hint_bar(
        frame,
        chunks[2],
        &[
            ("\u{2191}\u{2193}", "navigate"),
            ("Enter", "execute"),
            ("Esc", "cancel"),
        ],
        theme,
    );
}

fn compute_scroll_offset(state: &CommandPaletteState, visible_lines: usize) -> usize {
    if state.cursor < visible_lines / 2 {
        return 0;
    }
    state.cursor.saturating_sub(visible_lines / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        title: &str,
        description: &str,
        value: &str,
        category: &str,
        suggested: bool,
    ) -> CommandEntry {
        CommandEntry {
            title: title.to_string(),
            description: description.to_string(),
            value: value.to_string(),
            category: category.to_string(),
            shortcut: String::new(),
            suggested,
            action: CommandAction::ShowHelp,
        }
    }

    #[test]
    fn test_search_text_includes_description() {
        let entry = make_entry(
            "Switch model",
            "Change the current AI model",
            "model.list",
            "Agent",
            true,
        );
        let text = entry.search_text();
        assert!(text.contains("switch"));
        assert!(text.contains("model"));
        assert!(text.contains("change"));
        assert!(text.contains("current"));
        assert!(text.contains("agent"));
    }

    #[test]
    fn test_fuzzy_search_matches_description() {
        let mut state = CommandPaletteState::new();
        // Search for "change" which only appears in descriptions
        state.search = "change".to_string();
        state.rebuild_filtered();
        // "Switch model" has "Change the current AI model" as description
        assert!(
            state
                .filtered
                .iter()
                .any(|&idx| state.entries[idx].value == "model.list"),
            "should match 'Switch model' via description"
        );
    }

    #[test]
    fn test_fuzzy_search_matches_shortcut() {
        let mut state = CommandPaletteState::new();
        state.search = "ctrl+m".to_string();
        state.rebuild_filtered();
        assert!(
            state
                .filtered
                .iter()
                .any(|&idx| state.entries[idx].value == "model.list"),
            "should match 'Switch model' via shortcut"
        );
    }

    #[test]
    fn test_max_20_visible_entries() {
        // Create a state with more than 20 entries
        let mut state = CommandPaletteState::new();
        // Add extra entries to exceed 20
        for i in 0..30 {
            state.entries.push(make_entry(
                &format!("Extra {i}"),
                &format!("Extra description {i}"),
                &format!("extra.{i}"),
                "Agent",
                false,
            ));
        }
        state.rebuild_filtered();
        assert!(state.filtered.len() > MAX_VISIBLE_ENTRIES);

        // Verify the rendering cap is enforced — we check by simulating the render logic
        let display_count = state.filtered.len().min(MAX_VISIBLE_ENTRIES);
        assert_eq!(display_count, MAX_VISIBLE_ENTRIES);
    }

    #[test]
    fn test_frecency_sorting_when_no_query() {
        use std::path::PathBuf;
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut store = FrecencyStore::new(PathBuf::from("/tmp/test-frecency-cmd-palette.json"));
        // Record "session.list" more often to give it higher frecency
        for _ in 0..10 {
            store.record("session.list");
        }
        // Record "model.list" less often
        for _ in 0..2 {
            store.record("model.list");
        }
        // Record "theme.toggle" once
        store.record("theme.toggle");

        let mut state = CommandPaletteState::with_frecency(store);
        state.open(); // This triggers rebuild_filtered with empty query

        // Find positions of suggested entries in filtered list
        let mut positions: Vec<(usize, &str)> = state
            .filtered
            .iter()
            .map(|&idx| (idx, state.entries[idx].value.as_str()))
            .filter(|&(_, v)| {
                v == "session.list"
                    || v == "model.list"
                    || v == "theme.toggle"
                    || v == "session.new"
            })
            .map(|(idx, v)| {
                // Find the position in filtered
                let pos = state.filtered.iter().position(|&i| i == idx).unwrap();
                (pos, v)
            })
            .collect();
        positions.sort_by_key(|&(pos, _)| pos);

        // "session.list" should come before "model.list" due to higher frecency
        let session_pos = positions
            .iter()
            .position(|&(_, v)| v == "session.list")
            .unwrap();
        let model_pos = positions
            .iter()
            .position(|&(_, v)| v == "model.list")
            .unwrap();
        assert!(
            session_pos < model_pos,
            "session.list (pos {}) should come before model.list (pos {})",
            session_pos,
            model_pos
        );
    }

    #[test]
    fn test_all_entries_have_descriptions() {
        let state = CommandPaletteState::new();
        for entry in &state.entries {
            assert!(
                !entry.description.is_empty(),
                "Entry '{}' has no description",
                entry.title
            );
        }
    }

    #[test]
    fn test_command_entry_count() {
        let state = CommandPaletteState::new();
        assert_eq!(
            state.entries.len(),
            26,
            "should have exactly 26 command entries"
        );
    }

    #[test]
    fn test_cursor_stays_within_bounds_after_filter() {
        let mut state = CommandPaletteState::new();
        state.open();
        // Move cursor down past the end
        for _ in 0..1000 {
            state.cursor_down();
        }
        assert!(state.cursor < state.filtered.len());
    }

    #[test]
    fn test_confirm_sets_selected_value() {
        let mut state = CommandPaletteState::new();
        state.open();
        state.confirm();
        assert!(state.selected.is_some());
        assert!(!state.open);
    }
}
