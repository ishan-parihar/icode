use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::app::AppMode;
use crate::tui::app::AppState;
use crate::tui::theme::Theme;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugTab {
    AppState,
    EventLog,
    Memory,
    Performance,
}

impl DebugTab {
    fn label(&self) -> &str {
        match self {
            DebugTab::AppState => "State",
            DebugTab::EventLog => "Events",
            DebugTab::Memory => "Memory",
            DebugTab::Performance => "Perf",
        }
    }

    fn all() -> &'static [DebugTab] {
        &[
            DebugTab::AppState,
            DebugTab::EventLog,
            DebugTab::Memory,
            DebugTab::Performance,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct DebugPanelState {
    pub open: bool,
    pub tab: DebugTab,
}

impl DebugPanelState {
    pub fn new() -> Self {
        Self {
            open: false,
            tab: DebugTab::AppState,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn next_tab(&mut self) {
        let tabs = DebugTab::all();
        let idx = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.tab = tabs[(idx + 1) % tabs.len()];
    }

    pub fn prev_tab(&mut self) {
        let tabs = DebugTab::all();
        let idx = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
        self.tab = tabs[(idx + tabs.len() - 1) % tabs.len()];
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> DebugAction {
        use crossterm::event::{KeyCode, KeyModifiers};
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) | (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.close();
                DebugAction::Close
            }
            (_, KeyCode::Char('q')) => {
                self.close();
                DebugAction::Close
            }
            (KeyModifiers::SHIFT, KeyCode::Tab) | (KeyModifiers::CONTROL, KeyCode::Left) => {
                self.prev_tab();
                DebugAction::PrevTab
            }
            (_, KeyCode::Tab) | (KeyModifiers::CONTROL, KeyCode::Right) => {
                self.next_tab();
                DebugAction::NextTab
            }
            (_, KeyCode::Char('1')) => {
                self.tab = DebugTab::AppState;
                DebugAction::None
            }
            (_, KeyCode::Char('2')) => {
                self.tab = DebugTab::EventLog;
                DebugAction::None
            }
            (_, KeyCode::Char('3')) => {
                self.tab = DebugTab::Memory;
                DebugAction::None
            }
            (_, KeyCode::Char('4')) => {
                self.tab = DebugTab::Performance;
                DebugAction::None
            }
            _ => DebugAction::None,
        }
    }
}

impl Default for DebugPanelState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum DebugAction {
    None,
    Close,
    NextTab,
    PrevTab,
}

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

pub fn render_debug_panel(
    frame: &mut Frame,
    state: &DebugPanelState,
    area: Rect,
    theme: Theme,
    app: &AppState,
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
            " Debug Panel ",
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
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Tab bar
    render_tab_bar(frame, state, chunks[0], theme);

    // Tab content
    let content_area = chunks[1];
    match state.tab {
        DebugTab::AppState => render_app_state_tab(frame, content_area, theme, app),
        DebugTab::EventLog => render_event_log_tab(frame, content_area, theme, app),
        DebugTab::Memory => render_memory_tab(frame, content_area, theme),
        DebugTab::Performance => render_performance_tab(frame, content_area, theme, app),
    }

    // Footer
    let hint = Span::styled(
        " Tab: next tab  •  Shift+Tab: prev tab  •  1-4: jump  •  Esc/q: close ",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[2]);
}

fn render_tab_bar(frame: &mut Frame, state: &DebugPanelState, area: Rect, theme: Theme) {
    let tabs = DebugTab::all();
    let mut spans = Vec::new();

    for (i, tab) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(theme.border)));
        }
        let is_active = state.tab == *tab;
        let style = if is_active {
            Style::default()
                .fg(theme.background)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_app_state_tab(frame: &mut Frame, area: Rect, theme: Theme, app: &AppState) {
    let lines = vec![
        Line::from(vec![
            Span::styled("Session:  ", Style::default().fg(theme.text_muted)),
            Span::styled(&app.session.id, Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Model:    ", Style::default().fg(theme.text_muted)),
            Span::styled(&app.session.model, Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Messages: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}", app.messages.len()),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("Tools:    ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}", app.tools.len()),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("Turns:    ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}", app.session.turns),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("Tokens:   ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!(
                    "{} in / {} out",
                    app.session.input_tokens, app.session.output_tokens
                ),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("Cost:     ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("${:.4}", app.session.cumulative_cost),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("Streaming:", Style::default().fg(theme.text_muted)),
            Span::styled(
                (if app.is_streaming { "yes" } else { "no" }).to_string(),
                Style::default().fg(if app.is_streaming {
                    theme.warning
                } else {
                    theme.text
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Thinking: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                (if app.is_thinking { "yes" } else { "no" }).to_string(),
                Style::default().fg(if app.is_thinking {
                    theme.warning
                } else {
                    theme.text
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Mode:     ", Style::default().fg(theme.text_muted)),
            Span::styled(format!("{:?}", app.mode), Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Sidebar:  ", Style::default().fg(theme.text_muted)),
            Span::styled(
                (if app.sidebar_visible {
                    "visible"
                } else {
                    "hidden"
                })
                .to_string(),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("LSP:      ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}", app.lsp_count),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("MCP:      ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}", app.mcp_dialog.servers.len()),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("Skills:   ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}", app.skill_count),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("Plugins:  ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{}", app.plugin_count),
                Style::default().fg(theme.text),
            ),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_event_log_tab(frame: &mut Frame, area: Rect, theme: Theme, app: &AppState) {
    let mut lines = Vec::new();

    let max_events = area.height.saturating_sub(2) as usize;
    let recent_tools: Vec<_> = app.tools.iter().rev().take(max_events).collect();

    if recent_tools.is_empty() {
        lines.push(Line::from(Span::styled(
            "No events recorded",
            Style::default().fg(theme.text_muted),
        )));
    } else {
        for tool in recent_tools {
            let status_icon = match tool.status {
                crate::tui::app::ToolStatus::Completed => "✓",
                crate::tui::app::ToolStatus::Failed => "✗",
                crate::tui::app::ToolStatus::Running => "⟳",
                crate::tui::app::ToolStatus::Pending => "○",
            };
            let status_color = match tool.status {
                crate::tui::app::ToolStatus::Completed => theme.success,
                crate::tui::app::ToolStatus::Failed => theme.error,
                crate::tui::app::ToolStatus::Running => theme.warning,
                crate::tui::app::ToolStatus::Pending => theme.text_muted,
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{status_icon} "), Style::default().fg(status_color)),
                Span::styled(&tool.name, Style::default().fg(theme.text)),
                Span::styled(": ", Style::default().fg(theme.text_muted)),
                Span::styled(&tool.input_summary, Style::default().fg(theme.text_muted)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_memory_tab(frame: &mut Frame, area: Rect, theme: Theme) {
    let mut lines = Vec::new();

    // Read /proc/self/status on Linux for memory info
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:")
                    || line.starts_with("VmSize:")
                    || line.starts_with("VmPeak:")
                {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let key = parts[0].trim_end_matches(':');
                        let val = parts[1];
                        let unit = parts[2];
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{key:<8}"),
                                Style::default().fg(theme.text_muted),
                            ),
                            Span::styled(format!("{val} {unit}"), Style::default().fg(theme.text)),
                        ]));
                    }
                }
            }
        }
    }

    // Fallback for non-Linux: use std::process info
    #[cfg(not(target_os = "linux"))]
    {
        let pid = std::process::id();
        lines.push(Line::from(vec![
            Span::styled("PID:      ", Style::default().fg(theme.text_muted)),
            Span::styled(format!("{}", pid), Style::default().fg(theme.text)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "Memory info not available on this platform",
            Style::default().fg(theme.text_muted),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_performance_tab(frame: &mut Frame, area: Rect, theme: Theme, app: &AppState) {
    let mut lines = Vec::new();

    // Last turn duration
    if let Some(duration) = app.last_turn_duration {
        let ms = duration.as_millis();
        let label = if ms < 100 {
            (format!("{ms}ms"), theme.success)
        } else if ms < 1000 {
            (format!("{ms}ms"), theme.warning)
        } else {
            (format!("{:.1}s", ms as f64 / 1000.0), theme.error)
        };
        lines.push(Line::from(vec![
            Span::styled("Last turn: ", Style::default().fg(theme.text_muted)),
            Span::styled(label.0, Style::default().fg(label.1)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Last turn: ", Style::default().fg(theme.text_muted)),
            Span::styled("not measured", Style::default().fg(theme.text_muted)),
        ]));
    }

    // Current turn elapsed
    if let Some(elapsed) = app.turn_elapsed() {
        lines.push(Line::from(vec![
            Span::styled("Current:   ", Style::default().fg(theme.text_muted)),
            Span::styled(elapsed, Style::default().fg(theme.text)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Current:   ", Style::default().fg(theme.text_muted)),
            Span::styled("idle", Style::default().fg(theme.text_muted)),
        ]));
    }

    // Total messages
    lines.push(Line::from(vec![
        Span::styled("Messages:  ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format!("{}", app.messages.len()),
            Style::default().fg(theme.text),
        ),
    ]));

    // Total tools
    lines.push(Line::from(vec![
        Span::styled("Tool calls: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format!("{}", app.tools.len()),
            Style::default().fg(theme.text),
        ),
    ]));

    // Token throughput
    let total_tokens = app.session.input_tokens + app.session.output_tokens;
    lines.push(Line::from(vec![
        Span::styled("Total tok: ", Style::default().fg(theme.text_muted)),
        Span::styled(format!("{total_tokens}"), Style::default().fg(theme.text)),
    ]));

    // Turn count
    lines.push(Line::from(vec![
        Span::styled("Turns:     ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format!("{}", app.session.turns),
            Style::default().fg(theme.text),
        ),
    ]));

    frame.render_widget(Paragraph::new(lines), area);
}

pub fn render_debug_panel_ext(
    frame: &mut Frame,
    state: &DebugPanelState,
    area: Rect,
    theme: Theme,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    context_window: u32,
    turns: u32,
    message_count: usize,
    is_streaming: bool,
    connected: bool,
    mode: &AppMode,
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
        .title(" Debug Panel ");
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let mut lines = vec![
        Line::from(Span::styled(
            "Model:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(model),
        Line::from(""),
        Line::from(Span::styled(
            "Tokens:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  Input: {input_tokens}")),
        Line::from(format!("  Output: {output_tokens}")),
        Line::from(format!("  Context: {context_window}")),
        Line::from(format!(
            "  Used: {}%",
            if context_window > 0 {
                (input_tokens + output_tokens) as f64 / context_window as f64 * 100.0
            } else {
                0.0
            } as u32
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Session:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  Turns: {turns}")),
        Line::from(format!("  Messages: {message_count}")),
        Line::from(format!("  Streaming: {is_streaming}")),
        Line::from(format!("  Connected: {connected}")),
        Line::from(""),
        Line::from(Span::styled(
            "Mode:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  {mode:?}")),
    ];

    let max_lines = inner.height as usize;
    if lines.len() > max_lines {
        lines.truncate(max_lines);
    }

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
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
        let state = DebugPanelState::new();
        assert!(!state.open);
        assert_eq!(state.tab, DebugTab::AppState);
    }

    #[test]
    fn test_open_close_toggle() {
        let mut state = DebugPanelState::new();
        state.open();
        assert!(state.open);
        state.close();
        assert!(!state.open);
        state.toggle();
        assert!(state.open);
        state.toggle();
        assert!(!state.open);
    }

    #[test]
    fn test_tab_navigation() {
        let mut state = DebugPanelState::new();
        assert_eq!(state.tab, DebugTab::AppState);

        state.next_tab();
        assert_eq!(state.tab, DebugTab::EventLog);

        state.next_tab();
        assert_eq!(state.tab, DebugTab::Memory);

        state.next_tab();
        assert_eq!(state.tab, DebugTab::Performance);

        // Wrap around
        state.next_tab();
        assert_eq!(state.tab, DebugTab::AppState);

        // Prev tab
        state.prev_tab();
        assert_eq!(state.tab, DebugTab::Performance);
    }

    #[test]
    fn test_handle_key_esc_closes() {
        let mut state = DebugPanelState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(action, DebugAction::Close));
        assert!(!state.open);
    }

    #[test]
    fn test_handle_key_q_closes() {
        let mut state = DebugPanelState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(matches!(action, DebugAction::Close));
    }

    #[test]
    fn test_handle_key_tab_switches() {
        let mut state = DebugPanelState::new();
        state.open();
        let action = state.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(matches!(action, DebugAction::NextTab));
        assert_eq!(state.tab, DebugTab::EventLog);
    }

    #[test]
    fn test_handle_key_number_jumps() {
        let mut state = DebugPanelState::new();
        state.open();

        state.handle_key(key(KeyCode::Char('3'), KeyModifiers::NONE));
        assert_eq!(state.tab, DebugTab::Memory);

        state.handle_key(key(KeyCode::Char('4'), KeyModifiers::NONE));
        assert_eq!(state.tab, DebugTab::Performance);

        state.handle_key(key(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(state.tab, DebugTab::AppState);
    }

    #[test]
    fn test_debug_tab_label() {
        assert_eq!(DebugTab::AppState.label(), "State");
        assert_eq!(DebugTab::EventLog.label(), "Events");
        assert_eq!(DebugTab::Memory.label(), "Memory");
        assert_eq!(DebugTab::Performance.label(), "Perf");
    }

    #[test]
    fn test_default_implementation() {
        let state = DebugPanelState::default();
        assert!(!state.open);
    }
}
