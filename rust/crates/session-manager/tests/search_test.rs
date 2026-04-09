use session_manager::search;
use session_manager::types::SessionMessage;
use tempfile::TempDir;

fn write_json(path: &std::path::Path, value: &impl serde::Serialize) {
    let json = serde_json::to_string_pretty(value).unwrap();
    std::fs::write(path, json).unwrap();
}

fn make_message(role: &str, content: &str) -> SessionMessage {
    SessionMessage {
        role: role.to_string(),
        timestamp: None,
        content: content.to_string(),
    }
}

#[test]
fn search_finds_match() {
    let dir = TempDir::new().unwrap();

    let messages = vec![
        make_message("user", "Hello world"),
        make_message("assistant", "The quick brown fox jumps"),
    ];
    write_json(&dir.path().join("sess-001.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "fox", false, None);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, "sess-001");
    assert_eq!(results[0].message_index, 1);
}

#[test]
fn search_case_insensitive_by_default() {
    let dir = TempDir::new().unwrap();

    let messages = vec![make_message("user", "Hello WORLD")];
    write_json(&dir.path().join("sess-001.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "world", false, None);
    assert_eq!(results.len(), 1);
}

#[test]
fn search_case_sensitive() {
    let dir = TempDir::new().unwrap();

    let messages = vec![make_message("user", "Hello World")];
    write_json(&dir.path().join("sess-001.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "world", true, None);
    assert_eq!(results.len(), 0);
}

#[test]
fn search_no_results() {
    let dir = TempDir::new().unwrap();

    let messages = vec![make_message("user", "Hello there")];
    write_json(&dir.path().join("sess-001.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "notfound", false, None);
    assert_eq!(results.len(), 0);
}

#[test]
fn search_with_limit() {
    let dir = TempDir::new().unwrap();

    let messages: Vec<SessionMessage> = (0..5)
        .map(|i| make_message("user", &format!("The test message number {i}")))
        .collect();
    write_json(&dir.path().join("sess-001.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "test", false, Some(2));
    assert_eq!(results.len(), 2);
}

#[test]
fn search_provides_context() {
    let dir = TempDir::new().unwrap();

    let messages = vec![make_message(
        "user",
        "This is a longer message with the word target embedded inside it",
    )];
    write_json(&dir.path().join("sess-001.json"), &messages);

    let results = search::search_sessions(dir.path().to_str().unwrap(), "target", false, None);
    assert_eq!(results.len(), 1);
    assert!(!results[0].context_before.is_empty() || !results[0].context_after.is_empty());
}
