use crate::tui::app::{AppState, MessagePart};
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::collections::BTreeMap;

/// Status of a file tracked in the session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileStatus {
    Modified,
    Created,
    Deleted,
}

/// A single file entry with its path and status.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub status: FileStatus,
}

/// State for the files panel in the sidebar.
#[derive(Debug, Clone)]
pub struct FilesPanelState {
    pub files: Vec<FileEntry>,
    pub expanded: bool,
}

impl FilesPanelState {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            expanded: false,
        }
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn update_from_session(&mut self, state: &AppState) {
        let mut file_map: BTreeMap<String, FileStatus> = BTreeMap::new();

        for msg in &state.messages {
            for part in &msg.parts {
                if let MessagePart::ToolCall {
                    name,
                    input_summary,
                    ..
                } = part
                {
                    if let Some((path, status)) = extract_file_from_tool(name, input_summary) {
                        let should_update = match file_map.get(&path) {
                            None => true,
                            Some(FileStatus::Deleted) => false,
                            Some(FileStatus::Created) => matches!(status, FileStatus::Deleted),
                            Some(FileStatus::Modified) => true,
                        };
                        if should_update {
                            file_map.insert(path, status);
                        }
                    }
                }
            }
        }

        self.files = file_map
            .into_iter()
            .map(|(path, status)| FileEntry { path, status })
            .collect();
    }
}

/// Extract file path and status from a tool call.
/// Returns (path, `FileStatus`) if the tool is a file-modifying tool.
fn extract_file_from_tool(tool_name: &str, input_summary: &str) -> Option<(String, FileStatus)> {
    let status = match tool_name {
        "write_file" | "edit_file" | "write" | "edit" | "Write" | "Edit" => FileStatus::Modified,
        "create_file" | "new_file" | "CreateFile" | "NewFile" => FileStatus::Created,
        "delete_file" | "remove_file" | "DeleteFile" | "RemoveFile" => FileStatus::Deleted,
        _ => return None,
    };

    let path = extract_path_from_summary(input_summary)?;
    Some((path, status))
}

fn extract_path_from_summary(input_summary: &str) -> Option<String> {
    let trimmed = input_summary.trim();

    if let Some(rest) = trimmed.strip_prefix("path: ") {
        let p = rest.trim();
        if !p.is_empty() {
            return Some(p.to_string());
        }
    }

    if let Some(rest) = trimmed.strip_prefix("file: ") {
        let p = rest.trim();
        if !p.is_empty() {
            return Some(p.to_string());
        }
    }

    for prefix in &["writing ", "editing ", "deleting ", "creating "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let p = rest.trim();
            if !p.is_empty() {
                return Some(p.to_string());
            }
        }
    }

    if trimmed.contains('/') || trimmed.starts_with('.') {
        return Some(trimmed.to_string());
    }

    None
}

/// Extract the basename from a file path.
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

const MAX_VISIBLE_FILES: usize = 8;

/// Render the files panel in the sidebar.
pub fn render_files_panel(frame: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let files_panel = &state.files_panel;
    let total = files_panel.files.len();

    if total == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    let header_icon = if files_panel.expanded { "▼" } else { "▶" };
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {header_icon} Files "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("({total})"), Style::default().fg(theme.text_muted)),
    ]));

    if files_panel.expanded {
        let visible = if total > MAX_VISIBLE_FILES {
            &files_panel.files[..MAX_VISIBLE_FILES]
        } else {
            &files_panel.files[..]
        };

        for entry in visible {
            let (icon, color) = match entry.status {
                FileStatus::Modified => ("M", theme.diff_changed),
                FileStatus::Created => ("A", theme.diff_added),
                FileStatus::Deleted => ("D", theme.diff_removed),
            };

            let display_name = if entry.path.len() > 30 {
                basename(&entry.path).to_string()
            } else {
                entry.path.clone()
            };

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("[{icon}]"),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(display_name, Style::default().fg(theme.text)),
            ]));
        }

        if total > MAX_VISIBLE_FILES {
            let remaining = total - MAX_VISIBLE_FILES;
            lines.push(Line::from(vec![Span::styled(
                format!("  ...{remaining} more"),
                Style::default().fg(theme.text_muted),
            )]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Message, MessageRole, SessionInfo, ToolStatus};
    use crate::tui::widgets::{LspPanelState, McpPanelState};

    fn make_test_state() -> AppState {
        AppState {
            mode: crate::tui::app::AppMode::Normal,
            theme: Theme::dark(),
            messages: Vec::new(),
            prompt: crate::tui::input::InputState::new("> "),
            session: SessionInfo {
                id: "test".into(),
                title: "Test".into(),
                model: "test".into(),
                permission_mode: "test".into(),
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
            cwd: "/test".into(),
            git_branch: None,
            git_dirty: false,
            model_picker: crate::tui::model_picker::ModelPickerState::new(),
            command_palette: crate::tui::command_palette::CommandPaletteState::new(),
            autocomplete: crate::tui::autocomplete::AutocompleteState::new(),
            mcp_dialog: crate::tui::dialog_mcp::McpDialogState::new(),
            skills_dialog: crate::tui::dialog_skills::SkillsDialogState::new(None),
            theme_list_dialog: crate::tui::dialog_theme_list::ThemeListDialogState::new("opencode"),
            plugins_dialog: crate::tui::dialog_plugins::PluginsDialogState::new(),
            sessions_dialog: crate::tui::dialog_sessions::SessionsDialogState::new(),
            message_action_dialog:
                crate::tui::dialog_message_actions::MessageActionDialogState::new(),
            help_dialog: crate::tui::dialog_help::HelpDialogState::new(),
            context_viz_dialog: crate::tui::dialog_context_viz::ContextVizDialogState::new(),
            branching_dialog: crate::tui::dialog_session_branching::SessionBranchingState::new(),
            prompt_stash: crate::tui::dialog_prompt_stash::PromptStashState::new(),
            export_options: crate::tui::dialog_export_options::ExportOptionsState::new(),
            debug_panel: crate::tui::debug_panel::DebugPanelState::new(),
            home_screen: crate::tui::home_screen::HomeScreenState::new(),
            provider_dialog: crate::tui::dialog_providers::ProviderDialogState::new(),
            workspace_dialog: crate::tui::dialog_workspaces::WorkspaceDialogState::new(),
            skill_count: 0,
            plugin_count: 0,
            plugin_slots: std::collections::HashMap::new(),
            plugin_routes: Vec::new(),
            revert: None,
            leader_active: false,
            leader_activated_at: None,
            context_window: 0,
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
            pager: crate::tui::widgets::PagerState::default(),
            files_panel: FilesPanelState::new(),
            todo_panel: crate::tui::widgets::TodoPanelState::new(),
            home_placeholder_idx: 0,
            home_placeholder_timer: std::time::Instant::now(),
            pending_file_refs: Vec::new(),
            pending_slash_command: None,
            permission_dialog: crate::tui::dialog_permission::PermissionDialogState::new(),
            question_prompt: crate::tui::dialog_question::QuestionPromptState::new(),
            has_shown_welcome: false,
        }
    }

    fn add_tool_call(state: &mut AppState, name: &str, input_summary: &str) {
        state.messages.push(Message {
            role: MessageRole::Assistant,
            parts: vec![MessagePart::ToolCall {
                id: format!("tc-{name}"),
                name: name.into(),
                status: ToolStatus::Completed,
                input_summary: input_summary.into(),
                output: Some("ok".into()),
                expanded: false,
            }],
            agent: "build".into(),
            timestamp: 1000,
            is_streaming: false,
            tool_timeline: Vec::new(),
            turn_duration_ms: 0,
            sub_agents: Vec::new(),
        });
    }

    #[test]
    fn test_extract_path_path_prefix() {
        assert_eq!(
            extract_path_from_summary("path: src/main.rs"),
            Some("src/main.rs".into())
        );
    }

    #[test]
    fn test_extract_path_file_prefix() {
        assert_eq!(
            extract_path_from_summary("file: README.md"),
            Some("README.md".into())
        );
    }

    #[test]
    fn test_extract_path_writing_prefix() {
        assert_eq!(
            extract_path_from_summary("writing src/lib.rs"),
            Some("src/lib.rs".into())
        );
    }

    #[test]
    fn test_extract_path_editing_prefix() {
        assert_eq!(
            extract_path_from_summary("editing Cargo.toml"),
            Some("Cargo.toml".into())
        );
    }

    #[test]
    fn test_extract_path_bare_path() {
        assert_eq!(
            extract_path_from_summary("/home/user/file.txt"),
            Some("/home/user/file.txt".into())
        );
    }

    #[test]
    fn test_extract_path_relative() {
        assert_eq!(
            extract_path_from_summary("./src/test.rs"),
            Some("./src/test.rs".into())
        );
    }

    #[test]
    fn test_extract_path_empty() {
        assert_eq!(extract_path_from_summary(""), None);
    }

    #[test]
    fn test_extract_path_no_match() {
        assert_eq!(extract_path_from_summary("hello world"), None);
    }

    #[test]
    fn test_extract_file_from_tool_write() {
        let result = extract_file_from_tool("write_file", "path: src/main.rs");
        assert!(result.is_some());
        let (path, status) = result.unwrap();
        assert_eq!(path, "src/main.rs");
        assert_eq!(status, FileStatus::Modified);
    }

    #[test]
    fn test_extract_file_from_tool_edit() {
        let result = extract_file_from_tool("edit_file", "editing Cargo.toml");
        assert!(result.is_some());
        let (path, status) = result.unwrap();
        assert_eq!(path, "Cargo.toml");
        assert_eq!(status, FileStatus::Modified);
    }

    #[test]
    fn test_extract_file_from_tool_create() {
        let result = extract_file_from_tool("create_file", "path: new_file.txt");
        assert!(result.is_some());
        let (path, status) = result.unwrap();
        assert_eq!(path, "new_file.txt");
        assert_eq!(status, FileStatus::Created);
    }

    #[test]
    fn test_extract_file_from_tool_delete() {
        let result = extract_file_from_tool("delete_file", "path: old_file.txt");
        assert!(result.is_some());
        let (path, status) = result.unwrap();
        assert_eq!(path, "old_file.txt");
        assert_eq!(status, FileStatus::Deleted);
    }

    #[test]
    fn test_extract_file_from_tool_non_file_tool() {
        let result = extract_file_from_tool("bash", "ls -la");
        assert!(result.is_none());
    }

    #[test]
    fn test_basename_extraction() {
        assert_eq!(basename("src/components/button.rs"), "button.rs");
        assert_eq!(basename("Cargo.toml"), "Cargo.toml");
        assert_eq!(basename("/a/b/c/d.rs"), "d.rs");
    }

    #[test]
    fn test_update_from_session_single_file() {
        let mut state = make_test_state();
        add_tool_call(&mut state, "write_file", "path: src/main.rs");

        let mut panel = FilesPanelState::new();
        panel.update_from_session(&state);

        assert_eq!(panel.files.len(), 1);
        assert_eq!(panel.files[0].path, "src/main.rs");
        assert_eq!(panel.files[0].status, FileStatus::Modified);
    }

    #[test]
    fn test_update_from_session_multiple_files() {
        let mut state = make_test_state();
        add_tool_call(&mut state, "write_file", "path: src/main.rs");
        add_tool_call(&mut state, "edit_file", "path: src/lib.rs");
        add_tool_call(&mut state, "create_file", "path: docs/README.md");

        let mut panel = FilesPanelState::new();
        panel.update_from_session(&state);

        assert_eq!(panel.files.len(), 3);
    }

    #[test]
    fn test_update_from_session_dedup_same_file() {
        let mut state = make_test_state();
        add_tool_call(&mut state, "write_file", "path: src/main.rs");
        add_tool_call(&mut state, "edit_file", "path: src/main.rs");

        let mut panel = FilesPanelState::new();
        panel.update_from_session(&state);

        assert_eq!(panel.files.len(), 1);
        assert_eq!(panel.files[0].path, "src/main.rs");
        assert_eq!(panel.files[0].status, FileStatus::Modified);
    }

    #[test]
    fn test_update_from_session_deleted_overwrites() {
        let mut state = make_test_state();
        add_tool_call(&mut state, "write_file", "path: src/main.rs");
        add_tool_call(&mut state, "delete_file", "path: src/main.rs");

        let mut panel = FilesPanelState::new();
        panel.update_from_session(&state);

        assert_eq!(panel.files.len(), 1);
        assert_eq!(panel.files[0].status, FileStatus::Deleted);
    }

    #[test]
    fn test_toggle_expanded() {
        let mut panel = FilesPanelState::new();
        assert!(!panel.expanded);

        panel.toggle();
        assert!(panel.expanded);

        panel.toggle();
        assert!(!panel.expanded);
    }

    #[test]
    fn test_file_status_derives() {
        let s1 = FileStatus::Modified;
        let s2 = FileStatus::Modified.clone();
        assert_eq!(s1, s2);

        let debug_str = format!("{s1:?}");
        assert!(debug_str.contains("Modified"));
    }

    #[test]
    fn test_file_entry_clone() {
        let entry = FileEntry {
            path: "test.rs".into(),
            status: FileStatus::Created,
        };
        let cloned = entry.clone();
        assert_eq!(entry.path, cloned.path);
        assert_eq!(entry.status, cloned.status);
    }
}
