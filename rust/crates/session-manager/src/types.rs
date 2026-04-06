use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub message_count: usize,
    pub first_message_at: Option<String>,
    pub last_message_at: Option<String>,
    pub agents_used: Vec<String>,
    pub has_todos: bool,
    pub has_transcript: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub timestamp: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionInfo>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionReadResponse {
    pub session_id: String,
    pub messages: Vec<SessionMessage>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSearchResult {
    pub session_id: String,
    pub message_index: usize,
    pub matched_content: String,
    pub context_before: String,
    pub context_after: String,
}
