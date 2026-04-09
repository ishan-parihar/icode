use crate::tui::app::{AppState, MessagePart};
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Status of a todo item tracked in the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    Completed,
}

/// A single todo entry with its text and status.
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub text: String,
    pub status: TodoStatus,
}

/// State for the todo panel in the sidebar.
#[derive(Debug, Clone)]
pub struct TodoPanelState {
    pub todos: Vec<TodoItem>,
    pub expanded: bool,
}

impl TodoPanelState {
    pub fn new() -> Self {
        Self {
            todos: Vec::new(),
            expanded: false,
        }
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn update_from_session(&mut self, state: &AppState) {
        self.update_from_session_with_messages(&state.messages);
    }

    pub fn update_from_session_with_messages(&mut self, messages: &[crate::tui::app::Message]) {
        let mut todos: Vec<TodoItem> = Vec::new();

        for msg in messages {
            for part in &msg.parts {
                if let MessagePart::ToolCall { name, output, .. } = part {
                    if name == "todowrite" || name == "TodoWrite" {
                        if let Some(output_text) = output {
                            if let Some(extracted) = extract_todos_from_json(output_text) {
                                todos = extracted;
                            }
                        }
                    }
                }
            }
        }

        if todos.is_empty() {
            for msg in messages {
                for part in &msg.parts {
                    if let MessagePart::ToolCall {
                        name,
                        input_summary,
                        ..
                    } = part
                    {
                        if name == "todowrite" || name == "TodoWrite" {
                            if let Some(extracted) = extract_todos_from_summary(input_summary) {
                                todos = extracted;
                            }
                        }
                    }
                }
            }
        }

        self.todos = todos;
    }
}

/// Parse todo items from JSON output of the todowrite tool.
/// Expected format: {"todos": [{"content": "...", "status": "pending|completed"}]}
fn extract_todos_from_json(output: &str) -> Option<Vec<TodoItem>> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    let todos_array = value.get("todos")?.as_array()?;

    let mut items = Vec::new();
    for item in todos_array {
        let content = item.get("content")?.as_str()?;
        let status_str = item
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("pending");
        let status = if status_str == "completed" {
            TodoStatus::Completed
        } else {
            TodoStatus::Pending
        };
        items.push(TodoItem {
            text: content.to_string(),
            status,
        });
    }

    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

/// Fall back: parse pending/completed counts from `input_summary`.
/// Format: "todos: 3 pending, 2 completed"
fn extract_todos_from_summary(input_summary: &str) -> Option<Vec<TodoItem>> {
    let trimmed = input_summary.trim();

    // Try to extract counts like "3 pending, 2 completed"
    let pending_count = extract_count(trimmed, "pending")?;
    let completed_count = extract_count(trimmed, "completed").unwrap_or(0);

    if pending_count == 0 && completed_count == 0 {
        return None;
    }

    let mut items = Vec::new();
    for _ in 0..completed_count {
        items.push(TodoItem {
            text: "(completed)".to_string(),
            status: TodoStatus::Completed,
        });
    }
    for _ in 0..pending_count {
        items.push(TodoItem {
            text: "(pending)".to_string(),
            status: TodoStatus::Pending,
        });
    }

    Some(items)
}

/// Extract a count for a given status keyword from the summary.
fn extract_count(summary: &str, keyword: &str) -> Option<usize> {
    let words: Vec<&str> = summary.split_whitespace().collect();
    for i in 0..words.len() {
        let clean = words[i].trim_matches(',');
        if let Ok(n) = clean.parse::<usize>() {
            if let Some(next) = words.get(i + 1) {
                if next.trim_matches(',').eq_ignore_ascii_case(keyword) {
                    return Some(n);
                }
            }
        }
    }
    None
}

const MAX_VISIBLE_TODOS: usize = 8;

/// Render the todo panel in the sidebar.
pub fn render_todo_panel(frame: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let todo_panel = &state.todo_panel;
    let total = todo_panel.todos.len();

    if total == 0 {
        return;
    }

    let pending = todo_panel
        .todos
        .iter()
        .filter(|t| matches!(t.status, TodoStatus::Pending))
        .count();
    let completed = total - pending;

    let mut lines: Vec<Line> = Vec::new();

    let header_icon = if todo_panel.expanded {
        "\u{25bc}"
    } else {
        "\u{25b6}"
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {header_icon} Todos "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({pending}/{completed})"),
            Style::default().fg(theme.text_muted),
        ),
    ]));

    if todo_panel.expanded {
        let visible = if total > MAX_VISIBLE_TODOS {
            &todo_panel.todos[..MAX_VISIBLE_TODOS]
        } else {
            &todo_panel.todos[..]
        };

        for item in visible {
            let (icon, color) = match item.status {
                TodoStatus::Completed => ("\u{2713}", theme.success),
                TodoStatus::Pending => (" ", theme.text_muted),
            };

            let display_text = if item.text.chars().count() > 35 {
                format!("{}...", item.text.chars().take(32).collect::<String>())
            } else {
                item.text.clone()
            };

            let style = if matches!(item.status, TodoStatus::Completed) {
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::DIM)
            } else {
                Style::default().fg(theme.text)
            };

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("[{icon}]"),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(display_text, style),
            ]));
        }

        if total > MAX_VISIBLE_TODOS {
            let remaining = total - MAX_VISIBLE_TODOS;
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
    use crate::tui::input::InputState;
    use crate::tui::widgets::{LspPanelState, McpPanelState};

    fn make_test_state() -> AppState {
        AppState {
            mode: crate::tui::app::AppMode::Normal,
            theme: Theme::dark(),
            messages: Vec::new(),
            prompt: InputState::new("> "),
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
            files_panel: crate::tui::widgets::FilesPanelState::new(),
            todo_panel: TodoPanelState::new(),
            home_placeholder_idx: 0,
            home_placeholder_timer: std::time::Instant::now(),
            pending_file_refs: Vec::new(),
            pending_slash_command: None,
            permission_dialog: crate::tui::dialog_permission::PermissionDialogState::new(),
            question_prompt: crate::tui::dialog_question::QuestionPromptState::new(),
        }
    }

    fn add_todowrite_tool_call(state: &mut AppState, output: &str) {
        state.messages.push(Message {
            role: MessageRole::Assistant,
            parts: vec![MessagePart::ToolCall {
                id: "tc-todowrite".into(),
                name: "todowrite".into(),
                status: ToolStatus::Completed,
                input_summary: "todos: 2 pending, 1 completed".into(),
                output: Some(output.into()),
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

    fn add_todowrite_no_output(state: &mut AppState, input_summary: &str) {
        state.messages.push(Message {
            role: MessageRole::Assistant,
            parts: vec![MessagePart::ToolCall {
                id: "tc-todowrite".into(),
                name: "todowrite".into(),
                status: ToolStatus::Completed,
                input_summary: input_summary.into(),
                output: None,
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
    fn test_extract_todos_from_json_basic() {
        let json = r#"{"todos": [{"content": "Build sidebar", "status": "pending"}, {"content": "Write tests", "status": "completed"}]}"#;
        let result = extract_todos_from_json(json);
        assert!(result.is_some());
        let todos = result.unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].text, "Build sidebar");
        assert_eq!(todos[0].status, TodoStatus::Pending);
        assert_eq!(todos[1].text, "Write tests");
        assert_eq!(todos[1].status, TodoStatus::Completed);
    }

    #[test]
    fn test_extract_todos_from_json_all_pending() {
        let json = r#"{"todos": [{"content": "Task 1", "status": "pending"}, {"content": "Task 2", "status": "pending"}]}"#;
        let result = extract_todos_from_json(json);
        assert!(result.is_some());
        let todos = result.unwrap();
        assert_eq!(todos.len(), 2);
        assert!(todos
            .iter()
            .all(|t| matches!(t.status, TodoStatus::Pending)));
    }

    #[test]
    fn test_extract_todos_from_json_invalid() {
        let result = extract_todos_from_json("not json");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_todos_from_json_empty_array() {
        let json = r#"{"todos": []}"#;
        let result = extract_todos_from_json(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_todos_from_json_missing_todos_key() {
        let json = r#"{"items": []}"#;
        let result = extract_todos_from_json(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_todos_from_summary_with_counts() {
        let result = extract_todos_from_summary("todos: 3 pending, 2 completed");
        assert!(result.is_some());
        let todos = result.unwrap();
        assert_eq!(todos.len(), 5);
        let pending = todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending))
            .count();
        let completed = todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Completed))
            .count();
        assert_eq!(pending, 3);
        assert_eq!(completed, 2);
    }

    #[test]
    fn test_extract_todos_from_summary_only_pending() {
        let result = extract_todos_from_summary("todos: 2 pending");
        assert!(result.is_some());
        let todos = result.unwrap();
        assert_eq!(todos.len(), 2);
        assert!(todos
            .iter()
            .all(|t| matches!(t.status, TodoStatus::Pending)));
    }

    #[test]
    fn test_extract_todos_from_summary_no_counts() {
        let result = extract_todos_from_summary("updated todos");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_count_valid() {
        assert_eq!(
            extract_count("todos: 3 pending, 2 completed", "pending"),
            Some(3)
        );
        assert_eq!(
            extract_count("todos: 3 pending, 2 completed", "completed"),
            Some(2)
        );
    }

    #[test]
    fn test_extract_count_not_found() {
        assert_eq!(extract_count("todos: 3 pending", "completed"), None);
    }

    #[test]
    fn test_update_from_session_json_output() {
        let mut state = make_test_state();
        add_todowrite_tool_call(
            &mut state,
            r#"{"todos": [{"content": "Implement feature", "status": "pending"}, {"content": "Setup CI", "status": "completed"}]}"#,
        );

        let mut panel = TodoPanelState::new();
        panel.update_from_session(&state);

        assert_eq!(panel.todos.len(), 2);
        assert_eq!(panel.todos[0].text, "Implement feature");
        assert_eq!(panel.todos[0].status, TodoStatus::Pending);
        assert_eq!(panel.todos[1].text, "Setup CI");
        assert_eq!(panel.todos[1].status, TodoStatus::Completed);
    }

    #[test]
    fn test_update_from_session_fallback_to_summary() {
        let mut state = make_test_state();
        add_todowrite_no_output(&mut state, "todos: 2 pending, 1 completed");

        let mut panel = TodoPanelState::new();
        panel.update_from_session(&state);

        assert_eq!(panel.todos.len(), 3);
        let pending = panel
            .todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending))
            .count();
        let completed = panel
            .todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Completed))
            .count();
        assert_eq!(pending, 2);
        assert_eq!(completed, 1);
    }

    #[test]
    fn test_update_from_session_no_todowrite_calls() {
        let mut state = make_test_state();
        state.messages.push(Message {
            role: MessageRole::Assistant,
            parts: vec![MessagePart::Text {
                content: "Hello".into(),
            }],
            agent: "build".into(),
            timestamp: 1000,
            is_streaming: false,
            tool_timeline: Vec::new(),
            turn_duration_ms: 0,
            sub_agents: Vec::new(),
        });

        let mut panel = TodoPanelState::new();
        panel.update_from_session(&state);

        assert!(panel.todos.is_empty());
    }

    #[test]
    fn test_update_from_session_latest_wins() {
        let mut state = make_test_state();
        // First todowrite call
        add_todowrite_tool_call(
            &mut state,
            r#"{"todos": [{"content": "Old task", "status": "pending"}]}"#,
        );
        // Second todowrite call (should override)
        add_todowrite_tool_call(
            &mut state,
            r#"{"todos": [{"content": "New task", "status": "completed"}]}"#,
        );

        let mut panel = TodoPanelState::new();
        panel.update_from_session(&state);

        assert_eq!(panel.todos.len(), 1);
        assert_eq!(panel.todos[0].text, "New task");
        assert_eq!(panel.todos[0].status, TodoStatus::Completed);
    }

    #[test]
    fn test_toggle_expanded() {
        let mut panel = TodoPanelState::new();
        assert!(!panel.expanded);

        panel.toggle();
        assert!(panel.expanded);

        panel.toggle();
        assert!(!panel.expanded);
    }

    #[test]
    fn test_todo_status_derives() {
        let s1 = TodoStatus::Pending;
        let s2 = TodoStatus::Pending.clone();
        assert_eq!(s1, s2);

        let debug_str = format!("{s1:?}");
        assert!(debug_str.contains("Pending"));
    }

    #[test]
    fn test_todo_item_clone() {
        let item = TodoItem {
            text: "Test task".into(),
            status: TodoStatus::Pending,
        };
        let cloned = item.clone();
        assert_eq!(item.text, cloned.text);
        assert_eq!(item.status, cloned.status);
    }

    #[test]
    fn test_todo_status_default_in_json() {
        // When status field is missing, should default to pending
        let json = r#"{"todos": [{"content": "No status task"}]}"#;
        let result = extract_todos_from_json(json);
        assert!(result.is_some());
        let todos = result.unwrap();
        assert_eq!(todos[0].status, TodoStatus::Pending);
    }
}
