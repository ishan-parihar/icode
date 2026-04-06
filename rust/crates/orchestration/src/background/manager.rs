use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use super::cleanup::cleanup_expired;
use super::concurrency::ConcurrencyLimiter;
use super::types::{BackgroundTask, BackgroundTaskStatus};

/// Manages background task lifecycle with concurrency limiting.
pub struct BackgroundManager {
    tasks: Arc<RwLock<HashMap<String, BackgroundTask>>>,
    concurrency: ConcurrencyLimiter,
    next_id: AtomicUsize,
}

impl BackgroundManager {
    #[must_use]
    pub fn new(default_concurrency: usize) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            concurrency: ConcurrencyLimiter::new(default_concurrency),
            next_id: AtomicUsize::new(1),
        }
    }

    /// Register a new background task. Returns `task_id` like `"bg_1"`.
    pub fn register(&self, description: String, model: String) -> String {
        let id = format!("bg_{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let task = BackgroundTask::new(id.clone(), description, "default".to_string(), model);
        self.tasks
            .write()
            .expect("rwlock poisoned")
            .insert(id.clone(), task);
        id
    }

    /// Get a task by ID.
    pub fn get(&self, task_id: &str) -> Option<BackgroundTask> {
        self.tasks
            .read()
            .expect("rwlock poisoned")
            .get(task_id)
            .cloned()
    }

    /// Update task status.
    pub fn update_status(&self, task_id: &str, status: BackgroundTaskStatus) -> Result<(), String> {
        let mut tasks = self.tasks.write().expect("rwlock poisoned");
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("task {task_id} not found"))?;
        task.status = status;
        Ok(())
    }

    /// Mark task as completed with result.
    pub fn complete(&self, task_id: &str, result: String) -> Result<(), String> {
        let mut tasks = self.tasks.write().expect("rwlock poisoned");
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("task {task_id} not found"))?;
        task.status = BackgroundTaskStatus::Completed;
        task.completed_at = Some(std::time::SystemTime::now());
        task.result = Some(result);
        Ok(())
    }

    /// Mark task as failed with error.
    pub fn fail(&self, task_id: &str, error: String) -> Result<(), String> {
        let mut tasks = self.tasks.write().expect("rwlock poisoned");
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("task {task_id} not found"))?;
        task.status = BackgroundTaskStatus::Failed;
        task.completed_at = Some(std::time::SystemTime::now());
        task.error = Some(error);
        Ok(())
    }

    /// Cancel a task. Fails if already completed.
    pub fn cancel(&self, task_id: &str) -> Result<(), String> {
        let mut tasks = self.tasks.write().expect("rwlock poisoned");
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("task {task_id} not found"))?;
        match task.status {
            BackgroundTaskStatus::Completed
            | BackgroundTaskStatus::Failed
            | BackgroundTaskStatus::Cancelled => {
                Err(format!("cannot cancel task in {:?} state", task.status))
            }
            _ => {
                task.status = BackgroundTaskStatus::Cancelled;
                task.completed_at = Some(std::time::SystemTime::now());
                Ok(())
            }
        }
    }

    /// List all tasks.
    pub fn list(&self) -> Vec<BackgroundTask> {
        self.tasks
            .read()
            .expect("rwlock poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub fn concurrency(&self) -> &ConcurrencyLimiter {
        &self.concurrency
    }

    /// Cleanup expired tasks, return count removed.
    pub fn cleanup(&self, ttl: Duration) -> usize {
        cleanup_expired(&mut self.tasks.write().expect("rwlock poisoned"), ttl)
    }
}
