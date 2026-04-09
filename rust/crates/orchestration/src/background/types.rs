use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// A background task tracked by the `BackgroundManager`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackgroundTask {
    pub id: String,
    pub description: String,
    pub session_id: String,
    pub status: BackgroundTaskStatus,
    pub created_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub model: String,
}

/// Status of a background task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackgroundTaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl BackgroundTask {
    /// Create a new background task in Pending state.
    #[must_use]
    pub fn new(id: String, description: String, session_id: String, model: String) -> Self {
        Self {
            id,
            description,
            session_id,
            status: BackgroundTaskStatus::Pending,
            created_at: SystemTime::now(),
            completed_at: None,
            result: None,
            error: None,
            model,
        }
    }
}
