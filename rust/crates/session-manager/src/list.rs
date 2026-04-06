use std::fs;
use std::path::Path;

use crate::types::{SessionInfo, SessionListResponse};

#[must_use]
pub fn list_sessions(
    store_dir: &str,
    limit: Option<usize>,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> SessionListResponse {
    let dir = Path::new(store_dir);
    if !dir.is_dir() {
        return SessionListResponse {
            sessions: vec![],
            total: 0,
        };
    }

    let mut sessions = Vec::new();

    let Ok(entries) = fs::read_dir(dir) else {
        return SessionListResponse {
            sessions: vec![],
            total: 0,
        };
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        let Ok(info) = serde_json::from_str::<SessionInfo>(&content) else {
            continue;
        };

        if let Some(from) = from_date {
            if let Some(last) = &info.last_message_at {
                if last.as_str() < from {
                    continue;
                }
            }
        }
        if let Some(to) = to_date {
            if let Some(last) = &info.last_message_at {
                if last.as_str() > to {
                    continue;
                }
            }
        }

        sessions.push(info);
    }

    sessions.sort_by(|a, b| {
        let a_time = a.last_message_at.as_deref().unwrap_or("");
        let b_time = b.last_message_at.as_deref().unwrap_or("");
        b_time.cmp(a_time)
    });

    let total = sessions.len();

    if let Some(limit) = limit {
        sessions.truncate(limit);
    }

    SessionListResponse { sessions, total }
}
