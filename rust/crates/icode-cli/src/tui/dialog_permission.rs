use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;

use runtime::{PermissionMode, PermissionRequest};

use crate::tui::theme::Theme;

const BUTTON_COUNT: usize = 3;
const BTN_APPROVE: &str = "Approve";
const BTN_DENY: &str = "Deny";
const BTN_ALWAYS: &str = "Always";

/// Represents the user's decision on a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionAction {
    Approve,
    Deny,
    AlwaysAllow,
}

/// State for the permission dialog overlay.
pub struct PermissionDialogState {
    pub open: bool,
    pub request: Option<PermissionRequest>,
    pub focused_button: usize,
}

impl Default for PermissionDialogState {
    fn default() -> Self {
        Self {
            open: false,
            request: None,
            focused_button: 0,
        }
    }
}

impl PermissionDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the dialog with a permission request.
    pub fn open(&mut self, req: PermissionRequest) {
        self.open = true;
        self.request = Some(req);
        self.focused_button = 0;
    }

    /// Close the dialog and clear state.
    pub fn close(&mut self) {
        self.open = false;
        self.request = None;
        self.focused_button = 0;
    }

    /// Handle a key event and return an action if the user made a decision.
    pub fn handle_key(&mut self, code: crossterm::event::KeyCode) -> Option<PermissionAction> {
        match code {
            crossterm::event::KeyCode::Enter => {
                let action = match self.focused_button {
                    0 => PermissionAction::Approve,
                    1 => PermissionAction::Deny,
                    2 => PermissionAction::AlwaysAllow,
                    _ => PermissionAction::Approve,
                };
                self.close();
                Some(action)
            }
            crossterm::event::KeyCode::Esc => {
                self.close();
                Some(PermissionAction::Deny)
            }
            crossterm::event::KeyCode::Char('a') => {
                self.close();
                Some(PermissionAction::AlwaysAllow)
            }
            crossterm::event::KeyCode::Left => {
                if self.focused_button > 0 {
                    self.focused_button -= 1;
                } else {
                    self.focused_button = BUTTON_COUNT - 1;
                }
                None
            }
            crossterm::event::KeyCode::Right => {
                self.focused_button = (self.focused_button + 1) % BUTTON_COUNT;
                None
            }
            _ => None,
        }
    }
}

fn tool_icon(name: &str) -> &'static str {
    match name {
        "bash" | "sh" => "$",
        "read" | "cat" | "read_file" => "\u{2192}",
        "write" | "create" | "save" | "write_file" => "\u{2190}",
        "edit" | "patch" | "replace" | "edit_file" => "\u{270e}",
        "glob" | "find" | "glob_search" => "\u{2731}",
        "grep" | "search" | "grep_search" => "\u{2731}",
        "web_search" => "\u{25c7}",
        "web_fetch" | "fetch" => "%",
        "task" | "delegate" => "\u{2502}",
        "todo_write" | "todo" => "\u{2611}",
        "notebook_edit" => "N",
        _ => "\u{2699}",
    }
}

fn human_tool_name(name: &str) -> String {
    match name {
        "bash" | "sh" => "Shell".to_string(),
        "read_file" => "Read File".to_string(),
        "write_file" => "Write File".to_string(),
        "edit_file" => "Edit File".to_string(),
        "glob_search" => "Glob Search".to_string(),
        "grep_search" => "Grep Search".to_string(),
        "web_search" => "Web Search".to_string(),
        "web_fetch" => "Web Fetch".to_string(),
        "todo_write" => "Todo Write".to_string(),
        "notebook_edit" => "Notebook Edit".to_string(),
        _ => {
            let mut chars = name.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    }
}

fn humanize_action(req: &PermissionRequest) -> String {
    let tool = human_tool_name(&req.tool_name);
    match req.required_mode {
        PermissionMode::ReadOnly => format!("{tool} wants to read data"),
        PermissionMode::WorkspaceWrite => format!("{tool} wants to modify files"),
        PermissionMode::DangerFullAccess => format!("{tool} wants to run a shell command"),
        PermissionMode::Prompt => format!("{tool} requires approval"),
        PermissionMode::Allow => format!("{tool} is requesting access"),
    }
}

fn format_input_details(input: &str, max_lines: usize) -> Vec<Line<'static>> {
    if input.is_empty() {
        return vec![Line::from(Span::styled(
            "(no input details)",
            Style::default().fg(ratatui::style::Color::DarkGray),
        ))];
    }

    // Try to parse as JSON and show key-value pairs
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(input) {
        return format_json_value(&val, 0, max_lines);
    }

    // Fall back to raw text, truncated
    let lines: Vec<&str> = input.lines().collect();
    let mut result = Vec::new();
    for (i, line) in lines.iter().take(max_lines).enumerate() {
        if i == max_lines - 1 && lines.len() > max_lines {
            result.push(Line::from(Span::styled(
                format!("{line}..."),
                Style::default().fg(ratatui::style::Color::DarkGray),
            )));
        } else {
            result.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(ratatui::style::Color::DarkGray),
            )));
        }
    }
    if result.is_empty() {
        result.push(Line::from(Span::styled(
            input.chars().take(80).collect::<String>(),
            Style::default().fg(ratatui::style::Color::DarkGray),
        )));
    }
    result
}

fn format_json_value(
    val: &serde_json::Value,
    indent: usize,
    max_lines: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let prefix = "  ".repeat(indent);

    match val {
        serde_json::Value::Object(map) => {
            for (i, (key, value)) in map.iter().enumerate() {
                if lines.len() >= max_lines {
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}..."),
                        Style::default().fg(ratatui::style::Color::DarkGray),
                    )));
                    return lines;
                }
                match value {
                    serde_json::Value::String(s) => {
                        let display = if s.len() > 60 {
                            format!("{}...", &s[..57])
                        } else {
                            s.clone()
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{prefix}{key}: "),
                                Style::default().fg(ratatui::style::Color::DarkGray),
                            ),
                            Span::styled(
                                display,
                                Style::default().fg(ratatui::style::Color::White),
                            ),
                        ]));
                    }
                    serde_json::Value::Number(n) => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{prefix}{key}: "),
                                Style::default().fg(ratatui::style::Color::DarkGray),
                            ),
                            Span::styled(
                                n.to_string(),
                                Style::default().fg(ratatui::style::Color::Cyan),
                            ),
                        ]));
                    }
                    serde_json::Value::Bool(b) => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{prefix}{key}: "),
                                Style::default().fg(ratatui::style::Color::DarkGray),
                            ),
                            Span::styled(
                                b.to_string(),
                                Style::default().fg(ratatui::style::Color::Yellow),
                            ),
                        ]));
                    }
                    serde_json::Value::Null => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{prefix}{key}: "),
                                Style::default().fg(ratatui::style::Color::DarkGray),
                            ),
                            Span::styled(
                                "null",
                                Style::default().fg(ratatui::style::Color::DarkGray),
                            ),
                        ]));
                    }
                    serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{prefix}{key}: "),
                                Style::default().fg(ratatui::style::Color::DarkGray),
                            ),
                            Span::styled(
                                format!(
                                    "[{} entries]",
                                    match value {
                                        serde_json::Value::Array(a) => a.len(),
                                        serde_json::Value::Object(o) => o.len(),
                                        _ => 0,
                                    }
                                ),
                                Style::default().fg(ratatui::style::Color::DarkGray),
                            ),
                        ]));
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                if lines.len() >= max_lines {
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}..."),
                        Style::default().fg(ratatui::style::Color::DarkGray),
                    )));
                    return lines;
                }
                if let serde_json::Value::String(s) = item {
                    let display = if s.len() > 60 {
                        format!("{}...", &s[..57])
                    } else {
                        s.clone()
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{prefix}[{i}] {display}"),
                        Style::default().fg(ratatui::style::Color::White),
                    )));
                }
            }
        }
        serde_json::Value::String(s) => {
            lines.push(Line::from(Span::styled(
                s.chars().take(120).collect::<String>(),
                Style::default().fg(ratatui::style::Color::White),
            )));
        }
        _ => {
            lines.push(Line::from(Span::styled(
                val.to_string(),
                Style::default().fg(ratatui::style::Color::DarkGray),
            )));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(empty)",
            Style::default().fg(ratatui::style::Color::DarkGray),
        )));
    }
    lines
}

fn permission_mode_badge(mode: PermissionMode) -> (&'static str, ratatui::style::Color) {
    match mode {
        PermissionMode::ReadOnly => ("read-only", ratatui::style::Color::Green),
        PermissionMode::WorkspaceWrite => ("workspace-write", ratatui::style::Color::Yellow),
        PermissionMode::DangerFullAccess => ("danger-full-access", ratatui::style::Color::Red),
        PermissionMode::Prompt => ("prompt", ratatui::style::Color::Blue),
        PermissionMode::Allow => ("allow", ratatui::style::Color::Green),
    }
}

/// Render the permission dialog as a centered overlay panel.
pub fn render_permission_dialog(
    frame: &mut Frame,
    state: &mut PermissionDialogState,
    area: Rect,
    theme: Theme,
) {
    let Some(ref req) = state.request else {
        return;
    };

    // Calculate panel dimensions
    let panel_width = (area.width * 60 / 100).clamp(50, 80);
    let panel_height = 14u16; // fixed height for consistency

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(panel_width)) / 2,
        y: area.y + (area.height.saturating_sub(panel_height)) / 2,
        width: panel_width,
        height: panel_height,
    };

    // Draw semi-transparent backdrop
    let backdrop = Paragraph::new("").style(Style::default().bg(ratatui::style::Color::Black));
    frame.render_widget(backdrop, area);

    // Build panel content
    let panel_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warning))
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(theme.background_panel));

    let inner = panel_block.inner(popup_area);
    frame.render_widget(panel_block, popup_area);

    if inner.width < 20 || inner.height < 8 {
        return;
    }

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Title
    lines.push(Line::from(vec![
        Span::styled(
            " \u{26a0} ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Permission Required",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Tool name with icon
    let icon = tool_icon(&req.tool_name);
    let human_name = human_tool_name(&req.tool_name);
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {icon} "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            human_name,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Action description
    let action_text = humanize_action(req);
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(action_text, Style::default().fg(theme.text)),
    ]));
    lines.push(Line::from(""));

    // Permission mode escalation info
    let (current_label, current_color) = permission_mode_badge(req.current_mode);
    let (required_label, required_color) = permission_mode_badge(req.required_mode);
    if req.current_mode != req.required_mode {
        lines.push(Line::from(vec![
            Span::styled("    Mode: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                current_label,
                Style::default()
                    .fg(current_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" \u{2192} ", Style::default().fg(theme.text_muted)),
            Span::styled(
                required_label,
                Style::default()
                    .fg(required_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
    }

    // Reason (if any)
    if let Some(ref reason) = req.reason {
        let max_len = (inner.width as usize).saturating_sub(8);
        let reason_text = if reason.len() > max_len {
            format!("{}...", &reason[..max_len.saturating_sub(3)])
        } else {
            reason.clone()
        };
        lines.push(Line::from(vec![
            Span::styled("    Why: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                reason_text,
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        lines.push(Line::from(""));
    }

    // Input details header
    lines.push(Line::from(vec![Span::styled(
        "    Input:",
        Style::default().fg(theme.text_muted),
    )]));

    // Input details
    let detail_lines = format_input_details(&req.input, 3);
    for dl in detail_lines {
        let mut prefixed = vec![Span::raw("      ")];
        prefixed.extend(dl.spans);
        lines.push(Line::from(prefixed));
    }

    lines.push(Line::from(""));

    // Render all text content
    let text_height = lines.len() as u16;
    let text_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: text_height.min(inner.height.saturating_sub(3)),
    };

    let text_widget = Paragraph::new(lines)
        .style(Style::default().bg(theme.background_panel))
        .alignment(Alignment::Left);
    frame.render_widget(text_widget, text_area);

    // Button bar at bottom
    let buttons_y = inner.bottom().saturating_sub(3);
    let buttons = vec![(BTN_APPROVE, 0), (BTN_DENY, 1), (BTN_ALWAYS, 2)];

    let total_btn_text: usize = buttons.iter().map(|(label, _)| label.len() + 4).sum();
    let gap = if total_btn_text < inner.width as usize {
        (inner.width as usize - total_btn_text) / (buttons.len() + 1)
    } else {
        1
    };

    let mut x = inner.x + gap as u16;
    for (label, idx) in &buttons {
        let is_focused = state.focused_button == *idx;
        let btn_style = if is_focused {
            match idx {
                0 => Style::default()
                    .fg(theme.background_panel)
                    .bg(theme.success)
                    .add_modifier(Modifier::BOLD),
                1 => Style::default()
                    .fg(theme.background_panel)
                    .bg(theme.error)
                    .add_modifier(Modifier::BOLD),
                _ => Style::default()
                    .fg(theme.background_panel)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            }
        } else {
            Style::default().fg(theme.text).bg(theme.background_element)
        };

        let shortcut = match idx {
            0 => "Enter",
            1 => "Esc",
            _ => "a",
        };

        let text = format!(" [{label}] ");
        let btn_width = text.len() as u16;
        let btn_area = Rect {
            x,
            y: buttons_y,
            width: btn_width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(text).style(btn_style), btn_area);

        // Shortcut hint below
        let hint_area = Rect {
            x,
            y: buttons_y.saturating_add(1),
            width: btn_width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("({shortcut})"),
                Style::default().fg(theme.text_muted),
            )),
            hint_area,
        );

        x += btn_width + gap as u16;
    }
}
