use planning::boulder;
use planning::notepad;

/// Handle `/handoff` command.
///
/// Creates a structured context summary for continuing work in a new session.
pub fn handle_handoff(directory: &str) -> Result<String, String> {
    let Some(state) = boulder::read_boulder_state(directory) else {
        return Ok(
            "Handoff\n  No active plan found. Start work with `/start-work` first.".to_string(),
        );
    };

    let progress = planning::plan_parser::get_plan_progress(&state.active_plan);
    let pct = if progress.total > 0 {
        (progress.completed as f64 / progress.total as f64) * 100.0
    } else {
        0.0
    };

    let notepad = notepad::read_notepad(directory, &state.plan_name);

    let mut sections = vec![
        "Handoff".to_string(),
        format!("  Plan:          {}", state.plan_name),
        format!(
            "  Progress:      {}/{} ({pct:.0}%)",
            progress.completed, progress.total
        ),
        format!("  Sessions:      {}", state.session_ids.join(", ")),
    ];

    if let Some(agent) = &state.agent {
        sections.push(format!("  Agent:         {agent}"));
    }

    if let Some(ref np) = notepad {
        let has_learnings = !np.learnings.trim().is_empty();
        let has_decisions = !np.decisions.trim().is_empty();
        let has_issues = !np.issues.trim().is_empty();

        if has_learnings || has_decisions || has_issues {
            sections.push(String::new());
            if has_learnings {
                sections.push("## Learnings".to_string());
                sections.push(np.learnings.trim().to_string());
            }
            if has_decisions {
                sections.push(String::new());
                sections.push("## Decisions".to_string());
                sections.push(np.decisions.trim().to_string());
            }
            if has_issues {
                sections.push(String::new());
                sections.push("## Open Issues".to_string());
                sections.push(np.issues.trim().to_string());
            }
        }
    }

    Ok(sections.join("\n"))
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
        let path = std::env::temp_dir().join(format!("handoff-{label}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn handoff_reports_no_active_plan() {
        let dir = temp_dir("no-plan");
        let result = handle_handoff(&dir);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No active plan"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn handoff_includes_plan_progress() {
        let dir = temp_dir("with-plan");
        let plan_file = format!("{dir}/.sisyphus/plans/test.md");
        fs::create_dir_all(format!("{dir}/.sisyphus/plans")).unwrap();
        fs::write(&plan_file, "- [x] 1. Done\n- [ ] 2. Pending\n").unwrap();

        let state = boulder::create_boulder_state(&plan_file, "s1", None, None);
        let _ = boulder::write_boulder_state(&dir, &state);

        let result = handle_handoff(&dir);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.contains("test"));
        assert!(msg.contains("1/2"));
        let _ = fs::remove_dir_all(dir);
    }
}
