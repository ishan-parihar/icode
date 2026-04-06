use std::fs;
use std::path::Path;

use crate::types::SessionInfo;

pub fn session_info(store_dir: &str, session_id: &str) -> Result<SessionInfo, String> {
    let path = Path::new(store_dir).join(format!("{session_id}.json"));

    if !path.exists() {
        return Err(format!("Session not found: {session_id}"));
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read session file: {e}"))?;

    let info: SessionInfo =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse session info: {e}"))?;

    Ok(info)
}
