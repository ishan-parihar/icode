use session_manager::list;
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
fn list_empty_directory() {
    let dir = TempDir::new().unwrap();
    let result = list::list_sessions(dir.path().to_str().unwrap(), None, None, None);
    assert_eq!(result.sessions.len(), 0);
    assert_eq!(result.total, 0);
}

#[test]
fn list_nonexistent_directory() {
    let result = list::list_sessions("/nonexistent/path", None, None, None);
    assert_eq!(result.sessions.len(), 0);
    assert_eq!(result.total, 0);
}

#[test]
fn list_single_session() {
    let dir = TempDir::new().unwrap();
    let session = make_session_info("sess-001", "2025-03-01T10:00:00Z", 5);
    write_json(&dir.path().join("sess-001.json"), &session);

    let result = list::list_sessions(dir.path().to_str().unwrap(), None, None, None);
    assert_eq!(result.total, 1);
    assert_eq!(result.sessions.len(), 1);
    assert_eq!(result.sessions[0].session_id, "sess-001");
}

#[test]
fn list_multiple_sessions_sorted_descending() {
    let dir = TempDir::new().unwrap();
    let s1 = make_session_info("sess-001", "2025-01-01T00:00:00Z", 3);
    let s2 = make_session_info("sess-002", "2025-03-01T00:00:00Z", 10);
    let s3 = make_session_info("sess-003", "2025-02-01T00:00:00Z", 7);
    write_json(&dir.path().join("sess-001.json"), &s1);
    write_json(&dir.path().join("sess-002.json"), &s2);
    write_json(&dir.path().join("sess-003.json"), &s3);

    let result = list::list_sessions(dir.path().to_str().unwrap(), None, None, None);
    assert_eq!(result.total, 3);
    assert_eq!(result.sessions[0].session_id, "sess-002");
    assert_eq!(result.sessions[1].session_id, "sess-003");
    assert_eq!(result.sessions[2].session_id, "sess-001");
}

#[test]
fn list_with_limit() {
    let dir = TempDir::new().unwrap();
    for i in 1..=5 {
        let s = make_session_info(
            &format!("sess-{i:03}"),
            &format!("2025-0{i}-01T00:00:00Z"),
            i,
        );
        write_json(&dir.path().join(format!("sess-{i:03}.json")), &s);
    }

    let result = list::list_sessions(dir.path().to_str().unwrap(), Some(2), None, None);
    assert_eq!(result.sessions.len(), 2);
    assert_eq!(result.total, 5);
}

#[test]
fn list_with_from_date_filter() {
    let dir = TempDir::new().unwrap();
    let s1 = make_session_info("sess-001", "2025-01-01T00:00:00Z", 1);
    let s2 = make_session_info("sess-002", "2025-06-01T00:00:00Z", 2);
    write_json(&dir.path().join("sess-001.json"), &s1);
    write_json(&dir.path().join("sess-002.json"), &s2);

    let result = list::list_sessions(
        dir.path().to_str().unwrap(),
        None,
        Some("2025-03-01T00:00:00Z"),
        None,
    );
    assert_eq!(result.total, 1);
    assert_eq!(result.sessions[0].session_id, "sess-002");
}

#[test]
fn list_with_to_date_filter() {
    let dir = TempDir::new().unwrap();
    let s1 = make_session_info("sess-001", "2025-01-01T00:00:00Z", 1);
    let s2 = make_session_info("sess-002", "2025-12-01T00:00:00Z", 2);
    write_json(&dir.path().join("sess-001.json"), &s1);
    write_json(&dir.path().join("sess-002.json"), &s2);

    let result = list::list_sessions(
        dir.path().to_str().unwrap(),
        None,
        None,
        Some("2025-06-01T00:00:00Z"),
    );
    assert_eq!(result.total, 1);
    assert_eq!(result.sessions[0].session_id, "sess-001");
}
