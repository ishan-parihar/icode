use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 56;
const MIN_HEIGHT: u16 = 14;

fn dialog_width(term_width: u16) -> u16 {
    if term_width >= 128 {
        80
    } else if term_width >= 96 {
        72
    } else {
        MIN_WIDTH
    }
}

fn dialog_height(_term_height: u16) -> u16 {
    MIN_HEIGHT
}

/// Export option labels in display order.
const OPTION_LABELS: &[&str] = &[
    "Include thinking content",
    "Include tool call details",
    "Include session metadata",
    "Include timestamps",
];

const FORMAT_LABELS: &[&str] = &["Plain text (.txt)", "Markdown (.md)", "JSON (.json)"];

#[derive(Debug, Clone)]
pub struct ExportOptionsState {
    pub open: bool,
    pub filename: String,
    pub format_idx: usize,
    pub include_thinking: bool,
    pub include_tool_details: bool,
    pub include_metadata: bool,
    pub include_timestamps: bool,
    pub selected: usize,
    pub editing_filename: bool,
}

impl ExportOptionsState {
    pub fn new() -> Self {
        Self {
            open: false,
            filename: Self::default_filename(),
            format_idx: 1,
            include_thinking: true,
            include_tool_details: true,
            include_metadata: true,
            include_timestamps: true,
            selected: 0,
            editing_filename: false,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.editing_filename = false;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.editing_filename = false;
    }

    fn default_filename() -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        format!("export-{secs}.txt")
    }

    fn checkbox_value(&self, idx: usize) -> bool {
        match idx {
            0 => self.include_thinking,
            1 => self.include_tool_details,
            2 => self.include_metadata,
            3 => self.include_timestamps,
            _ => false,
        }
    }

    fn set_checkbox(&mut self, idx: usize, value: bool) {
        match idx {
            0 => self.include_thinking = value,
            1 => self.include_tool_details = value,
            2 => self.include_metadata = value,
            3 => self.include_timestamps = value,
            _ => {}
        }
    }

    pub fn selected_format(&self) -> &'static str {
        FORMAT_LABELS
            .get(self.format_idx)
            .copied()
            .unwrap_or(FORMAT_LABELS[0])
    }

    pub fn cycle_format(&mut self) {
        self.format_idx = (self.format_idx + 1) % FORMAT_LABELS.len();
        let ext = match self.format_idx {
            0 => ".txt",
            1 => ".md",
            2 => ".json",
            _ => ".txt",
        };
        let base = self
            .filename
            .rsplit_once('.')
            .map_or(self.filename.as_str(), |(b, _)| b);
        self.filename = format!("{base}{ext}");
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ExportAction {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                self.close();
                ExportAction::Close
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                ExportAction::Close
            }
            (_, KeyCode::Enter) => {
                if self.editing_filename {
                    self.editing_filename = false;
                    ExportAction::None
                } else {
                    self.close();
                    ExportAction::Confirm
                }
            }
            (_, KeyCode::Up) => {
                if !self.editing_filename && self.selected > 0 {
                    self.selected -= 1;
                }
                ExportAction::None
            }
            (_, KeyCode::Down) => {
                let max_sel = OPTION_LABELS.len() + 1;
                if !self.editing_filename && self.selected < max_sel {
                    self.selected += 1;
                }
                ExportAction::None
            }
            (_, KeyCode::Char(' ')) => {
                if !self.editing_filename && self.selected < OPTION_LABELS.len() {
                    let current = self.checkbox_value(self.selected);
                    self.set_checkbox(self.selected, !current);
                    ExportAction::Toggle(self.selected)
                } else {
                    ExportAction::None
                }
            }
            (_, KeyCode::Char('x')) => {
                self.cycle_format();
                ExportAction::None
            }
            (_, KeyCode::Char('f')) => {
                if !self.editing_filename && self.selected == OPTION_LABELS.len() + 1 {
                    self.editing_filename = true;
                    ExportAction::EditFilename
                } else {
                    ExportAction::None
                }
            }
            (_, KeyCode::Backspace) => {
                if self.editing_filename {
                    self.filename.pop();
                    ExportAction::None
                } else if self.selected < OPTION_LABELS.len() {
                    self.set_checkbox(self.selected, false);
                    ExportAction::Toggle(self.selected)
                } else {
                    ExportAction::None
                }
            }
            (_, KeyCode::Char(c)) => {
                if self.editing_filename {
                    self.filename.push(c);
                    ExportAction::None
                } else {
                    ExportAction::None
                }
            }
            _ => ExportAction::None,
        }
    }
}

impl Default for ExportOptionsState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum ExportAction {
    None,
    Close,
    Confirm,
    Toggle(usize),
    EditFilename,
}

pub fn render_export_options_dialog(
    frame: &mut Frame,
    state: &ExportOptionsState,
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
            " Export Options ",
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
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Filename row
    let is_file_selected = state.selected > OPTION_LABELS.len();
    let filename_style = if is_file_selected || state.editing_filename {
        Style::default()
            .fg(theme.background)
            .bg(theme.primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let filename_line = if state.editing_filename {
        Line::from(vec![
            Span::styled("Filename: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}_", state.filename),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("Filename: ", Style::default().fg(theme.text_muted)),
            Span::styled(&state.filename, filename_style),
            Span::styled(" (f to edit)", Style::default().fg(theme.text_muted)),
        ])
    };
    frame.render_widget(Paragraph::new(filename_line), chunks[0]);

    let format_selected = state.selected == OPTION_LABELS.len();
    let format_style = if format_selected {
        Style::default()
            .fg(theme.background)
            .bg(theme.primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };
    let format_line = Line::from(vec![
        Span::styled("Format:   ", Style::default().fg(theme.text_muted)),
        Span::styled(state.selected_format(), format_style),
        Span::styled(" (x to cycle)", Style::default().fg(theme.text_muted)),
    ]);
    frame.render_widget(Paragraph::new(format_line), chunks[1]);

    // Separator
    frame.render_widget(
        Paragraph::new(Span::styled(
            "── Options ──",
            Style::default().fg(theme.border),
        )),
        chunks[2],
    );

    // Option checkboxes
    for (idx, &label) in OPTION_LABELS.iter().enumerate() {
        let is_selected = state.selected == idx + 1 && !state.editing_filename;
        let value = state.checkbox_value(idx);
        let checkbox = if value { "[✓]" } else { "[ ]" };

        let style = if is_selected {
            Style::default()
                .fg(theme.background)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        let line = Line::from(vec![
            Span::styled("  ", style),
            Span::styled(
                checkbox,
                if value {
                    Style::default().fg(theme.success)
                } else {
                    Style::default().fg(theme.text_muted)
                },
            ),
            Span::styled(" ", style),
            Span::styled(label, style),
            Span::styled(" (space)", Style::default().fg(theme.text_muted)),
        ]);
        frame.render_widget(Paragraph::new(line), chunks[3 + idx]);
    }

    // Hint row
    let hint = Span::styled(
        "Enter: export  •  Space: toggle  •  f: edit filename  •  ↑↓: navigate  •  Esc: cancel",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_new_state_defaults() {
        let state = ExportOptionsState::new();
        assert!(!state.open);
        assert!(state.include_thinking);
        assert!(state.include_tool_details);
        assert!(state.include_metadata);
        assert!(state.include_timestamps);
        assert_eq!(state.selected, 0);
        assert!(!state.editing_filename);
        assert!(state.filename.starts_with("export-"));
        assert!(state.filename.ends_with(".txt"));
        assert_eq!(state.format_idx, 1);
    }

    #[test]
    fn test_open_close() {
        let mut state = ExportOptionsState::new();
        state.open();
        assert!(state.open);
        state.close();
        assert!(!state.open);
    }

    #[test]
    fn test_esc_closes() {
        let mut state = ExportOptionsState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(action, ExportAction::Close));
        assert!(!state.open);
    }

    #[test]
    fn test_ctrl_c_closes() {
        let mut state = ExportOptionsState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(action, ExportAction::Close));
        assert!(!state.open);
    }

    #[test]
    fn test_enter_confirms() {
        let mut state = ExportOptionsState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(action, ExportAction::Confirm));
        assert!(!state.open);
    }

    #[test]
    fn test_space_toggles_option() {
        let mut state = ExportOptionsState::new();
        state.open();

        // First option is include_thinking, which defaults to true
        let action = state.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(action, ExportAction::Toggle(0)));
        assert!(!state.include_thinking);

        // Toggle back
        let action = state.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(action, ExportAction::Toggle(0)));
        assert!(state.include_thinking);
    }

    #[test]
    fn test_toggle_all_options() {
        let mut state = ExportOptionsState::new();
        state.open();

        // Toggle each option off then on
        for idx in 0..OPTION_LABELS.len() {
            state.selected = idx;
            state.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE));
            assert!(!state.checkbox_value(idx));
            state.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE));
            assert!(state.checkbox_value(idx));
        }
    }

    #[test]
    fn test_navigation() {
        let mut state = ExportOptionsState::new();
        state.open();

        assert_eq!(state.selected, 0);
        state.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.selected, 1);
        state.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.selected, 2);
        state.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.selected, 3);
        state.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
        // 4 = filename row
        assert_eq!(state.selected, 4);

        state.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.selected, 3);
        state.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn test_filename_editing() {
        let mut state = ExportOptionsState::new();
        state.open();

        state.selected = OPTION_LABELS.len() + 1;
        let action = state.handle_key(key(KeyCode::Char('f'), KeyModifiers::NONE));
        assert!(matches!(action, ExportAction::EditFilename));
        assert!(state.editing_filename);

        // Type in filename
        state.handle_key(key(KeyCode::Char('m'), KeyModifiers::NONE));
        state.handle_key(key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(state.filename.contains("my"));

        // Backspace
        state.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(!state.filename.ends_with('y'));

        // Enter exits edit mode
        let action = state.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(action, ExportAction::None));
        assert!(!state.editing_filename);
    }

    #[test]
    fn test_checkbox_value_and_set() {
        let mut state = ExportOptionsState::new();
        state.include_thinking = false;
        state.include_tool_details = true;

        assert!(!state.checkbox_value(0));
        assert!(state.checkbox_value(1));

        state.set_checkbox(0, true);
        assert!(state.checkbox_value(0));

        state.set_checkbox(2, true);
        assert!(state.include_metadata);
    }

    #[test]
    fn test_default_implementation() {
        let state = ExportOptionsState::default();
        assert!(!state.open);
    }
}
