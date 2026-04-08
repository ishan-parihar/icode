use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

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
    pub value: String,
    pub category: String,
    pub shortcut: String,
    pub suggested: bool,
    pub action: CommandAction,
}

impl CommandEntry {
    fn search_text(&self) -> String {
        format!("{} {} {}", self.title, self.value, self.category).to_lowercase()
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
        }
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

        // When no search, show "Suggested" category first
        if query.is_empty() {
            let start = self.filtered.len();
            let suggested_indices: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.suggested)
                .map(|(i, _)| i)
                .collect();
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
            value: "model.list".to_string(),
            category: "Agent".to_string(),
            shortcut: "Ctrl+M".to_string(),
            suggested: true,
            action: CommandAction::SwitchModel,
        },
        CommandEntry {
            title: "MCP servers".to_string(),
            value: "mcp.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowMcp,
        },
        CommandEntry {
            title: "Skills".to_string(),
            value: "skills.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowSkills,
        },
        CommandEntry {
            title: "Plugins".to_string(),
            value: "plugins.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowPlugins,
        },
        CommandEntry {
            title: "Toggle thinking".to_string(),
            value: "thinking.toggle".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ToggleThinking,
        },
        CommandEntry {
            title: "New session".to_string(),
            value: "session.new".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: true,
            action: CommandAction::NewSession,
        },
        CommandEntry {
            title: "Switch session".to_string(),
            value: "session.list".to_string(),
            category: "Session".to_string(),
            shortcut: "Ctrl+X L".to_string(),
            suggested: true,
            action: CommandAction::SwitchSession,
        },
        CommandEntry {
            title: "Fork session".to_string(),
            value: "session.fork".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ForkSession,
        },
        CommandEntry {
            title: "Clear conversation".to_string(),
            value: "session.clear".to_string(),
            category: "Session".to_string(),
            shortcut: "Ctrl+L".to_string(),
            suggested: false,
            action: CommandAction::ClearConversation,
        },
        CommandEntry {
            title: "Undo last message".to_string(),
            value: "session.undo".to_string(),
            category: "Session".to_string(),
            shortcut: "PgUp".to_string(),
            suggested: false,
            action: CommandAction::Undo,
        },
        CommandEntry {
            title: "Redo".to_string(),
            value: "session.redo".to_string(),
            category: "Session".to_string(),
            shortcut: "PgDn".to_string(),
            suggested: false,
            action: CommandAction::Redo,
        },
        CommandEntry {
            title: "Export session".to_string(),
            value: "session.export".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ExportSession,
        },
        CommandEntry {
            title: "Compact context".to_string(),
            value: "session.compact".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::CompactContext,
        },
        CommandEntry {
            title: "Context window".to_string(),
            value: "context.show".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowContextViz,
        },
        CommandEntry {
            title: "Prompt Stash".to_string(),
            value: "/stash".to_string(),
            category: "Session".to_string(),
            shortcut: "Ctrl+S".to_string(),
            suggested: false,
            action: CommandAction::ShowPromptStash,
        },
        CommandEntry {
            title: "External Editor".to_string(),
            value: "/editor".to_string(),
            category: "Session".to_string(),
            shortcut: "Alt+E".to_string(),
            suggested: false,
            action: CommandAction::OpenExternalEditor,
        },
        CommandEntry {
            title: "Export options".to_string(),
            value: "/export".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowExportOptions,
        },
        CommandEntry {
            title: "Provider connections".to_string(),
            value: "providers.list".to_string(),
            category: "Agent".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowProviders,
        },
        CommandEntry {
            title: "Workspaces".to_string(),
            value: "workspaces.list".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowWorkspaces,
        },
        CommandEntry {
            title: "Attach to session".to_string(),
            value: "session.attach".to_string(),
            category: "Session".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::AttachSession,
        },
        CommandEntry {
            title: "Toggle sidebar".to_string(),
            value: "sidebar.toggle".to_string(),
            category: "Session".to_string(),
            shortcut: "Alt+S".to_string(),
            suggested: false,
            action: CommandAction::ToggleSidebar,
        },
        CommandEntry {
            title: "View status".to_string(),
            value: "status.view".to_string(),
            category: "System".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowStatus,
        },
        CommandEntry {
            title: "Show cost".to_string(),
            value: "cost.show".to_string(),
            category: "System".to_string(),
            shortcut: String::new(),
            suggested: false,
            action: CommandAction::ShowCost,
        },
        CommandEntry {
            title: "Toggle theme".to_string(),
            value: "theme.toggle".to_string(),
            category: "System".to_string(),
            shortcut: String::new(),
            suggested: true,
            action: CommandAction::ToggleTheme,
        },
        CommandEntry {
            title: "Help & keybindings".to_string(),
            value: "help.show".to_string(),
            category: "System".to_string(),
            shortcut: "?".to_string(),
            suggested: false,
            action: CommandAction::ShowHelp,
        },
        CommandEntry {
            title: "Exit".to_string(),
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

    let width = dialog_width(area.width).min(area.width.saturating_sub(4));
    let height = dialog_height(area.height).min(area.height.saturating_sub(4));
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Command Palette ");
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

        let is_selected = pos == state.cursor;
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

        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(&entry.title, style.fg(theme.text)),
            shortcut_span,
        ]));
    }

    let list_para = Paragraph::new(lines);
    frame.render_widget(list_para, chunks[1]);

    let help_text = " \u{2191}\u{2193} navigate  Enter: execute  Esc: cancel  /: search ";
    let help = Span::styled(help_text, Style::default().fg(theme.text_muted));
    let help_para = Paragraph::new(help);
    frame.render_widget(help_para, chunks[2]);
}

fn compute_scroll_offset(state: &CommandPaletteState, visible_lines: usize) -> usize {
    if state.cursor < visible_lines / 2 {
        return 0;
    }
    state.cursor.saturating_sub(visible_lines / 2)
}
