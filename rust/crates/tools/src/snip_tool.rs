use runtime::{session_control::resolve_session_reference_for, ContentBlock, MessageRole, Session};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_SNIPPET_CHARS: usize = 500;
const DEFAULT_COUNT: usize = 20;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SnipToolInput {
    /// Session reference: id, path, or 'latest'
    pub session_ref: String,
    /// Number of recent messages to extract (default: 20)
    pub count: Option<usize>,
    /// Whether to include `ToolResult` blocks (default: true)
    pub include_tool_results: Option<bool>,
    /// Optional case-insensitive substring filter on message text
    pub search_term: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SnipToolOutput {
    pub session_id: String,
    pub message_count: usize,
    pub total_messages: usize,
    pub snippets: Vec<SnippetEntry>,
}

#[derive(Debug, Serialize)]
pub struct SnippetEntry {
    pub turn_index: usize,
    pub role: String,
    pub text: String,
    pub tool_name: Option<String>,
}

pub fn execute_snip_tool(input: &SnipToolInput) -> Result<SnipToolOutput, String> {
    let base_dir = std::env::current_dir().map_err(|e| e.to_string())?;

    let handle = resolve_session_reference_for(&base_dir, &input.session_ref)
        .map_err(|e| format!("Session not found: {} ({})", input.session_ref, e))?;

    if !handle.path.exists() {
        return Err(format!("Session not found: {}", input.session_ref));
    }

    let session = Session::load_from_path(&handle.path)
        .map_err(|e| format!("Failed to load session: {e}"))?;

    let session_id = handle.id;
    let total_messages = session.messages.len();
    let count = input.count.unwrap_or(DEFAULT_COUNT);
    let include_tool_results = input.include_tool_results.unwrap_or(true);
    let search_term = input.search_term.as_ref().map(|s| s.to_lowercase());

    let mut all_entries: Vec<SnippetEntry> = Vec::new();

    for (msg_idx, msg) in session.messages.iter().enumerate() {
        let role_str = match msg.role {
            MessageRole::System => "system".to_string(),
            MessageRole::User => "user".to_string(),
            MessageRole::Assistant => "assistant".to_string(),
            MessageRole::Tool => "tool".to_string(),
        };

        for block in &msg.blocks {
            let (text, tool_name) = match block {
                ContentBlock::Text { text } => {
                    if let Some(ref term) = search_term {
                        if !text.to_lowercase().contains(term) {
                            continue;
                        }
                    }
                    (truncate_text(text), None)
                }
                ContentBlock::ToolUse {
                    id: _,
                    name,
                    input: _,
                } => {
                    if let Some(ref term) = search_term {
                        if !name.to_lowercase().contains(term) {
                            continue;
                        }
                    }
                    (format!("[ToolUse: {name}]"), Some(name.clone()))
                }
                ContentBlock::ToolResult {
                    tool_use_id: _,
                    tool_name: tname,
                    output,
                    is_error: _,
                } => {
                    if !include_tool_results {
                        continue;
                    }
                    if let Some(ref term) = search_term {
                        let matches_name = tname.to_lowercase().contains(term);
                        let matches_output = output.to_lowercase().contains(term);
                        if !matches_name && !matches_output {
                            continue;
                        }
                    }
                    (truncate_text(output), Some(tname.clone()))
                }
            };

            all_entries.push(SnippetEntry {
                turn_index: msg_idx,
                role: role_str.clone(),
                text,
                tool_name,
            });
        }
    }

    let snippets: Vec<SnippetEntry> = if all_entries.len() > count {
        all_entries.split_off(all_entries.len() - count)
    } else {
        all_entries
    };

    Ok(SnipToolOutput {
        session_id,
        message_count: snippets.len(),
        total_messages,
        snippets,
    })
}

fn truncate_text(text: &str) -> String {
    if text.chars().count() > MAX_SNIPPET_CHARS {
        text.chars().take(MAX_SNIPPET_CHARS).collect::<String>() + "..."
    } else {
        text.to_string()
    }
}

pub fn snip_tool_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(SnipToolInput)).unwrap()
}
