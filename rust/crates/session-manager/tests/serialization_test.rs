use serde_json;
use session_manager::types::{
    SessionInfo, SessionListResponse, SessionMessage, SessionReadResponse, SessionSearchResult,
};

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
fn session_info_roundtrip() {
    let info = make_session_info("test-1", "2025-01-01T00:00:00Z", 10);
    let json = serde_json::to_string(&info).unwrap();
    let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_id, info.session_id);
    assert_eq!(parsed.message_count, info.message_count);
}

#[test]
fn session_message_roundtrip() {
    let msg = make_message("assistant", "Hello, world!");
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: SessionMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content, msg.content);
    assert_eq!(parsed.role, msg.role);
}

#[test]
fn session_list_response_roundtrip() {
    let resp = SessionListResponse {
        sessions: vec![make_session_info("s1", "2025-01-01T00:00:00Z", 1)],
        total: 1,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: SessionListResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.total, 1);
    assert_eq!(parsed.sessions.len(), 1);
}

#[test]
fn session_read_response_roundtrip() {
    let resp = SessionReadResponse {
        session_id: "test".to_string(),
        messages: vec![make_message("user", "hi")],
        message_count: 1,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: SessionReadResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_id, "test");
    assert_eq!(parsed.messages.len(), 1);
}

#[test]
fn session_search_result_roundtrip() {
    let result = SessionSearchResult {
        session_id: "s1".to_string(),
        message_index: 0,
        matched_content: "test".to_string(),
        context_before: "before".to_string(),
        context_after: "after".to_string(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: SessionSearchResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_id, "s1");
    assert_eq!(parsed.matched_content, "test");
}

#[test]
fn messages_vec_roundtrip() {
    let messages = vec![
        make_message("user", "Hello"),
        make_message("assistant", "Hi"),
    ];
    let json = serde_json::to_string(&messages).unwrap();
    let parsed: Vec<SessionMessage> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].role, "user");
    assert_eq!(parsed[1].role, "assistant");
}
