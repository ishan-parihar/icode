use planning::boulder;
use planning::plan_parser;

/// Handle `/start-work` command.
///
/// If a boulder state exists, reports resume status with plan progress.
/// Otherwise, finds the most recent plan and initializes a new boulder.
pub fn handle_start_work(directory: &str, session_id: &str) -> Result<String, String> {
    if let Some(state) = boulder::read_boulder_state(directory) {
        let progress = plan_parser::get_plan_progress(&state.active_plan);
        let plan_name = &state.plan_name;
        let pct = if progress.total > 0 {
            (progress.completed as f64 / progress.total as f64) * 100.0
        } else {
            0.0
        };

        let current_task = plan_parser::read_current_top_level_task(&state.active_plan)
            .map(|t| format!("\n  Next task:     {}. {}", t.label, t.title))
            .unwrap_or_default();

        Ok(format!(
            "Start Work (resume)\n  Plan:          {plan_name}\n  Progress:      {}/{} ({pct:.0}%){current_task}\n  Session:       {session_id}\n  Started:       {}",
            progress.completed, progress.total, state.started_at
        ))
    } else {
        let plans = plan_parser::find_prometheus_plans(directory);
        let Some(latest_plan) = plans.first() else {
            return Err(format!(
                "No plans found in {directory}/.sisyphus/plans/. Create a plan first."
            ));
        };

        let plan_name = plan_parser::get_plan_name(latest_plan);
        let _state = boulder::create_boulder_state(latest_plan, session_id, None, None);
        if !boulder::write_boulder_state(directory, &_state) {
            return Err("Failed to write initial boulder state.".to_string());
        }

        let notepad_ok = planning::notepad::create_notepad(directory, &plan_name);
        let notepad_note = if notepad_ok {
            String::new()
        } else {
            "\n  Note:        Notepad creation failed."
        };

        Ok(format!(
            "Start Work (initialized)\n  Plan:          {plan_name}\n  Path:          {latest_plan}\n  Session:       {session_id}\n  Ready to begin.{notepad_note}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("start-work-{label}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn start_work_initializes_from_plan() {
        let dir = temp_dir("init");
        let plans_dir = format!("{dir}/.sisyphus/plans");
        fs::create_dir_all(&plans_dir).unwrap();
        fs::write(
            format!("{plans_dir}/test-plan.md"),
            "# TODO\n\n- [ ] 1. First task\n- [ ] 2. Second task\n",
        )
        .unwrap();

        let result = handle_start_work(&dir, "session-1");
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.contains("test-plan"));
        assert!(msg.contains("initialized"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn start_work_resumes_existing() {
        let dir = temp_dir("resume");
        let plans_dir = format!("{dir}/.sisyphus/plans");
        let boulder_path = format!("{dir}/.sisyphus/boulder.json");
        fs::create_dir_all(&plans_dir).unwrap();
        let plan_file = format!("{plans_dir}/plan.md");
        fs::write(&plan_file, "# TODO\n\n- [x] 1. Done\n- [ ] 2. Pending\n").unwrap();

        fs::write(
            boulder_path,
            r#"{"active_plan":"PLAN_PATH","started_at":"1234567890","session_ids":["s1"],"session_origins":{"s1":"direct"},"plan_name":"test","agent":null,"worktree_path":null,"task_sessions":{}}"#,
        )
        .unwrap();

        let state = boulder::create_boulder_state(&plan_file, "s1", None, None);
        let _ = boulder::write_boulder_state(&dir, &state);

        let result = handle_start_work(&dir, "s2");
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.contains("resume"));
        assert!(msg.contains("plan"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn start_work_errors_without_plans() {
        let dir = temp_dir("no-plans");
        let result = handle_start_work(&dir, "s1");
        assert!(result.is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
