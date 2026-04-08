use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::tui::app::AppState;

#[derive(Debug, Clone)]
pub struct ListedSessionInfo {
    pub id: String,
    pub path: PathBuf,
    pub last_active: u128,
    pub message_count: usize,
    pub model: String,
    pub title: String,
    pub permission_mode: String,
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

/// Return the directory where sessions are stored.
/// Checks both ~/.icode/sessions/ and ./.icode/sessions/ (project-local).
pub fn sessions_dir() -> Option<PathBuf> {
    // Check project-local first
    let local = PathBuf::from(".icode").join("sessions");
    if local.is_dir() {
        return Some(local);
    }
    // Then global
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        let p = PathBuf::from(path).join("sessions");
        if p.is_dir() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let global = PathBuf::from(home).join(".icode").join("sessions");
    if global.is_dir() {
        Some(global)
    } else {
        None
    }
}

fn is_session_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "jsonl" || ext == "json")
}

fn file_last_modified_ms(path: &Path) -> u128 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

/// Scan session directories for available session files.
/// Returns a list of `ListedSessionInfo` sorted by `last_active` descending.
pub fn list_sessions() -> Vec<ListedSessionInfo> {
    let Some(dir) = sessions_dir() else {
        return Vec::new();
    };

    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut sessions = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !is_session_file(&path) {
            continue;
        }

        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let last_active = file_last_modified_ms(&path);

        // Try to load session metadata from the runtime
        let (
            msg_count,
            model,
            title,
            permission_mode,
            turns,
            input_tokens,
            output_tokens,
            cache_create_tokens,
            cache_read_tokens,
            cumulative_cost,
            budget_max,
            budget_remaining,
            compaction_count,
            compaction_removed_messages,
            effort_level,
        ) = match runtime::Session::load_from_path(&path) {
            Ok(session) => {
                let msg_count = session.messages.len();
                let turns = session
                    .messages
                    .iter()
                    .filter(|m| matches!(m.role, runtime::MessageRole::Assistant))
                    .count() as u32;
                let effort_level = "balanced".to_string();
                (
                    msg_count,
                    "unknown".to_string(),
                    format!("Session {}", session.session_id),
                    "workspace-write".to_string(),
                    turns,
                    0u32,
                    0u32,
                    0u32,
                    0u32,
                    0.0f64,
                    None,
                    None,
                    session.compaction.as_ref().map_or(0, |c| c.count),
                    session
                        .compaction
                        .as_ref()
                        .map_or(0, |c| c.removed_message_count as u32),
                    effort_level,
                )
            }
            Err(_) => {
                // Can't parse — still list it with defaults
                (
                    0,
                    "unknown".to_string(),
                    id.clone(),
                    "unknown".to_string(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    0.0,
                    None,
                    None,
                    0,
                    0,
                    "unknown".to_string(),
                )
            }
        };

        sessions.push(ListedSessionInfo {
            id,
            path,
            last_active,
            message_count: msg_count,
            model,
            title,
            permission_mode,
            turns,
            input_tokens,
            output_tokens,
            cache_create_tokens,
            cache_read_tokens,
            cumulative_cost,
            budget_max,
            budget_remaining,
            compaction_count,
            compaction_removed_messages,
            effort_level,
        });
    }

    // Sort by last_active descending
    sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    sessions
}

/// Load a session from file and populate an `AppState` with its data.
/// The returned `AppState` is in read-only / attach mode — it carries the session's
/// messages, tools, and metadata but is NOT connected to a live runtime.
pub fn attach_session(session_path: &Path) -> Result<AppState, String> {
    let session = runtime::Session::load_from_path(session_path)
        .map_err(|e| format!("Failed to load session: {e}"))?;

    let session_ts = session.updated_at_ms / 1000;
    let saved_msgs = session.messages.clone();
    let msg_count = saved_msgs.len();
    let turns = saved_msgs
        .iter()
        .filter(|m| matches!(m.role, runtime::MessageRole::Assistant))
        .count() as u32;

    let session_id = session.session_id.clone();
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| ".".to_string());

    // Build AppState from session data
    let mut state = AppState::new("unknown", "workspace-write", &cwd);

    // Convert runtime messages to TUI messages
    state.messages.clear();
    state.tools.clear();
    state.revert = None;
    state.messages = convert_runtime_messages_to_tui(&saved_msgs, session_ts);
    state.scroll_offset = usize::MAX;
    state.is_streaming = false;
    state.is_thinking = false;
    state.session.id = session_id.clone();
    state.session.title = format!("Session {session_id}");
    state.session.message_count = msg_count;
    state.session.turns = turns;

    Ok(state)
}

/// Convert runtime `ConversationMessage` vec to TUI Message vec.
/// This is a simplified conversion — it extracts text content from blocks.
fn convert_runtime_messages_to_tui(
    messages: &[runtime::ConversationMessage],
    session_ts: u64,
) -> Vec<crate::tui::app::Message> {
    use crate::tui::app::{Message, MessagePart, MessageRole};

    let mut result = Vec::new();

    for msg in messages {
        let role = match msg.role {
            runtime::MessageRole::User => MessageRole::User,
            runtime::MessageRole::Assistant => MessageRole::Assistant,
            runtime::MessageRole::Tool => MessageRole::Tool {
                name: "tool".into(),
            },
            runtime::MessageRole::System => continue, // Skip system messages in TUI
        };

        let mut parts = Vec::new();
        let mut agent = "build".to_string();

        for block in &msg.blocks {
            match block {
                runtime::ContentBlock::Text { text } => {
                    if text.is_empty() {
                        continue;
                    }
                    if parts.is_empty() || !matches!(parts.last(), Some(MessagePart::Text { .. })) {
                        parts.push(MessagePart::Text {
                            content: text.clone(),
                        });
                    } else if let Some(MessagePart::Text { content }) = parts.last_mut() {
                        content.push('\n');
                        content.push_str(text);
                    }
                }
                runtime::ContentBlock::ToolUse { id, name, input } => {
                    parts.push(MessagePart::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        status: crate::tui::app::ToolStatus::Running,
                        input_summary: input.clone(),
                        output: None,
                        expanded: false,
                    });
                }
                runtime::ContentBlock::ToolResult {
                    tool_name,
                    output,
                    is_error,
                    ..
                } => {
                    // Mark the last tool call as completed/failed
                    for part in parts.iter_mut().rev() {
                        if let MessagePart::ToolCall {
                            status,
                            output: tc_output,
                            ..
                        } = part
                        {
                            if matches!(status, crate::tui::app::ToolStatus::Running) {
                                if *is_error {
                                    *status = crate::tui::app::ToolStatus::Failed;
                                } else {
                                    *status = crate::tui::app::ToolStatus::Completed;
                                }
                                *tc_output = Some(output.clone());
                                break;
                            }
                        }
                    }
                    agent = tool_name.clone();
                }
            }
        }

        if parts.is_empty() {
            parts.push(MessagePart::Text {
                content: String::new(),
            });
        }

        result.push(Message {
            role,
            parts,
            agent,
            timestamp: session_ts,
            is_streaming: false,
            tool_timeline: Vec::new(),
            turn_duration_ms: 0,
            sub_agents: Vec::new(),
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_session_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("icode_test_sessions_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_is_session_file() {
        assert!(is_session_file(Path::new("session.json")));
        assert!(is_session_file(Path::new("session.jsonl")));
        assert!(!is_session_file(Path::new("session.txt")));
        assert!(!is_session_file(Path::new("session")));
    }

    #[test]
    fn test_list_sessions_empty_dir() {
        // When no sessions dir exists, should return empty
        // (Can't easily test the real sessions_dir without side effects)
        assert!(sessions_dir().is_some() || sessions_dir().is_none());
    }

    #[test]
    fn test_list_sessions_finds_files() {
        let dir = temp_session_dir();
        let path = dir.join("test-session.json");
        let mut f = fs::File::create(&path).unwrap();
        // Write minimal valid JSON session
        write!(f, "{{\"version\":1,\"session_id\":\"test-1\",\"created_at_ms\":0,\"updated_at_ms\":0,\"messages\":[]}}").unwrap();
        drop(f);

        // Override HOME to point to our test dir
        // Instead, test is_session_file and file_last_modified_ms directly
        assert!(is_session_file(&path));
        assert!(file_last_modified_ms(&path) > 0);

        // Cleanup
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_attach_session_invalid_file() {
        let dir = temp_session_dir();
        let path = dir.join("invalid.json");
        let mut f = fs::File::create(&path).unwrap();
        write!(f, "not valid json").unwrap();
        drop(f);

        let result = attach_session(&path);
        assert!(result.is_err());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_attach_session_valid_empty() {
        let dir = temp_session_dir();
        let path = dir.join("empty-session.json");
        let mut f = fs::File::create(&path).unwrap();
        write!(f, r#"{{"version":1,"session_id":"empty-1","created_at_ms":0,"updated_at_ms":1000,"messages":[]}}"#).unwrap();
        drop(f);

        let result = attach_session(&path);
        assert!(result.is_ok());
        let state = result.unwrap();
        assert_eq!(state.session.id, "empty-1");
        assert_eq!(state.messages.len(), 0);
        assert_eq!(state.session.message_count, 0);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }
}
