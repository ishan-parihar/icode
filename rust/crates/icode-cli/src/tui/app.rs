use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing;

use crate::tui::command_palette::CommandPaletteState;
use crate::tui::dialog_help::HelpDialogState;
use crate::tui::dialog_mcp::McpDialogState;
use crate::tui::dialog_message_actions::MessageActionDialogState;
use crate::tui::dialog_plugins::PluginsDialogState;
use crate::tui::dialog_sessions::SessionsDialogState;
use crate::tui::dialog_skills::SkillsDialogState;
use crate::tui::input::InputState;
use crate::tui::model_picker::ModelPickerState;
use crate::tui::theme::Theme;
use crate::tui::widgets::{capabilities_for_model, MessageList, Sidebar};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Loading,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum MessagePart {
    Text {
        content: String,
    },
    Thinking {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        status: ToolStatus,
        input_summary: String,
        output: Option<String>,
        expanded: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub parts: Vec<MessagePart>,
    pub agent: String,
    pub timestamp: u64,
    pub is_streaming: bool,
}

impl Message {
    /// Concatenate all Text parts into a single string.
    /// Used for user message content (undo/redo, copy, dialog display).
    pub fn full_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                MessagePart::Text { content } => Some(content.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Concatenate all Thinking parts into a single string.
    pub fn thinking_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                MessagePart::Thinking { content } => Some(content.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool { name: String },
}

#[derive(Debug, Clone)]
pub struct ToolEvent {
    pub name: String,
    pub status: ToolStatus,
    pub input_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub title: String,
    pub model: String,
    pub permission_mode: String,
    pub message_count: usize,
    pub turns: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created_at: Instant,
}

pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

pub struct TextSelection {
    pub start_row: u16,
    pub start_col: u16,
    pub end_row: u16,
    pub end_col: u16,
    pub content_lines: Vec<String>,
}

pub struct AppState {
    pub mode: AppMode,
    pub theme: Theme,
    pub messages: Vec<Message>,
    pub prompt: InputState,
    pub session: SessionInfo,
    pub tools: Vec<ToolEvent>,
    pub sidebar_visible: bool,
    pub scroll_offset: usize,
    pub is_streaming: bool,
    pub connected: bool,
    pub lsp_count: usize,
    pub mcp_count: usize,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub model_picker: ModelPickerState,
    pub command_palette: CommandPaletteState,
    pub mcp_dialog: McpDialogState,
    pub skills_dialog: SkillsDialogState,
    pub plugins_dialog: PluginsDialogState,
    pub sessions_dialog: SessionsDialogState,
    pub message_action_dialog: MessageActionDialogState,
    pub help_dialog: HelpDialogState,
    pub skill_count: usize,
    pub plugin_count: usize,
    pub revert: Option<RevertState>,
    pub leader_active: bool,
    pub leader_activated_at: Option<Instant>,
    pub context_window: u32,
    pub turn_in_progress: bool,
    pub toasts: Vec<Toast>,
    pub selection: Option<TextSelection>,
    pub show_thinking: bool,
    pub is_thinking: bool,
    pub interrupt_count: u8,
    pub interrupt_timestamp: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct RevertState {
    pub message_boundary: usize,
    pub prompt_text: String,
}

fn icode_config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".icode")
}

fn theme_config_path() -> PathBuf {
    icode_config_dir().join("theme.json")
}

fn load_theme() -> Theme {
    let path = theme_config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(name) = value.get("theme").and_then(|v| v.as_str()) {
                return match name {
                    "light" => Theme::light(),
                    _ => Theme::dark(),
                };
            }
        }
    }
    Theme::dark()
}

fn save_theme(name: &str) {
    let path = theme_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::json!({ "theme": name });
    let _ = fs::write(&path, content.to_string());
}

impl AppState {
    pub fn new(model: &str, permission_mode: &str, cwd: &str) -> Self {
        let caps = capabilities_for_model(model);
        let theme = load_theme();
        Self {
            mode: AppMode::Normal,
            theme,
            messages: Vec::new(),
            prompt: InputState::new("\u{203a} "),
            session: SessionInfo {
                id: String::new(),
                title: "New Session".into(),
                model: model.into(),
                permission_mode: permission_mode.into(),
                message_count: 0,
                turns: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            tools: Vec::new(),
            sidebar_visible: true,
            scroll_offset: usize::MAX,
            is_streaming: false,
            connected: false,
            lsp_count: 0,
            mcp_count: 0,
            cwd: cwd.into(),
            git_branch: None,
            git_dirty: false,
            model_picker: ModelPickerState::new(),
            command_palette: CommandPaletteState::new(),
            mcp_dialog: McpDialogState::new(),
            skills_dialog: SkillsDialogState::new(),
            plugins_dialog: PluginsDialogState::new(),
            sessions_dialog: SessionsDialogState::new(),
            message_action_dialog: MessageActionDialogState::new(),
            help_dialog: HelpDialogState::new(),
            skill_count: 0,
            plugin_count: 0,
            revert: None,
            leader_active: false,
            leader_activated_at: None,
            context_window: caps.context_window,
            turn_in_progress: false,
            toasts: Vec::new(),
            selection: None,
            show_thinking: true,
            is_thinking: false,
            interrupt_count: 0,
            interrupt_timestamp: None,
        }
    }

    pub fn set_theme(&mut self, name: &str) {
        self.theme = match name {
            "light" => Theme::light(),
            _ => Theme::dark(),
        };
        save_theme(name);
    }

    pub fn toggle_theme(&mut self) -> &'static str {
        let is_dark = self.theme.background == ratatui::style::Color::Rgb(10, 10, 10);
        let new_name = if is_dark { "light" } else { "dark" };
        self.set_theme(new_name);
        new_name
    }

    pub fn add_user_message(&mut self, content: String) {
        if let Some(last) = self.messages.last() {
            if matches!(last.role, MessageRole::User) && last.full_text() == content {
                return;
            }
        }
        self.messages.push(Message {
            role: MessageRole::User,
            parts: vec![MessagePart::Text { content }],
            agent: "build".into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            is_streaming: false,
        });
        self.scroll_to_bottom();
    }

    pub fn start_assistant_stream(&mut self, agent: &str) {
        if self.messages.iter().any(|m| m.is_streaming) {
            return;
        }
        self.is_thinking = false;
        self.messages.push(Message {
            role: MessageRole::Assistant,
            parts: Vec::new(),
            agent: agent.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            is_streaming: true,
        });
        self.is_streaming = true;
        self.scroll_to_bottom();
    }

    pub fn append_to_stream(&mut self, delta: &str) {
        if let Some(msg) = self.messages.last_mut() {
            if msg.is_streaming {
                match msg.parts.last_mut() {
                    Some(MessagePart::Text { content }) => content.push_str(delta),
                    _ => msg.parts.push(MessagePart::Text {
                        content: delta.into(),
                    }),
                }
            }
        }
    }

    pub fn finish_stream(&mut self, max_scroll: usize) {
        if let Some(msg) = self.messages.last_mut() {
            msg.is_streaming = false;
        }
        self.is_streaming = false;
        self.is_thinking = false;
        self.recalculate_scroll(max_scroll);
    }

    pub fn start_thinking(&mut self) {
        if let Some(msg) = self.messages.last_mut() {
            if msg.is_streaming {
                self.is_thinking = true;
                msg.parts.push(MessagePart::Thinking {
                    content: String::new(),
                });
            }
        }
    }

    pub fn append_thinking(&mut self, delta: &str) {
        if let Some(msg) = self.messages.last_mut() {
            if msg.is_streaming {
                match msg.parts.last_mut() {
                    Some(MessagePart::Thinking { content }) => content.push_str(delta),
                    _ => {}
                }
            }
        }
    }

    pub fn end_thinking(&mut self) {
        self.is_thinking = false;
    }

    pub fn add_tool_event(&mut self, name: &str, input_summary: &str) {
        self.tools.push(ToolEvent {
            name: name.into(),
            status: ToolStatus::Running,
            input_summary: input_summary.into(),
        });
        if let Some(msg) = self.messages.last_mut() {
            if matches!(msg.role, MessageRole::Assistant) {
                let id = format!("tc-{}-{}", msg.timestamp, name);
                msg.parts.push(MessagePart::ToolCall {
                    id,
                    name: name.into(),
                    status: ToolStatus::Running,
                    input_summary: input_summary.into(),
                    output: None,
                    expanded: false,
                });
            }
        }
    }

    pub fn complete_tool_event(&mut self, name: &str, output: &str, success: bool) {
        if let Some(tool) = self.tools.iter_mut().rev().find(|t| t.name == name) {
            tool.status = if success {
                ToolStatus::Completed
            } else {
                ToolStatus::Failed
            };
        }
        let new_status = if success {
            ToolStatus::Completed
        } else {
            ToolStatus::Failed
        };
        for msg in self.messages.iter_mut().rev() {
            for part in msg.parts.iter_mut().rev() {
                if let MessagePart::ToolCall {
                    name: tc_name,
                    status,
                    output: tc_output,
                    ..
                } = part
                {
                    if tc_name == name && matches!(status, ToolStatus::Running) {
                        *status = new_status;
                        *tc_output = Some(output.into());
                        return;
                    }
                }
            }
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        let old = self.scroll_offset;
        self.scroll_offset = usize::MAX;
        if old != usize::MAX {
            tracing::debug!(event = "scroll_to_bottom", old_offset = %old);
        }
    }

    pub fn recalculate_scroll(&mut self, max_scroll: usize) {
        if self.scroll_offset == usize::MAX {
            return;
        }
        let old = self.scroll_offset;
        self.scroll_offset = self.scroll_offset.min(max_scroll);
        if old != self.scroll_offset {
            tracing::debug!(event = "recalculate_scroll", old_offset = %old, new_offset = %self.scroll_offset, max_scroll = %max_scroll, clamped = true);
        }
    }

    pub fn set_completions(&mut self, completions: Vec<String>) {
        self.prompt.set_completions(completions);
    }

    /// Display an error message inline and set mode to Error.
    /// The error is also appended to messages as a visible error block.
    pub fn show_error(&mut self, msg: String) {
        self.mode = AppMode::Error(msg.clone());
        self.messages.push(Message {
            role: MessageRole::Tool {
                name: "error".into(),
            },
            parts: vec![MessagePart::Text { content: msg }],
            agent: "system".into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            is_streaming: false,
        });
        self.scroll_to_bottom();
    }

    pub fn undo_message(&mut self) -> bool {
        if self.is_streaming {
            return false;
        }
        let boundary = self
            .revert
            .as_ref()
            .map(|r| r.message_boundary)
            .unwrap_or(self.messages.len());
        if boundary == 0 {
            return false;
        }
        let last_user_idx = self.messages[..boundary]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| matches!(m.role, MessageRole::User))
            .map(|(i, _)| i);

        let Some(idx) = last_user_idx else {
            return false;
        };

        let prompt_text = self.messages[idx].full_text();
        self.revert = Some(RevertState {
            message_boundary: idx,
            prompt_text,
        });
        true
    }

    pub fn redo_message(&mut self) -> bool {
        let Some(revert) = self.revert.as_ref() else {
            return false;
        };
        let boundary = revert.message_boundary;
        let next_user = self.messages[boundary..]
            .iter()
            .position(|m| matches!(m.role, MessageRole::User));

        if let Some(offset) = next_user {
            let new_boundary = boundary + offset;
            let prompt_text = self.messages[new_boundary].full_text();
            self.revert = Some(RevertState {
                message_boundary: new_boundary,
                prompt_text,
            });
        } else {
            self.revert = None;
        }
        true
    }

    pub fn cleanup_reverted(&mut self, max_scroll: usize) {
        if let Some(ref revert) = self.revert {
            self.messages.truncate(revert.message_boundary);
        }
        self.recalculate_scroll(max_scroll);
        self.revert = None;
    }

    pub fn reverted_count(&self) -> usize {
        self.revert
            .as_ref()
            .map_or(0, |r| self.messages.iter().skip(r.message_boundary).count())
    }

    pub fn activate_leader(&mut self) {
        self.leader_active = true;
        self.leader_activated_at = Some(Instant::now());
    }

    pub fn deactivate_leader(&mut self) {
        self.leader_active = false;
        self.leader_activated_at = None;
    }

    pub fn check_leader_timeout(&mut self) {
        if self.leader_active {
            if let Some(activated) = self.leader_activated_at {
                if activated.elapsed().as_secs() >= 2 {
                    self.deactivate_leader();
                }
            }
        }
    }

    pub fn add_toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast {
            message: message.into(),
            kind,
            created_at: Instant::now(),
        });
    }

    pub fn prune_expired_toasts(&mut self) {
        self.toasts.retain(|t| t.created_at.elapsed().as_secs() < 3);
    }

    pub fn add_tool_call(&mut self, name: &str, input_summary: &str) {
        if let Some(msg) = self.messages.last_mut() {
            if matches!(msg.role, MessageRole::Assistant) {
                let id = format!("tc-{}-{}", msg.timestamp, name);
                msg.parts.push(MessagePart::ToolCall {
                    id,
                    name: name.into(),
                    status: ToolStatus::Running,
                    input_summary: input_summary.into(),
                    output: None,
                    expanded: false,
                });
            }
        }
    }

    pub fn complete_tool_call(&mut self, name: &str, output: &str, success: bool) {
        let new_status = if success {
            ToolStatus::Completed
        } else {
            ToolStatus::Failed
        };
        for msg in self.messages.iter_mut().rev() {
            for part in msg.parts.iter_mut().rev() {
                if let MessagePart::ToolCall {
                    name: tc_name,
                    status,
                    output: tc_output,
                    ..
                } = part
                {
                    if tc_name == name && matches!(status, ToolStatus::Running) {
                        *status = new_status;
                        *tc_output = Some(output.into());
                        return;
                    }
                }
            }
        }
    }

    pub fn clear_tool_calls(&mut self, max_scroll: usize) {
        let msg_count = self.messages.len();
        for msg in &mut self.messages {
            msg.parts
                .retain(|p| matches!(p, MessagePart::Text { .. } | MessagePart::Thinking { .. }));
        }
        self.recalculate_scroll(max_scroll);
        tracing::info!(event = "clear_tool_calls", message_count = msg_count);
    }

    pub fn toggle_tool_expand(&mut self, msg_idx: usize, tc_idx: usize) {
        if let Some(msg) = self.messages.get_mut(msg_idx) {
            let mut tc_seen = 0;
            for part in &mut msg.parts {
                if let MessagePart::ToolCall { expanded, .. } = part {
                    if tc_seen == tc_idx {
                        *expanded = !*expanded;
                        return;
                    }
                    tc_seen += 1;
                }
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn toggle_thinking(&mut self) -> bool {
        self.show_thinking = !self.show_thinking;
        self.show_thinking
    }

    pub fn tool_count_for_message(&self, msg_idx: usize) -> usize {
        self.messages
            .get(msg_idx)
            .map(|m| {
                m.parts
                    .iter()
                    .filter(|p| matches!(p, MessagePart::ToolCall { .. }))
                    .count()
            })
            .unwrap_or(0)
    }
}
