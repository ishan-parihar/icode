use std::collections::HashMap;
use std::fs;

use planning::boulder::{
    append_session_id, clear_boulder_state, create_boulder_state, get_task_session_state,
    read_boulder_state, upsert_task_session_state, write_boulder_state,
};
use planning::notepad::{create_notepad, read_notepad, write_notepad};
use planning::plan_parser::{
    find_prometheus_plans, get_plan_name, get_plan_progress, read_current_top_level_task,
};
use planning::types::{BoulderState, Notepad, SessionOrigin, TaskSection, UpsertTaskInput};

fn make_temp() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    (dir, path)
}

fn make_boulder(_dir: &str) -> BoulderState {
    BoulderState {
        active_plan: "/path/to/plan.md".to_string(),
        started_at: "2024-01-01T00:00:00Z".to_string(),
        session_ids: vec!["sess-1".to_string()],
        session_origins: HashMap::new(),
        plan_name: "plan".to_string(),
        agent: Some("atlas".to_string()),
        worktree_path: Some("/worktree".to_string()),
        task_sessions: HashMap::new(),
    }
}

#[test]
fn test_boulder_state_serialize_roundtrip() {
    let state = make_boulder("/tmp");
    let json = serde_json::to_string(&state).unwrap();
    let parsed: BoulderState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.active_plan, state.active_plan);
    assert_eq!(parsed.plan_name, state.plan_name);
    assert_eq!(parsed.session_ids, state.session_ids);
}

#[test]
fn test_boulder_state_deserialize_from_json() {
    let json = r#"{"active_plan":"/p.md","started_at":"2024-01-01","session_ids":["s1"],"session_origins":{},"plan_name":"p","agent":null,"worktree_path":null,"task_sessions":{}}"#;
    let state: BoulderState = serde_json::from_str(json).unwrap();
    assert_eq!(state.plan_name, "p");
    assert_eq!(state.agent, None);
}

#[test]
fn test_session_origin_direct_serialize() {
    let json = serde_json::to_string(&SessionOrigin::Direct).unwrap();
    assert_eq!(json, "\"direct\"");
}

#[test]
fn test_session_origin_appended_serialize() {
    let json = serde_json::to_string(&SessionOrigin::Appended).unwrap();
    assert_eq!(json, "\"appended\"");
}

#[test]
fn test_session_origin_deserialize() {
    let direct: SessionOrigin = serde_json::from_str("\"direct\"").unwrap();
    assert!(matches!(direct, SessionOrigin::Direct));

    let appended: SessionOrigin = serde_json::from_str("\"appended\"").unwrap();
    assert!(matches!(appended, SessionOrigin::Appended));
}

#[test]
fn test_read_boulder_state_missing_file() {
    let (_guard, dir) = make_temp();
    assert!(read_boulder_state(&dir).is_none());
}

#[test]
fn test_write_and_read_boulder_state() {
    let (_guard, dir) = make_temp();
    let state = make_boulder(&dir);
    assert!(write_boulder_state(&dir, &state));
    let loaded = read_boulder_state(&dir).unwrap();
    assert_eq!(loaded.active_plan, state.active_plan);
}

#[test]
fn test_append_session_id_direct() {
    let (_guard, dir) = make_temp();
    let state = make_boulder(&dir);
    let _ = write_boulder_state(&dir, &state);

    let result = append_session_id(&dir, "sess-2", SessionOrigin::Direct).unwrap();
    assert_eq!(result.session_ids.len(), 2);
    assert!(result.session_ids.contains(&"sess-2".to_string()));
    assert!(matches!(
        result.session_origins.get("sess-2"),
        Some(SessionOrigin::Direct)
    ));
}

#[test]
fn test_append_session_id_appended() {
    let (_guard, dir) = make_temp();
    let state = make_boulder(&dir);
    let _ = write_boulder_state(&dir, &state);

    let result = append_session_id(&dir, "sess-3", SessionOrigin::Appended).unwrap();
    assert!(matches!(
        result.session_origins.get("sess-3"),
        Some(SessionOrigin::Appended)
    ));
}

#[test]
fn test_clear_boulder_state() {
    let (_guard, dir) = make_temp();
    let state = make_boulder(&dir);
    let _ = write_boulder_state(&dir, &state);
    assert!(clear_boulder_state(&dir));
    assert!(read_boulder_state(&dir).is_none());
}

#[test]
fn test_create_boulder_state_factory() {
    let state = create_boulder_state("/plans/my-plan.md", "sess-1", Some("atlas"), Some("/wt"));
    assert_eq!(state.active_plan, "/plans/my-plan.md");
    assert_eq!(state.plan_name, "my-plan");
    assert_eq!(state.agent, Some("atlas".to_string()));
    assert_eq!(state.worktree_path, Some("/wt".to_string()));
    assert_eq!(state.session_ids, vec!["sess-1"]);
}

#[test]
fn test_task_session_state_get_empty() {
    let (_guard, dir) = make_temp();
    let state = make_boulder(&dir);
    let _ = write_boulder_state(&dir, &state);
    assert!(get_task_session_state(&dir, "todo:1").is_none());
}

#[test]
fn test_task_session_state_upsert() {
    let (_guard, dir) = make_temp();
    let state = make_boulder(&dir);
    let _ = write_boulder_state(&dir, &state);

    let input = UpsertTaskInput {
        task_key: "todo:1".to_string(),
        task_label: "1".to_string(),
        task_title: "First task".to_string(),
        session_id: "sess-1".to_string(),
        agent: Some("atlas".to_string()),
        category: None,
    };
    let result = upsert_task_session_state(&dir, input).unwrap();
    let task = result.task_sessions.get("todo:1").unwrap();
    assert_eq!(task.task_title, "First task");
    assert!(!task.updated_at.is_empty());
}

#[test]
fn test_plan_progress_unchecked() {
    let (_guard, dir) = make_temp();
    let path = format!("{dir}/plan.md");
    fs::write(&path, "- [ ] Task 1\n- [ ] Task 2\n").unwrap();
    let progress = get_plan_progress(&path);
    assert_eq!(progress.total, 2);
    assert_eq!(progress.completed, 0);
    assert!(!progress.is_complete);
}

#[test]
fn test_plan_progress_all_checked() {
    let (_guard, dir) = make_temp();
    let path = format!("{dir}/plan.md");
    fs::write(&path, "- [x] Done 1\n- [x] Done 2\n").unwrap();
    let progress = get_plan_progress(&path);
    assert_eq!(progress.total, 2);
    assert_eq!(progress.completed, 2);
    assert!(progress.is_complete);
}

#[test]
fn test_plan_progress_mixed() {
    let (_guard, dir) = make_temp();
    let path = format!("{dir}/plan.md");
    fs::write(&path, "- [x] Done\n- [ ] Pending\n").unwrap();
    let progress = get_plan_progress(&path);
    assert_eq!(progress.total, 2);
    assert_eq!(progress.completed, 1);
    assert!(!progress.is_complete);
}

#[test]
fn test_plan_progress_empty() {
    let (_guard, dir) = make_temp();
    let path = format!("{dir}/plan.md");
    fs::write(&path, "").unwrap();
    let progress = get_plan_progress(&path);
    assert_eq!(progress.total, 0);
    assert!(!progress.is_complete);
}

#[test]
fn test_get_plan_name() {
    assert_eq!(get_plan_name("/path/to/my-plan.md"), "my-plan");
    assert_eq!(get_plan_name("plan.md"), "plan");
}

#[test]
fn test_find_prometheus_plans() {
    let (_guard, dir) = make_temp();
    let plans_dir = format!("{dir}/.sisyphus/plans");
    fs::create_dir_all(&plans_dir).unwrap();
    fs::write(format!("{plans_dir}/a.md"), "").unwrap();
    fs::write(format!("{plans_dir}/b.md"), "").unwrap();

    let plans = find_prometheus_plans(&dir);
    assert_eq!(plans.len(), 2);
    assert!(plans.iter().any(|p| p.ends_with("a.md")));
    assert!(plans.iter().any(|p| p.ends_with("b.md")));
}

#[test]
fn test_find_prometheus_plans_empty_dir() {
    let (_guard, dir) = make_temp();
    let plans = find_prometheus_plans(&dir);
    assert!(plans.is_empty());
}

#[test]
fn test_read_current_top_level_task_todo() {
    let (_guard, dir) = make_temp();
    let path = format!("{dir}/plan.md");
    let content = "# TODOs\n\n- [ ] 1. Implement feature\n- [ ] 2. Add tests\n";
    fs::write(&path, content).unwrap();

    let task = read_current_top_level_task(&path).unwrap();
    assert_eq!(task.key, "todo:1");
    assert_eq!(task.label, "1");
    assert_eq!(task.title, "Implement feature");
    assert!(matches!(task.section, TaskSection::Todo));
}

#[test]
fn test_read_current_top_level_task_final_wave() {
    let (_guard, dir) = make_temp();
    let path = format!("{dir}/plan.md");
    let content = "# Final Verification Wave\n\n- [ ] F1. Verify edge cases\n";
    fs::write(&path, content).unwrap();

    let task = read_current_top_level_task(&path).unwrap();
    assert_eq!(task.key, "final-wave:F1");
    assert_eq!(task.label, "F1");
    assert_eq!(task.title, "Verify edge cases");
    assert!(matches!(task.section, TaskSection::FinalWave));
}

#[test]
fn test_read_current_top_level_task_all_done() {
    let (_guard, dir) = make_temp();
    let path = format!("{dir}/plan.md");
    let content = "# TODOs\n\n- [x] 1. Done\n";
    fs::write(&path, content).unwrap();

    assert!(read_current_top_level_task(&path).is_none());
}

#[test]
fn test_notepad_create() {
    let (_guard, dir) = make_temp();
    assert!(create_notepad(&dir, "test-plan"));
    let notepad_path = format!("{dir}/.sisyphus/notepads/test-plan");
    assert!(fs::metadata(&notepad_path).unwrap().is_dir());
    assert!(fs::read_to_string(format!("{notepad_path}/learnings.md"))
        .unwrap()
        .is_empty());
}

#[test]
fn test_notepad_read_write_roundtrip() {
    let (_guard, dir) = make_temp();
    assert!(create_notepad(&dir, "roundtrip-plan"));

    let notepad = Notepad {
        plan_name: "roundtrip-plan".to_string(),
        learnings: "Learned Rust".to_string(),
        decisions: "Chose serde".to_string(),
        issues: "None".to_string(),
        verification: "All clear".to_string(),
        problems: "NA".to_string(),
    };
    assert!(write_notepad(&dir, "roundtrip-plan", &notepad));

    let loaded = read_notepad(&dir, "roundtrip-plan").unwrap();
    assert_eq!(loaded.learnings, "Learned Rust");
    assert_eq!(loaded.decisions, "Chose serde");
    assert_eq!(loaded.plan_name, "roundtrip-plan");
}

#[test]
fn test_task_section_serialization() {
    let json = serde_json::to_string(&TaskSection::Todo).unwrap();
    assert_eq!(json, "\"todo\"");

    let json = serde_json::to_string(&TaskSection::FinalWave).unwrap();
    assert_eq!(json, "\"final_wave\"");
}
