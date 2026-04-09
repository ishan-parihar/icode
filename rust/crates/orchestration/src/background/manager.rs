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
    /// Returns `None` if concurrency limit is reached for the model.
    pub fn register(&self, description: String, model: String) -> Option<String> {
        if !self.concurrency.try_acquire(&model) {
            return None;
        }
        let id = format!("bg_{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let task = BackgroundTask::new(id.clone(), description, "default".to_string(), model);
        self.tasks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id.clone(), task);
        Some(id)
    }

    /// Get a task by ID.
    pub fn get(&self, task_id: &str) -> Option<BackgroundTask> {
        self.tasks
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(task_id)
            .cloned()
    }

    /// Update task status.
    pub fn update_status(&self, task_id: &str, status: BackgroundTaskStatus) -> Result<(), String> {
        let is_running = matches!(status, BackgroundTaskStatus::Running);
        let mut tasks = self
            .tasks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("task {task_id} not found"))?;
        task.status = status;
        if is_running {
            task.started_at = Some(std::time::SystemTime::now());
        }
        Ok(())
    }

    /// Mark task as completed with result.
    pub fn complete(&self, task_id: &str, result: String) -> Result<(), String> {
        let mut tasks = self
            .tasks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("task {task_id} not found"))?;
        let model = task.model.clone();
        task.status = BackgroundTaskStatus::Completed;
        task.completed_at = Some(std::time::SystemTime::now());
        task.result = Some(result);
        drop(tasks);
        self.concurrency.release(&model);
        Ok(())
    }

    /// Mark task as failed with error.
    pub fn fail(&self, task_id: &str, error: String) -> Result<(), String> {
        let mut tasks = self
            .tasks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("task {task_id} not found"))?;
        let model = task.model.clone();
        task.status = BackgroundTaskStatus::Failed;
        task.completed_at = Some(std::time::SystemTime::now());
        task.error = Some(error);
        drop(tasks);
        self.concurrency.release(&model);
        Ok(())
    }

    /// Cancel a task. Fails if already completed.
    pub fn cancel(&self, task_id: &str) -> Result<(), String> {
        let mut tasks = self
            .tasks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                let model = task.model.clone();
                task.status = BackgroundTaskStatus::Cancelled;
                task.completed_at = Some(std::time::SystemTime::now());
                drop(tasks);
                self.concurrency.release(&model);
                Ok(())
            }
        }
    }

    /// List all tasks.
    pub fn list(&self) -> Vec<BackgroundTask> {
        self.tasks
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    pub fn concurrency(&self) -> &ConcurrencyLimiter {
        &self.concurrency
    }

    /// Cleanup expired tasks, return count removed.
    pub fn cleanup(&self, ttl: Duration) -> usize {
        cleanup_expired(
            &mut self
                .tasks
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            ttl,
        )
    }
}
