use session_manager::search;
use session_manager::types::{SessionInfo, SessionMessage};
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

fn make_message(role: &str, content: &str) -> SessionMessage {
    SessionMessage {
        role: role.to_string(),
        timestamp: Some("2025-01-01T00:00:00Z".to_string()),
        content: content.to_string(),
    }
}

#[test]
fn search_finds_match() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-01-01T00:00:00Z", 3);
    write_json(&dir.path().join("sess-001.json"), &session);

    let messages = vec![
        make_message("user", "Hello world"),
        make_message("assistant", "The quick brown fox jumps"),
    ];
    write_json(&dir.path().join("sess-001_messages.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "fox", false, None);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, "sess-001");
    assert_eq!(results[0].message_index, 1);
}

#[test]
fn search_case_insensitive_by_default() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-01-01T00:00:00Z", 1);
    write_json(&dir.path().join("sess-001.json"), &session);

    let messages = vec![make_message("user", "Hello WORLD")];
    write_json(&dir.path().join("sess-001_messages.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "world", false, None);
    assert_eq!(results.len(), 1);
}

#[test]
fn search_case_sensitive() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-01-01T00:00:00Z", 1);
    write_json(&dir.path().join("sess-001.json"), &session);

    let messages = vec![make_message("user", "Hello World")];
    write_json(&dir.path().join("sess-001_messages.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "world", true, None);
    assert_eq!(results.len(), 0);
}

#[test]
fn search_no_results() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-01-01T00:00:00Z", 1);
    write_json(&dir.path().join("sess-001.json"), &session);

    let messages = vec![make_message("user", "Hello there")];
    write_json(&dir.path().join("sess-001_messages.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "notfound", false, None);
    assert_eq!(results.len(), 0);
}

#[test]
fn search_with_limit() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-01-01T00:00:00Z", 5);
    write_json(&dir.path().join("sess-001.json"), &session);

    let messages: Vec<SessionMessage> = (0..5)
        .map(|i| make_message("user", &format!("The test message number {i}")))
        .collect();
    write_json(&dir.path().join("sess-001_messages.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "test", false, Some(2));
    assert_eq!(results.len(), 2);
}

#[test]
fn search_provides_context() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-01-01T00:00:00Z", 1);
    write_json(&dir.path().join("sess-001.json"), &session);

    let messages = vec![make_message(
        "user",
        "This is a longer message with the word target embedded inside it",
    )];
    write_json(&dir.path().join("sess-001_messages.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "target", false, None);
    assert_eq!(results.len(), 1);
    assert!(!results[0].context_before.is_empty() || !results[0].context_after.is_empty());
}
