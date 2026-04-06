use std::fs;
use std::path::Path;

use crate::types::{SessionInfo, SessionMessage, SessionSearchResult};

const CONTEXT_CHARS: usize = 50;

fn extract_context(
    content: &str,
    match_start: usize,
    match_end: usize,
) -> (String, String, String) {
    let before_start = match_start.saturating_sub(CONTEXT_CHARS);
    let after_end = (match_end + CONTEXT_CHARS).min(content.len());

    let before = content[before_start..match_start].to_string();
    let matched = content[match_start..match_end].to_string();
    let after = content[match_end..after_end].to_string();

    (before, matched, after)
}

#[must_use]
pub fn search_sessions(
    store_dir: &str,
    query: &str,
    case_sensitive: bool,
    limit: Option<usize>,
) -> Vec<SessionSearchResult> {
    let dir = Path::new(store_dir);
    if !dir.is_dir() || query.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();

    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
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
        let session_id = info.session_id;

        let messages_path = Path::new(store_dir).join(format!("{session_id}_messages.json"));
        let Ok(messages_content) = fs::read_to_string(&messages_path) else {
            continue;
        };

        let Ok(messages) = serde_json::from_str::<Vec<SessionMessage>>(&messages_content) else {
            continue;
        };

        let search_query = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        for (idx, msg) in messages.iter().enumerate() {
            let search_content = if case_sensitive {
                msg.content.clone()
            } else {
                msg.content.to_lowercase()
            };

            let mut start = 0;
            while let Some(pos) = search_content[start..].find(&search_query) {
                let match_start = start + pos;
                let match_end = match_start + query.len();

                let (before, _matched, after) =
                    extract_context(&msg.content, match_start, match_end);

                results.push(SessionSearchResult {
                    session_id: session_id.clone(),
                    message_index: idx,
                    matched_content: query.to_string(),
                    context_before: before,
                    context_after: after,
                });

                start = match_end;

                if let Some(limit) = limit {
                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }
    }

    results
}
