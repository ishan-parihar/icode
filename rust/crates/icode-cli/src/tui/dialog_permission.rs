use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use runtime::{PermissionMode, PermissionRequest};

use crate::tui::popup_utils;
use crate::tui::theme::Theme;

// ---------------------------------------------------------------------------
// Public API — unchanged
// ---------------------------------------------------------------------------

/// Represents the user's decision on a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionAction {
    Approve,
    Deny,
    AlwaysAllow,
}

// ---------------------------------------------------------------------------
// Stage enum
// ---------------------------------------------------------------------------

/// Multi-stage flow for the permission dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionStage {
    #[default]
    Permission,
    AlwaysConfirm,
    RejectMessage,
}

// ---------------------------------------------------------------------------
// Display info
// ---------------------------------------------------------------------------

/// Tool-specific display information for rendering the permission dialog.
#[derive(Debug, Clone)]
pub struct PermissionDisplayInfo {
    pub icon: char,
    pub title: String,
    pub body: Vec<String>,
}

/// Map a permission request to tool-specific display info.
fn tool_display_info(req: &PermissionRequest) -> PermissionDisplayInfo {
    let name = req.tool_name.as_str();
    let input = &req.input;

    // Try to extract filepath from JSON input for file-related tools
    let filepath = extract_filepath(input);

    match name {
        "bash" | "sh" => PermissionDisplayInfo {
            icon: '$',
            title: "Shell command".to_string(),
            body: vec![input.clone()],
        },
        "edit" | "write_file" => PermissionDisplayInfo {
            icon: '\u{2192}',
            title: format!("Edit {}", filepath.as_deref().unwrap_or("file")),
            body: filepath.map_or_else(|| vec![input.clone()], |p| vec![p]),
        },
        "read" | "read_file" => PermissionDisplayInfo {
            icon: '\u{2192}',
            title: format!("Read {}", filepath.as_deref().unwrap_or("file")),
            body: filepath.map_or_else(|| vec![input.clone()], |p| vec![p]),
        },
        "glob_search" => PermissionDisplayInfo {
            icon: '\u{2731}',
            title: "Glob pattern".to_string(),
            body: vec![extract_pattern(input).unwrap_or_else(|| input.clone())],
        },
        "grep_search" => PermissionDisplayInfo {
            icon: '\u{2731}',
            title: "Grep pattern".to_string(),
            body: vec![extract_pattern(input).unwrap_or_else(|| input.clone())],
        },
        "list" | "glob" => PermissionDisplayInfo {
            icon: '\u{2192}',
            title: "List directory".to_string(),
            body: vec![extract_path(input).unwrap_or_else(|| input.clone())],
        },
        "task" | "agent" => PermissionDisplayInfo {
            icon: '#',
            title: "Agent Task".to_string(),
            body: vec![extract_description(input).unwrap_or_else(|| input.clone())],
        },
        "web_fetch" => PermissionDisplayInfo {
            icon: '%',
            title: "WebFetch".to_string(),
            body: vec![extract_url(input).unwrap_or_else(|| input.clone())],
        },
        "web_search" => PermissionDisplayInfo {
            icon: '\u{25c7}',
            title: "Web Search".to_string(),
            body: vec![extract_query(input).unwrap_or_else(|| input.clone())],
        },
        _ => {
            let human = human_tool_name(name);
            PermissionDisplayInfo {
                icon: '\u{2699}',
                title: format!("Tool: {human}"),
                body: vec![input.clone()],
            }
        }
    }
}

// JSON input helpers -------------------------------------------------------

fn extract_json_field(input: &str, field: &str) -> Option<String> {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(input) {
        if let Some(s) = val.get(field).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn extract_filepath(input: &str) -> Option<String> {
    extract_json_field(input, "path")
        .or_else(|| extract_json_field(input, "file_path"))
        .or_else(|| extract_json_field(input, "filepath"))
}

fn extract_path(input: &str) -> Option<String> {
    extract_json_field(input, "path").or_else(|| extract_json_field(input, "directory"))
}

fn extract_pattern(input: &str) -> Option<String> {
    extract_json_field(input, "pattern")
}

fn extract_description(input: &str) -> Option<String> {
    extract_json_field(input, "description").or_else(|| extract_json_field(input, "prompt"))
}

fn extract_url(input: &str) -> Option<String> {
    extract_json_field(input, "url")
}

fn extract_query(input: &str) -> Option<String> {
    extract_json_field(input, "query")
}

// ---------------------------------------------------------------------------
// Legacy helpers (kept for compatibility and fallback rendering)
// ---------------------------------------------------------------------------

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

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(input) {
        return format_json_value(&val, 0, max_lines);
    }

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
                        let display = if s.chars().count() > 60 {
                            format!("{}...", s.chars().take(57).collect::<String>())
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
                    let display = if s.chars().count() > 60 {
                        format!("{}...", s.chars().take(57).collect::<String>())
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

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// State for the permission dialog overlay with multi-stage flow.
pub struct PermissionDialogState {
    pub open: bool,
    pub request: Option<PermissionRequest>,
    pub focused_button: usize,
    pub stage: PermissionStage,
    pub reject_message: String,
    pub always_patterns: Vec<String>,
}

impl Default for PermissionDialogState {
    fn default() -> Self {
        Self {
            open: false,
            request: None,
            focused_button: 0,
            stage: PermissionStage::Permission,
            reject_message: String::new(),
            always_patterns: Vec::new(),
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
        self.stage = PermissionStage::Permission;
        self.reject_message = String::new();
        self.always_patterns = Vec::new();
    }

    /// Close the dialog and clear state.
    pub fn close(&mut self) {
        self.open = false;
        self.request = None;
        self.focused_button = 0;
        self.stage = PermissionStage::Permission;
        self.reject_message = String::new();
        self.always_patterns = Vec::new();
    }

    /// Handle a key event and return an action if the user made a decision.
    pub fn handle_key(&mut self, code: crossterm::event::KeyCode) -> Option<PermissionAction> {
        match self.stage {
            PermissionStage::Permission => self.handle_permission_key(code),
            PermissionStage::AlwaysConfirm => self.handle_always_key(code),
            PermissionStage::RejectMessage => self.handle_reject_key(code),
        }
    }

    fn handle_permission_key(
        &mut self,
        code: crossterm::event::KeyCode,
    ) -> Option<PermissionAction> {
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
                // Transition to RejectMessage stage
                self.stage = PermissionStage::RejectMessage;
                self.reject_message = String::new();
                None
            }
            crossterm::event::KeyCode::Char('a' | 'A') => {
                // Transition to AlwaysConfirm stage
                self.stage = PermissionStage::AlwaysConfirm;
                None
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

    fn handle_always_key(&mut self, code: crossterm::event::KeyCode) -> Option<PermissionAction> {
        match code {
            crossterm::event::KeyCode::Enter => {
                self.close();
                Some(PermissionAction::AlwaysAllow)
            }
            crossterm::event::KeyCode::Esc => {
                // Back to Permission stage
                self.stage = PermissionStage::Permission;
                None
            }
            _ => None,
        }
    }

    fn handle_reject_key(&mut self, code: crossterm::event::KeyCode) -> Option<PermissionAction> {
        match code {
            crossterm::event::KeyCode::Enter => {
                self.close();
                Some(PermissionAction::Deny)
            }
            crossterm::event::KeyCode::Esc => {
                // Back to Permission stage
                self.stage = PermissionStage::Permission;
                None
            }
            crossterm::event::KeyCode::Backspace => {
                self.reject_message.pop();
                None
            }
            crossterm::event::KeyCode::Char(c) => {
                self.reject_message.push(c);
                None
            }
            _ => None,
        }
    }
}

const BUTTON_COUNT: usize = 3;
const BTN_APPROVE: &str = "Allow once";
const BTN_DENY: &str = "Reject";
const BTN_ALWAYS: &str = "Allow always";

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

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

    match state.stage {
        PermissionStage::Permission => render_permission_stage(frame, state, req, area, theme),
        PermissionStage::AlwaysConfirm => render_always_stage(frame, state, req, area, theme),
        PermissionStage::RejectMessage => render_reject_stage(frame, state, req, area, theme),
    }
}

fn render_permission_stage(
    frame: &mut Frame,
    state: &PermissionDialogState,
    req: &PermissionRequest,
    area: Rect,
    theme: Theme,
) {
    let display = tool_display_info(req);

    // Build content lines to determine height
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

    // Tool icon + title
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", display.icon),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            display.title,
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
        let max_len = 60;
        let reason_text = if reason.chars().count() > max_len {
            format!(
                "{}...",
                reason
                    .chars()
                    .take(max_len.saturating_sub(3))
                    .collect::<String>()
            )
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

    // Input details
    let detail_lines = format_input_details(&req.input, 3);
    lines.push(Line::from(vec![Span::styled(
        "    Input:",
        Style::default().fg(theme.text_muted),
    )]));
    for dl in detail_lines {
        let mut prefixed = vec![Span::raw("      ")];
        prefixed.extend(dl.spans);
        lines.push(Line::from(prefixed));
    }

    let content_height = lines.len() as u16 + 3u16; // buttons (1) + gap (1) + hint (1)

    let popup_area = popup_utils::popup_dimensions(area, 0.5, 30, 60, 0.5, content_height);

    // Block with left border
    let block =
        popup_utils::left_border_block(theme, theme.warning, "", Some(theme.background_panel));
    popup_utils::clear_area(frame, popup_area);
    frame.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    // Render text content
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

    // Button bar
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

        let text = format!(" [{label}] ");
        let btn_width = text.len() as u16;
        let btn_area = Rect {
            x,
            y: buttons_y,
            width: btn_width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(text).style(btn_style), btn_area);

        x += btn_width + gap as u16;
    }

    // Hint bar
    let hint_hints = [
        ("←→", "select"),
        ("Enter", "confirm"),
        ("A", "always"),
        ("Esc", "reject"),
    ];
    let hint_area = Rect {
        x: inner.x,
        y: inner.bottom().saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    popup_utils::render_hint_bar(frame, hint_area, &hint_hints, theme);
}

fn render_always_stage(
    frame: &mut Frame,
    state: &PermissionDialogState,
    req: &PermissionRequest,
    area: Rect,
    theme: Theme,
) {
    let display = tool_display_info(req);

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Title
    lines.push(Line::from(vec![
        Span::styled(
            " \u{2713} ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Always Allow",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Confirm message
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", display.icon),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            display.title.clone(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![Span::styled(
        "    Always allow this tool for this session?",
        Style::default().fg(theme.text),
    )]));
    lines.push(Line::from(""));

    // Patterns that would be always allowed
    lines.push(Line::from(vec![
        Span::styled("    Pattern: ", Style::default().fg(theme.text_muted)),
        Span::styled(req.tool_name.clone(), Style::default().fg(theme.primary)),
    ]));

    let content_height = lines.len() as u16 + 3u16;

    let popup_area = popup_utils::popup_dimensions(area, 0.5, 30, 60, 0.5, content_height);

    let block =
        popup_utils::left_border_block(theme, theme.primary, "", Some(theme.background_panel));
    popup_utils::clear_area(frame, popup_area);
    frame.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

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

    // Confirm button
    let btn_area = Rect {
        x: inner.x + 2,
        y: inner.bottom().saturating_sub(3),
        width: 14,
        height: 1,
    };
    let btn_style = Style::default()
        .fg(theme.background_panel)
        .bg(theme.primary)
        .add_modifier(Modifier::BOLD);
    frame.render_widget(Paragraph::new(" [Confirm] ").style(btn_style), btn_area);

    // Hint bar
    let hint_hints = [("Enter", "confirm"), ("Esc", "cancel")];
    let hint_area = Rect {
        x: inner.x,
        y: inner.bottom().saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    popup_utils::render_hint_bar(frame, hint_area, &hint_hints, theme);
}

fn render_reject_stage(
    frame: &mut Frame,
    state: &PermissionDialogState,
    req: &PermissionRequest,
    area: Rect,
    theme: Theme,
) {
    let display = tool_display_info(req);

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Title
    lines.push(Line::from(vec![
        Span::styled(
            " \u{2717} ",
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Reject Request",
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Tool info
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", display.icon),
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            display.title.clone(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Reason label
    lines.push(Line::from(vec![Span::styled(
        "    Reason (optional):",
        Style::default().fg(theme.text_muted),
    )]));

    // Current input with cursor
    let cursor_char = if state.reject_message.is_empty() {
        "\u{2588}"
    } else {
        "\u{2588}"
    };
    lines.push(Line::from(vec![
        Span::raw("    > "),
        Span::styled(
            format!("{}{}", state.reject_message, cursor_char),
            Style::default().fg(theme.text),
        ),
    ]));

    let content_height = lines.len() as u16 + 3u16;

    let popup_area = popup_utils::popup_dimensions(area, 0.5, 30, 60, 0.5, content_height);

    let block =
        popup_utils::left_border_block(theme, theme.error, "", Some(theme.background_panel));
    popup_utils::clear_area(frame, popup_area);
    frame.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

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

    // Submit button
    let btn_area = Rect {
        x: inner.x + 2,
        y: inner.bottom().saturating_sub(3),
        width: 12,
        height: 1,
    };
    let btn_style = Style::default()
        .fg(theme.background_panel)
        .bg(theme.error)
        .add_modifier(Modifier::BOLD);
    frame.render_widget(Paragraph::new(" [Reject] ").style(btn_style), btn_area);

    // Hint bar
    let hint_hints = [("Enter", "submit"), ("Esc", "cancel")];
    let hint_area = Rect {
        x: inner.x,
        y: inner.bottom().saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    popup_utils::render_hint_bar(frame, hint_area, &hint_hints, theme);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(tool_name: &str, input: &str) -> PermissionRequest {
        PermissionRequest {
            tool_name: tool_name.to_string(),
            input: input.to_string(),
            current_mode: PermissionMode::ReadOnly,
            required_mode: PermissionMode::WorkspaceWrite,
            reason: None,
        }
    }

    // --- Stage transition tests ---

    #[test]
    fn test_open_resets_to_permission_stage() {
        let mut state = PermissionDialogState::new();
        state.stage = PermissionStage::RejectMessage;
        state.reject_message = "test".to_string();

        state.open(make_request("bash", "ls"));

        assert_eq!(state.stage, PermissionStage::Permission);
        assert_eq!(state.reject_message, "");
    }

    #[test]
    fn test_close_resets_stage() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.stage = PermissionStage::AlwaysConfirm;

        state.close();

        assert_eq!(state.stage, PermissionStage::Permission);
        assert!(!state.open);
    }

    #[test]
    fn test_esc_transitions_to_reject_stage() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));

        let result = state.handle_key(crossterm::event::KeyCode::Esc);

        assert!(result.is_none());
        assert_eq!(state.stage, PermissionStage::RejectMessage);
    }

    #[test]
    fn test_a_transitions_to_always_stage() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));

        let result = state.handle_key(crossterm::event::KeyCode::Char('a'));

        assert!(result.is_none());
        assert_eq!(state.stage, PermissionStage::AlwaysConfirm);
    }

    #[test]
    fn test_enter_on_permission_returns_approve() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));

        let result = state.handle_key(crossterm::event::KeyCode::Enter);

        assert_eq!(result, Some(PermissionAction::Approve));
        assert!(!state.open);
    }

    #[test]
    fn test_enter_on_always_returns_always_allow() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.handle_key(crossterm::event::KeyCode::Char('a')); // go to AlwaysConfirm

        let result = state.handle_key(crossterm::event::KeyCode::Enter);

        assert_eq!(result, Some(PermissionAction::AlwaysAllow));
        assert!(!state.open);
    }

    #[test]
    fn test_enter_on_reject_returns_deny() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.handle_key(crossterm::event::KeyCode::Esc); // go to RejectMessage

        let result = state.handle_key(crossterm::event::KeyCode::Enter);

        assert_eq!(result, Some(PermissionAction::Deny));
        assert!(!state.open);
    }

    #[test]
    fn test_esc_from_always_returns_to_permission() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.handle_key(crossterm::event::KeyCode::Char('a'));

        let result = state.handle_key(crossterm::event::KeyCode::Esc);

        assert!(result.is_none());
        assert_eq!(state.stage, PermissionStage::Permission);
    }

    #[test]
    fn test_esc_from_reject_returns_to_permission() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.handle_key(crossterm::event::KeyCode::Esc);

        let result = state.handle_key(crossterm::event::KeyCode::Esc);

        assert!(result.is_none());
        assert_eq!(state.stage, PermissionStage::Permission);
    }

    #[test]
    fn test_left_right_cycle_buttons() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));

        state.handle_key(crossterm::event::KeyCode::Right);
        assert_eq!(state.focused_button, 1);

        state.handle_key(crossterm::event::KeyCode::Right);
        assert_eq!(state.focused_button, 2);

        state.handle_key(crossterm::event::KeyCode::Right);
        assert_eq!(state.focused_button, 0);

        state.handle_key(crossterm::event::KeyCode::Left);
        assert_eq!(state.focused_button, 2);
    }

    // --- Reject message capture tests ---

    #[test]
    fn test_reject_message_accumulates_chars() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.handle_key(crossterm::event::KeyCode::Esc);

        state.handle_key(crossterm::event::KeyCode::Char('t'));
        state.handle_key(crossterm::event::KeyCode::Char('e'));
        state.handle_key(crossterm::event::KeyCode::Char('s'));
        state.handle_key(crossterm::event::KeyCode::Char('t'));

        assert_eq!(state.reject_message, "test");
    }

    #[test]
    fn test_backspace_deletes_last_char() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.handle_key(crossterm::event::KeyCode::Esc);

        state.handle_key(crossterm::event::KeyCode::Char('a'));
        state.handle_key(crossterm::event::KeyCode::Char('b'));
        state.handle_key(crossterm::event::KeyCode::Backspace);

        assert_eq!(state.reject_message, "a");
    }

    #[test]
    fn test_backspace_on_empty_does_not_panic() {
        let mut state = PermissionDialogState::new();
        state.open(make_request("bash", "ls"));
        state.handle_key(crossterm::event::KeyCode::Esc);

        state.handle_key(crossterm::event::KeyCode::Backspace);

        assert_eq!(state.reject_message, "");
    }

    // --- Tool-specific display tests ---

    #[test]
    fn test_bash_display_info() {
        let req = make_request("bash", "ls -la");
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '$');
        assert_eq!(info.title, "Shell command");
        assert_eq!(info.body, vec!["ls -la"]);
    }

    #[test]
    fn test_edit_file_display_info() {
        let req = make_request("edit", r#"{"path": "src/main.rs"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{2192}');
        assert_eq!(info.title, "Edit src/main.rs");
        assert_eq!(info.body, vec!["src/main.rs"]);
    }

    #[test]
    fn test_read_file_display_info() {
        let req = make_request("read_file", r#"{"path": "Cargo.toml"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{2192}');
        assert_eq!(info.title, "Read Cargo.toml");
        assert_eq!(info.body, vec!["Cargo.toml"]);
    }

    #[test]
    fn test_glob_search_display_info() {
        let req = make_request("glob_search", r#"{"pattern": "**/*.rs"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{2731}');
        assert_eq!(info.title, "Glob pattern");
        assert_eq!(info.body, vec!["**/*.rs"]);
    }

    #[test]
    fn test_grep_search_display_info() {
        let req = make_request("grep_search", r#"{"pattern": "fn main"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{2731}');
        assert_eq!(info.title, "Grep pattern");
        assert_eq!(info.body, vec!["fn main"]);
    }

    #[test]
    fn test_list_directory_display_info() {
        let req = make_request("list", r#"{"path": "src/"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{2192}');
        assert_eq!(info.title, "List directory");
        assert_eq!(info.body, vec!["src/"]);
    }

    #[test]
    fn test_agent_task_display_info() {
        let req = make_request("task", r#"{"description": "Analyze codebase"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '#');
        assert_eq!(info.title, "Agent Task");
        assert_eq!(info.body, vec!["Analyze codebase"]);
    }

    #[test]
    fn test_web_fetch_display_info() {
        let req = make_request("web_fetch", r#"{"url": "https://example.com"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '%');
        assert_eq!(info.title, "WebFetch");
        assert_eq!(info.body, vec!["https://example.com"]);
    }

    #[test]
    fn test_web_search_display_info() {
        let req = make_request("web_search", r#"{"query": "rust best practices"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{25c7}');
        assert_eq!(info.title, "Web Search");
        assert_eq!(info.body, vec!["rust best practices"]);
    }

    #[test]
    fn test_unknown_tool_display_info() {
        let req = make_request("custom_tool", "some input");
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{2699}');
        assert_eq!(info.title, "Tool: Custom_tool");
        assert_eq!(info.body, vec!["some input"]);
    }

    #[test]
    fn test_write_file_display_info() {
        let req = make_request("write_file", r#"{"path": "output.txt"}"#);
        let info = tool_display_info(&req);

        assert_eq!(info.icon, '\u{2192}');
        assert_eq!(info.title, "Edit output.txt");
        assert_eq!(info.body, vec!["output.txt"]);
    }
}
