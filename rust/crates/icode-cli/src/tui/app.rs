use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::tui::autocomplete::AutocompleteState;
use crate::tui::command_palette::CommandPaletteState;
use crate::tui::debug_panel::DebugPanelState;
use crate::tui::dialog_context_viz::ContextVizDialogState;
use crate::tui::dialog_export_options::ExportOptionsState;
use crate::tui::dialog_help::HelpDialogState;
use crate::tui::dialog_mcp::McpDialogState;
use crate::tui::dialog_message_actions::MessageActionDialogState;
use crate::tui::dialog_permission::PermissionDialogState;
use crate::tui::dialog_plugins::PluginsDialogState;
use crate::tui::dialog_prompt_stash::PromptStashState;
use crate::tui::dialog_providers::ProviderDialogState;
use crate::tui::dialog_question::QuestionPromptState;
use crate::tui::dialog_session_branching::SessionBranchingState;
use crate::tui::dialog_sessions::SessionsDialogState;
use crate::tui::dialog_skills::SkillsDialogState;
use crate::tui::dialog_theme_list::ThemeListDialogState;
use crate::tui::dialog_workspaces::WorkspaceDialogState;
use crate::tui::home_screen::HomeScreenState;
use crate::tui::input::InputState;
use crate::tui::model_picker::ModelPickerState;
use crate::tui::plugin::{PluginRoute, PluginSlot, SlotContent};
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    capabilities_for_model, DiffView, FilesPanelState, LspPanelState, McpPanelState, MessageList,
    PagerState, Sidebar, SubAgentInfo, TodoPanelState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Loading,
    Error(String),
    AuthError(String),
    Welcome,
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
    pub tool_timeline: Vec<(String, bool, u64)>,
    pub turn_duration_ms: u64,
    pub sub_agents: Vec<SubAgentInfo>,
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
    pub cache_create_tokens: u32,
    pub cache_read_tokens: u32,
    pub cumulative_cost: f64,
    pub budget_max: Option<f64>,
    pub budget_remaining: Option<f64>,
    pub compaction_count: u32,
    pub compaction_removed_messages: u32,
    pub effort_level: String,
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
    pub auto_scroll: bool,
    pub scroll_paused: bool,
    pub is_streaming: bool,
    pub connected: bool,
    pub lsp_count: usize,
    pub lsp_panel: LspPanelState,
    pub mcp_panel: McpPanelState,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub model_picker: ModelPickerState,
    pub command_palette: CommandPaletteState,
    pub autocomplete: AutocompleteState,
    pub mcp_dialog: McpDialogState,
    pub skills_dialog: SkillsDialogState,
    pub theme_list_dialog: ThemeListDialogState,
    pub plugins_dialog: PluginsDialogState,
    pub sessions_dialog: SessionsDialogState,
    pub message_action_dialog: MessageActionDialogState,
    pub help_dialog: HelpDialogState,
    pub context_viz_dialog: ContextVizDialogState,
    pub branching_dialog: SessionBranchingState,
    pub prompt_stash: PromptStashState,
    pub export_options: ExportOptionsState,
    pub debug_panel: DebugPanelState,
    pub home_screen: HomeScreenState,
    pub provider_dialog: ProviderDialogState,
    pub workspace_dialog: WorkspaceDialogState,
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
    pub turn_started_at: Option<Instant>,
    pub last_turn_duration: Option<std::time::Duration>,
    pub diff_view: Option<DiffView>,
    pub pager: PagerState,
    pub files_panel: FilesPanelState,
    pub todo_panel: TodoPanelState,
    pub plugin_slots: std::collections::HashMap<PluginSlot, Vec<SlotContent>>,
    pub plugin_routes: Vec<PluginRoute>,
    pub home_placeholder_idx: usize,
    pub home_placeholder_timer: Instant,
    pub pending_file_refs: Vec<(String, String)>,
    pub pending_slash_command: Option<String>,
    pub permission_dialog: PermissionDialogState,
    pub question_prompt: QuestionPromptState,
    pub has_shown_welcome: bool,
}

#[derive(Debug, Clone)]
pub struct RevertState {
    pub message_boundary: usize,
    pub prompt_text: String,
}

pub fn icode_config_dir() -> PathBuf {
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
                if let Some(theme) = Theme::from_name(name) {
                    return theme;
                }
            }
        }
    }
    Theme::dark()
}

fn load_theme_id() -> String {
    let path = theme_config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(name) = value.get("theme").and_then(|v| v.as_str()) {
                return name.to_string();
            }
        }
    }
    "opencode".to_string()
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
    pub fn new(
        model: &str,
        permission_mode: &str,
        cwd: &str,
        skill_manager: Option<Arc<runtime::skill_manager::SkillManager>>,
    ) -> Self {
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
                cache_create_tokens: 0,
                cache_read_tokens: 0,
                cumulative_cost: 0.0,
                budget_max: None,
                budget_remaining: None,
                compaction_count: 0,
                compaction_removed_messages: 0,
                effort_level: "balanced".into(),
            },
            tools: Vec::new(),
            sidebar_visible: true,
            scroll_offset: usize::MAX,
            auto_scroll: true,
            scroll_paused: false,
            is_streaming: false,
            connected: false,
            lsp_count: 0,
            lsp_panel: LspPanelState::new(),
            mcp_panel: McpPanelState::new(),
            cwd: cwd.into(),
            git_branch: None,
            git_dirty: false,
            model_picker: ModelPickerState::new(),
            command_palette: CommandPaletteState::new(),
            autocomplete: AutocompleteState::new(),
            mcp_dialog: McpDialogState::new(),
            skills_dialog: SkillsDialogState::new(skill_manager),
            theme_list_dialog: ThemeListDialogState::new(&load_theme_id()),
            plugins_dialog: PluginsDialogState::new(),
            sessions_dialog: SessionsDialogState::new(),
            message_action_dialog: MessageActionDialogState::new(),
            help_dialog: HelpDialogState::new(),
            context_viz_dialog: ContextVizDialogState::new(),
            branching_dialog: SessionBranchingState::new(),
            prompt_stash: PromptStashState::new(),
            export_options: ExportOptionsState::new(),
            debug_panel: DebugPanelState::new(),
            home_screen: HomeScreenState::new(),
            provider_dialog: ProviderDialogState::new(),
            workspace_dialog: WorkspaceDialogState::new(),
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
            turn_started_at: None,
            last_turn_duration: None,
            diff_view: None,
            pager: PagerState::default(),
            files_panel: FilesPanelState::new(),
            todo_panel: TodoPanelState::new(),
            plugin_slots: std::collections::HashMap::new(),
            plugin_routes: Vec::new(),
            home_placeholder_idx: 0,
            home_placeholder_timer: Instant::now(),
            pending_file_refs: Vec::new(),
            pending_slash_command: None,
            permission_dialog: PermissionDialogState::new(),
            question_prompt: QuestionPromptState::new(),
            has_shown_welcome: false,
        }
    }

    pub fn set_theme(&mut self, name: &str) {
        let theme = Theme::from_name(name).unwrap_or_default();
        self.theme = theme;
        save_theme(name);
    }

    /// Cycle to the next theme in the registry.
    /// Returns the new theme's ID.
    pub fn toggle_theme(&mut self) -> &'static str {
        use crate::tui::theme_loader::list_themes;
        let all = list_themes();
        // Find current theme ID by matching background color
        let current_id = self.find_current_theme_id();
        let current_idx = all
            .iter()
            .position(|&id| id == current_id.as_str())
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % all.len();
        let next_id = all[next_idx];
        self.set_theme(next_id);
        next_id
    }

    /// Identify the current theme's ID by comparing background colors.
    fn find_current_theme_id(&self) -> String {
        use crate::tui::theme_loader::THEMES;
        THEMES
            .iter()
            .find(|entry| entry.theme.background == self.theme.background)
            .map_or_else(|| "opencode".to_string(), |entry| entry.id.to_string())
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
            tool_timeline: Vec::new(),
            turn_duration_ms: 0,
            sub_agents: Vec::new(),
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
            tool_timeline: Vec::new(),
            turn_duration_ms: 0,
            sub_agents: Vec::new(),
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

    pub fn finish_stream(&mut self) {
        if let Some(msg) = self.messages.last_mut() {
            msg.is_streaming = false;
        }
        self.is_streaming = false;
        self.is_thinking = false;
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
                if let Some(MessagePart::Thinking { content }) = msg.parts.last_mut() {
                    content.push_str(delta);
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
        self.scroll_offset = usize::MAX;
        self.auto_scroll = true;
        self.scroll_paused = false;
    }

    pub fn recalculate_scroll(&mut self) {
        if self.scroll_offset == usize::MAX {
            return;
        }
        self.scroll_offset = 0;
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
            tool_timeline: Vec::new(),
            turn_duration_ms: 0,
            sub_agents: Vec::new(),
        });
        self.scroll_to_bottom();
    }

    pub fn show_auth_error(&mut self, msg: String) {
        self.mode = AppMode::AuthError(msg.clone());
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
            tool_timeline: Vec::new(),
            turn_duration_ms: 0,
            sub_agents: Vec::new(),
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
            .map_or(self.messages.len(), |r| r.message_boundary);
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

    pub fn cleanup_reverted(&mut self) {
        if let Some(ref revert) = self.revert {
            self.messages.truncate(revert.message_boundary);
        }
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

    pub fn set_completions(&mut self, _completions: Vec<String>) {}

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

    pub fn clear_tool_calls(&mut self) {
        for msg in &mut self.messages {
            msg.parts
                .retain(|p| matches!(p, MessagePart::Text { .. } | MessagePart::Thinking { .. }));
        }
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
        self.messages.get(msg_idx).map_or(0, |m| {
            m.parts
                .iter()
                .filter(|p| matches!(p, MessagePart::ToolCall { .. }))
                .count()
        })
    }

    pub fn add_sub_agent(&mut self, agent: SubAgentInfo) {
        for msg in self.messages.iter_mut().rev() {
            if matches!(msg.role, MessageRole::Assistant) && msg.is_streaming {
                msg.sub_agents.push(agent);
                return;
            }
        }
    }

    pub fn update_sub_agent_status(
        &mut self,
        name: &str,
        status: crate::tui::widgets::SubAgentStatus,
    ) {
        for msg in self.messages.iter_mut().rev() {
            if let Some(sa) = msg.sub_agents.iter_mut().find(|s| s.name == name) {
                sa.status = status;
                return;
            }
        }
    }

    pub fn toggle_sub_agent_expand(&mut self, msg_idx: usize, sa_idx: usize) {
        if let Some(msg) = self.messages.get_mut(msg_idx) {
            if let Some(sa) = msg.sub_agents.get_mut(sa_idx) {
                sa.expanded = !sa.expanded;
            }
        }
    }

    pub fn turn_elapsed(&self) -> Option<String> {
        self.turn_started_at.map(|started| {
            let elapsed = started.elapsed();
            let total_secs = elapsed.as_secs();
            if total_secs >= 3600 {
                let hours = total_secs / 3600;
                let mins = (total_secs % 3600) / 60;
                format!("{hours}h {mins}m")
            } else if total_secs >= 60 {
                let mins = total_secs / 60;
                let secs = total_secs % 60;
                format!("{mins}m {secs}s")
            } else {
                format!("{total_secs}s")
            }
        })
    }

    pub fn register_slot_content(
        &mut self,
        plugin_id: impl Into<String>,
        slot: PluginSlot,
        lines: Vec<String>,
        style: crate::tui::plugin::SlotStyle,
    ) {
        let plugin_id = plugin_id.into();
        let entries = self.plugin_slots.entry(slot).or_default();
        entries.retain(|e| e.plugin_id != plugin_id);
        entries.push(SlotContent {
            plugin_id,
            lines,
            style,
        });
    }

    pub fn remove_plugin_slot_content(&mut self, plugin_id: &str) {
        for entries in self.plugin_slots.values_mut() {
            entries.retain(|e| e.plugin_id != plugin_id);
        }
        self.plugin_slots.retain(|_, v| !v.is_empty());
    }

    pub fn get_slot_content(&self, slot: PluginSlot) -> Vec<&SlotContent> {
        self.plugin_slots
            .get(&slot)
            .map_or(Vec::new(), |v| v.iter().collect())
    }

    pub fn register_plugin_route(&mut self, route: PluginRoute) {
        self.plugin_routes.push(route);
    }

    pub fn remove_plugin_routes_by_plugin(&mut self, plugin_id: &str) {
        self.plugin_routes
            .retain(|r| !r.id.starts_with(&format!("{plugin_id}:")));
    }
}
