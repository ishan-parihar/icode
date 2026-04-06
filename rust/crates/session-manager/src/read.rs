use std::fs;
use std::path::Path;

use crate::types::{SessionMessage, SessionReadResponse};

pub fn read_session(
    store_dir: &str,
    session_id: &str,
    limit: Option<usize>,
) -> Result<SessionReadResponse, String> {
    let path = Path::new(store_dir).join(format!("{session_id}.json"));

    if !path.exists() {
        return Err(format!("Session not found: {session_id}"));
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read session file: {e}"))?;

    let messages: Vec<SessionMessage> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse session messages: {e}"))?;

    let message_count = messages.len();

    let messages = match limit {
        Some(limit) => messages.into_iter().take(limit).collect(),
        None => messages,
    };

    Ok(SessionReadResponse {
        session_id: session_id.to_string(),
        messages,
        message_count,
    })
}
