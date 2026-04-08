use session_manager::read;
use session_manager::types::SessionMessage;
use std::fs;
use tempfile::TempDir;

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    let content = serde_json::to_string(value).unwrap();
    fs::write(path, content).unwrap();
}

fn make_message(role: &str, content: &str) -> SessionMessage {
    SessionMessage {
        role: role.to_string(),
        timestamp: Some("2025-01-01T00:00:00Z".to_string()),
        content: content.to_string(),
    }
}

#[test]
fn read_existing_session() {
    let dir = TempDir::new().unwrap();
    let messages = vec![
        make_message("user", "Hello"),
        make_message("assistant", "Hi there"),
    ];
    write_json(&dir.path().join("sess-001.json"), &messages);

    let result = read::read_session(dir.path().to_str().unwrap(), "sess-001", None).unwrap();
    assert_eq!(result.session_id, "sess-001");
    assert_eq!(result.message_count, 2);
    assert_eq!(result.messages.len(), 2);
}

#[test]
fn read_missing_session() {
    let dir = TempDir::new().unwrap();
    let result = read::read_session(dir.path().to_str().unwrap(), "nonexistent", None);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Session not found"));
}

#[test]
fn read_with_limit() {
    let dir = TempDir::new().unwrap();
    let messages: Vec<SessionMessage> = (0..10)
        .map(|i| make_message("user", &format!("Message {i}")))
        .collect();
    write_json(&dir.path().join("sess-001.json"), &messages);

    let result = read::read_session(dir.path().to_str().unwrap(), "sess-001", Some(3)).unwrap();
    assert_eq!(result.message_count, 10);
    assert_eq!(result.messages.len(), 3);
}

#[test]
fn read_empty_messages() {
    let dir = TempDir::new().unwrap();
    let messages: Vec<SessionMessage> = vec![];
    write_json(&dir.path().join("sess-empty.json"), &messages);

    let result = read::read_session(dir.path().to_str().unwrap(), "sess-empty", None).unwrap();
    assert_eq!(result.message_count, 0);
    assert_eq!(result.messages.len(), 0);
}
