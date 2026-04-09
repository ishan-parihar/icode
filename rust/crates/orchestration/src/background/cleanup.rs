use super::types::{BackgroundTask, BackgroundTaskStatus};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[must_use]
pub fn cleanup_expired<S: std::hash::BuildHasher>(
    tasks: &mut HashMap<String, BackgroundTask, S>,
    ttl: Duration,
) -> usize {
    let now = SystemTime::now();
    let mut to_remove = Vec::new();

    for (id, task) in tasks.iter() {
        match task.status {
            BackgroundTaskStatus::Completed
            | BackgroundTaskStatus::Cancelled
            | BackgroundTaskStatus::Failed => {
                if let Some(completed_at) = task.completed_at {
                    if now.duration_since(completed_at).unwrap_or(Duration::ZERO) > ttl {
                        to_remove.push(id.clone());
                    }
                }
            }
            BackgroundTaskStatus::Running => {
                if let Some(started_at) = task.started_at {
                    if now.duration_since(started_at).unwrap_or(Duration::ZERO) > ttl * 3 {
                        to_remove.push(id.clone());
                    }
                }
            }
            BackgroundTaskStatus::Pending => {
                if now
                    .duration_since(task.created_at)
                    .unwrap_or(Duration::ZERO)
                    > ttl
                {
                    to_remove.push(id.clone());
                }
            }
        }
    }

    for id in &to_remove {
        tasks.remove(id);
    }

    to_remove.len()
}

#[must_use]
pub fn status_counts<S: std::hash::BuildHasher>(
    tasks: &HashMap<String, BackgroundTask, S>,
) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for task in tasks.values() {
        let key = format!("{:?}", task.status);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}
