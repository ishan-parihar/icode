use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::constants::{BOULDER_DIR, BOULDER_FILE};
use crate::types::{BoulderState, SessionOrigin, TaskSessionState, UpsertTaskInput};

#[must_use]
pub fn boulder_file_path(directory: &str) -> String {
    format!("{directory}/{BOULDER_DIR}/{BOULDER_FILE}")
}

#[must_use]
pub fn read_boulder_state(directory: &str) -> Option<BoulderState> {
    let path = boulder_file_path(directory);
    let content = fs::read_to_string(&path).ok()?;
    let state: BoulderState = serde_json::from_str(&content).ok()?;

    if state.active_plan.is_empty() || state.started_at.is_empty() || state.plan_name.is_empty() {
        return None;
    }

    Some(state)
}

#[must_use]
pub fn write_boulder_state(directory: &str, state: &BoulderState) -> bool {
    let path = boulder_file_path(directory);
    let path_ref = Path::new(&path);
    if let Some(parent) = path_ref.parent() {
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let Ok(json) = serde_json::to_string_pretty(state) else {
        return false;
    };
    fs::write(&path, json).is_ok()
}

#[must_use]
pub fn append_session_id(
    directory: &str,
    session_id: &str,
    origin: SessionOrigin,
) -> Option<BoulderState> {
    let mut state = read_boulder_state(directory)?;

    if !state.session_ids.contains(&session_id.to_string()) {
        state.session_ids.push(session_id.to_string());
    }
    state.session_origins.insert(session_id.to_string(), origin);

    if write_boulder_state(directory, &state) {
        Some(state)
    } else {
        None
    }
}

#[must_use]
pub fn clear_boulder_state(directory: &str) -> bool {
    let path = boulder_file_path(directory);
    fs::remove_file(&path).is_ok()
}

#[must_use]
pub fn get_task_session_state(directory: &str, task_key: &str) -> Option<TaskSessionState> {
    let state = read_boulder_state(directory)?;
    state.task_sessions.get(task_key).cloned()
}

#[must_use]
pub fn upsert_task_session_state(directory: &str, input: UpsertTaskInput) -> Option<BoulderState> {
    let mut state = read_boulder_state(directory)?;

    let now = chrono_now();
    state.task_sessions.insert(
        input.task_key.clone(),
        TaskSessionState {
            task_key: input.task_key,
            task_label: input.task_label,
            task_title: input.task_title,
            session_id: input.session_id,
            agent: input.agent,
            category: input.category,
            updated_at: now,
        },
    );

    if write_boulder_state(directory, &state) {
        Some(state)
    } else {
        None
    }
}

#[must_use]
pub fn create_boulder_state(
    plan_path: &str,
    session_id: &str,
    agent: Option<&str>,
    worktree_path: Option<&str>,
) -> BoulderState {
    let plan_name = Path::new(plan_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let started_at = chrono_now();

    let mut session_origins = HashMap::new();
    session_origins.insert(session_id.to_string(), SessionOrigin::Direct);

    BoulderState {
        active_plan: plan_path.to_string(),
        started_at,
        session_ids: vec![session_id.to_string()],
        session_origins,
        plan_name,
        agent: agent.map(String::from),
        worktree_path: worktree_path.map(String::from),
        task_sessions: HashMap::new(),
    }
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
