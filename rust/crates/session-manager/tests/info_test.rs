use session_manager::info;
use session_manager::types::SessionInfo;
use std::fs;
use tempfile::TempDir;

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    let content = serde_json::to_string(value).unwrap();
    fs::write(path, content).unwrap();
}

fn make_session_info(id: &str, last_at: &str, msg_count: usize) -> SessionInfo {
    SessionInfo {
        session_id: id.to_string(),
        message_count: msg_count,
        first_message_at: Some("2025-01-01T00:00:00Z".to_string()),
        last_message_at: Some(last_at.to_string()),
        agents_used: vec!["build".to_string()],
        has_todos: false,
        has_transcript: true,
    }
}

#[test]
fn info_valid_session() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-03-01T00:00:00Z", 42);
    write_json(&dir.path().join("sess-001.json"), &session);

    let result = info::session_info(dir.path().to_str().unwrap(), "sess-001").unwrap();
    assert_eq!(result.session_id, "sess-001");
    assert_eq!(result.message_count, 42);
    assert_eq!(result.agents_used, vec!["build"]);
    assert!(result.has_transcript);
}

#[test]
fn info_invalid_session() {
    let dir = TempDir::new().unwrap();
    let result = info::session_info(dir.path().to_str().unwrap(), "nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Session not found"));
}
