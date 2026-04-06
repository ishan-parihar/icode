use serde::{Deserialize, Serialize};

/// Input for the `task()` delegation tool.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskInput {
    /// Category for model routing (mutually exclusive with `subagent_type`).
    pub category: Option<String>,
    /// Direct agent name (mutually exclusive with category).
    pub subagent_type: Option<String>,
    /// Task prompt.
    pub prompt: String,
    /// Skills to inject.
    #[serde(default)]
    pub load_skills: Vec<String>,
    /// Run in background (default: false).
    #[serde(default)]
    pub run_in_background: bool,
    /// Short description for tracking.
    pub description: Option<String>,
    /// Resume existing session by ID.
    pub session_id: Option<String>,
    /// Task dependencies this task is blocked by.
    pub blocked_by: Option<Vec<String>>,
    /// Tasks that this task blocks.
    pub blocks: Option<Vec<String>>,
}

/// Output from the `task()` tool.
#[derive(Debug, Clone, Serialize)]
pub struct TaskOutput {
    pub task_id: String,
    pub session_id: String,
    pub status: TaskStatus,
    pub result: Option<String>,
}

/// Lifecycle status of a delegated task.
#[derive(Debug, Clone, Serialize)]
pub enum TaskStatus {
    Spawned,
    Running,
    Completed(String),
    Failed(String),
    Cancelled,
}
