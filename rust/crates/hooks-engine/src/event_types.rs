use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Message,
    SessionEvent,
    Transform,
    Params,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEventType {
    Created,
    Deleted,
    Idle,
    Error(String),
    Compacted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub r#type: SessionEventType,
    pub session_id: String,
    pub timestamp: String,
}
