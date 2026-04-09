use orchestration::background::{
    cleanup::cleanup_expired,
    concurrency::ConcurrencyLimiter,
    manager::BackgroundManager,
    types::{BackgroundTask, BackgroundTaskStatus},
};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

// ─── Manager tests ───

#[test]
fn manager_register_returns_task_id() {
    let mgr = BackgroundManager::new(4);
    let id = mgr
        .register("search codebase".into(), "sonnet".into())
        .unwrap();
    assert_eq!(id, "bg_1");
}

#[test]
fn manager_get_returns_task() {
    let mgr = BackgroundManager::new(4);
    let id = mgr
        .register("search codebase".into(), "sonnet".into())
        .unwrap();
    let task = mgr.get(&id).unwrap();
    assert_eq!(task.id, "bg_1");
    assert_eq!(task.description, "search codebase");
    assert_eq!(task.model, "sonnet");
    assert_eq!(task.status, BackgroundTaskStatus::Pending);
}

#[test]
fn manager_get_nonexistent_returns_none() {
    let mgr = BackgroundManager::new(4);
    assert!(mgr.get("bg_999").is_none());
}

#[test]
fn manager_update_status() {
    let mgr = BackgroundManager::new(4);
    let id = mgr.register("task".into(), "sonnet".into()).unwrap();
    mgr.update_status(&id, BackgroundTaskStatus::Running)
        .unwrap();
    let task = mgr.get(&id).unwrap();
    assert_eq!(task.status, BackgroundTaskStatus::Running);
}

#[test]
fn manager_complete_sets_result_and_time() {
    let mgr = BackgroundManager::new(4);
    let id = mgr.register("task".into(), "sonnet".into()).unwrap();
    mgr.complete(&id, "done".into()).unwrap();
    let task = mgr.get(&id).unwrap();
    assert_eq!(task.status, BackgroundTaskStatus::Completed);
    assert_eq!(task.result, Some("done".into()));
    assert!(task.completed_at.is_some());
}

#[test]
fn manager_fail_sets_error() {
    let mgr = BackgroundManager::new(4);
    let id = mgr.register("task".into(), "sonnet".into()).unwrap();
    mgr.fail(&id, "timeout".into()).unwrap();
    let task = mgr.get(&id).unwrap();
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert_eq!(task.error, Some("timeout".into()));
    assert!(task.completed_at.is_some());
}

#[test]
fn manager_cancel_changes_status() {
    let mgr = BackgroundManager::new(4);
    let id = mgr.register("task".into(), "sonnet".into()).unwrap();
    mgr.cancel(&id).unwrap();
    let task = mgr.get(&id).unwrap();
    assert_eq!(task.status, BackgroundTaskStatus::Cancelled);
}

#[test]
fn manager_cancel_completed_fails() {
    let mgr = BackgroundManager::new(4);
    let id = mgr.register("task".into(), "sonnet".into()).unwrap();
    mgr.complete(&id, "ok".into()).unwrap();
    let err = mgr.cancel(&id).unwrap_err();
    assert!(err.contains("Completed"));
}

#[test]
fn manager_list_returns_all() {
    let mgr = BackgroundManager::new(4);
    mgr.register("task1".into(), "sonnet".into()).unwrap();
    mgr.register("task2".into(), "opus".into()).unwrap();
    let tasks = mgr.list();
    assert_eq!(tasks.len(), 2);
}

// ─── Concurrency limiter tests ───

#[test]
fn concurrency_limiter_default_limit() {
    let limiter = ConcurrencyLimiter::new(2);
    assert!(limiter.try_acquire("sonnet"));
    assert!(limiter.try_acquire("sonnet"));
    assert!(!limiter.try_acquire("sonnet"));
}

#[test]
fn concurrency_limiter_custom_limit() {
    let limiter = ConcurrencyLimiter::new(1);
    limiter.set_limit("sonnet".into(), 3);
    assert!(limiter.try_acquire("sonnet"));
    assert!(limiter.try_acquire("sonnet"));
    assert!(limiter.try_acquire("sonnet"));
    assert!(!limiter.try_acquire("sonnet"));
}

#[test]
fn concurrency_limiter_release_decrements() {
    let limiter = ConcurrencyLimiter::new(1);
    assert!(limiter.try_acquire("sonnet"));
    assert!(!limiter.try_acquire("sonnet"));
    limiter.release("sonnet");
    assert!(limiter.try_acquire("sonnet"));
}

#[test]
fn concurrency_limiter_per_model_isolation() {
    let limiter = ConcurrencyLimiter::new(1);
    assert!(limiter.try_acquire("sonnet"));
    assert!(limiter.try_acquire("opus"));
    assert_eq!(limiter.active_count("sonnet"), 1);
    assert_eq!(limiter.active_count("opus"), 1);
}

// ─── Cleanup tests ───

fn make_task(status: BackgroundTaskStatus, completed_at: Option<SystemTime>) -> BackgroundTask {
    BackgroundTask {
        id: "bg_test".into(),
        description: "test".into(),
        session_id: "s1".into(),
        status,
        created_at: SystemTime::now(),
        completed_at,
        result: None,
        error: None,
        model: "sonnet".into(),
    }
}

#[test]
fn cleanup_expired_removes_old_tasks() {
    let mut tasks = HashMap::new();
    let old_time = SystemTime::now()
        .checked_sub(Duration::from_secs(3600))
        .unwrap();
    tasks.insert(
        "bg_1".into(),
        make_task(BackgroundTaskStatus::Completed, Some(old_time)),
    );
    let removed = cleanup_expired(&mut tasks, Duration::from_secs(60));
    assert_eq!(removed, 1);
    assert!(tasks.is_empty());
}

#[test]
fn cleanup_keeps_recent_tasks() {
    let mut tasks = HashMap::new();
    tasks.insert(
        "bg_1".into(),
        make_task(BackgroundTaskStatus::Completed, Some(SystemTime::now())),
    );
    let removed = cleanup_expired(&mut tasks, Duration::from_secs(60));
    assert_eq!(removed, 0);
    assert_eq!(tasks.len(), 1);
}

#[test]
fn cleanup_keeps_incomplete_tasks() {
    let mut tasks = HashMap::new();
    tasks.insert(
        "bg_1".into(),
        make_task(BackgroundTaskStatus::Running, None),
    );
    tasks.insert(
        "bg_2".into(),
        make_task(BackgroundTaskStatus::Pending, None),
    );
    let removed = cleanup_expired(&mut tasks, Duration::from_secs(60));
    assert_eq!(removed, 0);
    assert_eq!(tasks.len(), 2);
}

#[test]
fn status_counts_by_status() {
    use orchestration::background::cleanup::status_counts;
    let mut tasks = HashMap::new();
    tasks.insert(
        "bg_1".into(),
        make_task(BackgroundTaskStatus::Completed, Some(SystemTime::now())),
    );
    tasks.insert(
        "bg_2".into(),
        make_task(BackgroundTaskStatus::Completed, Some(SystemTime::now())),
    );
    tasks.insert(
        "bg_3".into(),
        make_task(BackgroundTaskStatus::Pending, None),
    );
    let counts = status_counts(&tasks);
    assert_eq!(*counts.get("Completed").unwrap(), 2);
    assert_eq!(*counts.get("Pending").unwrap(), 1);
}
