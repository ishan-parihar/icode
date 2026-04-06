use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoulderState {
    pub active_plan: String,
    pub started_at: String,
    pub session_ids: Vec<String>,
    pub session_origins: HashMap<String, SessionOrigin>,
    pub plan_name: String,
    pub agent: Option<String>,
    pub worktree_path: Option<String>,
    pub task_sessions: HashMap<String, TaskSessionState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOrigin {
    Direct,
    Appended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSessionState {
    pub task_key: String,
    pub task_label: String,
    pub task_title: String,
    pub session_id: String,
    pub agent: Option<String>,
    pub category: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanProgress {
    pub total: usize,
    pub completed: usize,
    pub is_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopLevelTask {
    pub key: String,
    pub section: TaskSection,
    pub label: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSection {
    Todo,
    FinalWave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notepad {
    pub plan_name: String,
    pub learnings: String,
    pub decisions: String,
    pub issues: String,
    pub verification: String,
    pub problems: String,
}

#[derive(Debug, Clone)]
pub struct UpsertTaskInput {
    pub task_key: String,
    pub task_label: String,
    pub task_title: String,
    pub session_id: String,
    pub agent: Option<String>,
    pub category: Option<String>,
}
