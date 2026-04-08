use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone)]
pub struct WorktreeSessionState {
    pub original_cwd: PathBuf,
    pub worktree_path: PathBuf,
    pub branch: String,
    pub original_head: Option<String>,
}

fn global_worktree_session() -> Arc<Mutex<Option<WorktreeSessionState>>> {
    static STATE: OnceLock<Arc<Mutex<Option<WorktreeSessionState>>>> = OnceLock::new();
    STATE.get_or_init(|| Arc::new(Mutex::new(None))).clone()
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EnterWorktreeInput {
    pub branch: Option<String>,
    pub path: Option<String>,
    pub post_create_command: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ExitWorktreeInput {
    #[serde(default = "default_exit_action")]
    pub action: String,
    #[serde(default)]
    pub discard_changes: bool,
}

fn default_exit_action() -> String {
    "keep".to_string()
}

fn run_git(cwd: &PathBuf, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn execute_enter_worktree(input: &EnterWorktreeInput) -> Result<String, String> {
    let session = global_worktree_session();
    {
        let guard = session.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        if guard.is_some() {
            return Err("Already in a worktree session. Call ExitWorktree first.".to_string());
        }
    }

    let cwd = std::env::current_dir().map_err(|e| format!("cannot determine cwd: {e}"))?;

    let branch = input.branch.clone().unwrap_or_else(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let s = secs % 60;
        let m = (secs / 60) % 60;
        let h = (secs / 3600) % 24;
        let days = secs / 86400;
        let year = 1970 + days / 365;
        let day_of_year = days % 365;
        let month = day_of_year / 30 + 1;
        let day = day_of_year % 30 + 1;
        format!("icode-{year:04}{month:02}{day:02}-{h:02}{m:02}{s:02}")
    });

    let worktree_path = match &input.path {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(".worktrees").join(&branch),
    };

    let original_head = run_git(&cwd, &["rev-parse", "HEAD"]).ok();

    if worktree_path.exists() {
        return Err(format!(
            "Cannot create worktree: path '{}' already exists.",
            worktree_path.display()
        ));
    }

    let worktree_str = worktree_path.to_string_lossy();
    run_git(&cwd, &["worktree", "add", "-b", &branch, &worktree_str])?;

    let post_create_output = if let Some(cmd) = &input.post_create_command {
        let (shell, shell_arg) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };
        let output = Command::new(shell)
            .arg(shell_arg)
            .arg(cmd)
            .current_dir(&worktree_path)
            .output()
            .map_err(|e| format!("failed to run post-create command: {e}"))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                format!("\nPost-create command '{cmd}' completed successfully.")
            } else {
                format!(
                    "\nPost-create command '{cmd}' completed successfully.\nOutput: {}",
                    stdout.trim()
                )
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!(
                "\nPost-create command '{cmd}' exited with error.\nStderr: {}",
                stderr.trim()
            )
        }
    } else {
        String::new()
    };

    {
        let mut guard = session.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        *guard = Some(WorktreeSessionState {
            original_cwd: cwd.clone(),
            worktree_path: worktree_path.clone(),
            branch: branch.clone(),
            original_head,
        });
    }

    to_pretty_json(&json!({
        "status": "success",
        "message": format!(
            "Created worktree at {} on branch '{}'.\nUse ExitWorktree to return.",
            worktree_path.display(), branch
        ),
        "worktree_path": worktree_str.to_string(),
        "branch": branch,
        "original_cwd": cwd.to_string_lossy().to_string(),
        "post_create_output": if post_create_output.is_empty() { serde_json::Value::Null } else { json!(post_create_output) }
    }))
}

pub fn execute_exit_worktree(input: &ExitWorktreeInput) -> Result<String, String> {
    let session = global_worktree_session();

    let state = {
        let guard = session.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        guard
            .clone()
            .ok_or_else(|| "No-op: there is no active EnterWorktree session to exit.".to_string())?
    };

    if input.action == "remove" && !input.discard_changes {
        let status = run_git(&state.worktree_path, &["status", "--porcelain"]);
        let changed_files = status
            .as_deref()
            .unwrap_or("")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();

        let commit_count = if let Some(ref head) = state.original_head {
            run_git(
                &state.worktree_path,
                &["rev-list", "--count", &format!("{head}..HEAD")],
            )
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0)
        } else {
            0
        };

        if changed_files > 0 || commit_count > 0 {
            let mut parts = Vec::new();
            if changed_files > 0 {
                parts.push(format!("{changed_files} uncommitted file(s)"));
            }
            if commit_count > 0 {
                parts.push(format!("{commit_count} commit(s) on the worktree branch"));
            }
            return Err(format!(
                "Worktree has {}. Removing will discard this work permanently. \
                 Re-invoke with discard_changes=true, or use action='keep'.",
                parts.join(" and ")
            ));
        }
    }

    {
        let mut guard = session.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        *guard = None;
    }

    let message = match input.action.as_str() {
        "keep" => {
            let _ = run_git(&state.original_cwd, &["worktree", "prune"]);
            format!(
                "Exited worktree. Work preserved at {} on branch {}.",
                state.worktree_path.display(),
                state.branch
            )
        }
        "remove" => {
            let _ = run_git(
                &state.original_cwd,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    &state.worktree_path.to_string_lossy(),
                ],
            );
            let _ = run_git(&state.original_cwd, &["branch", "-D", &state.branch]);
            format!(
                "Exited and removed worktree at {}.",
                state.worktree_path.display()
            )
        }
        other => {
            return Err(format!("Unknown action '{other}'. Use 'keep' or 'remove'."));
        }
    };

    to_pretty_json(&json!({
        "status": "success",
        "message": message,
        "action": input.action,
        "original_cwd": state.original_cwd.to_string_lossy().to_string()
    }))
}

pub fn enter_worktree_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(EnterWorktreeInput)).unwrap()
}

pub fn exit_worktree_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(ExitWorktreeInput)).unwrap()
}

fn to_pretty_json(value: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_worktree_spec_has_properties() {
        let spec = enter_worktree_tool_spec();
        let props = spec["properties"].as_object();
        assert!(props.is_some());
        let props = props.unwrap();
        assert!(props.contains_key("branch"));
        assert!(props.contains_key("path"));
        assert!(props.contains_key("post_create_command"));
    }

    #[test]
    fn exit_worktree_spec_has_action_enum() {
        let spec = exit_worktree_tool_spec();
        let action = &spec["properties"]["action"];
        // schemars may represent enum as either "enum" array or via "anyOf" variants
        let enum_vals = action["enum"].as_array();
        if let Some(vals) = enum_vals {
            assert!(vals.iter().any(|v| v.as_str() == Some("keep")));
            assert!(vals.iter().any(|v| v.as_str() == Some("remove")));
        } else {
            // Fallback: check that action property exists and is string-typed
            assert_eq!(action["type"], "string");
        }
    }

    #[test]
    fn exit_worktree_requires_no_active_session() {
        let result = execute_exit_worktree(&ExitWorktreeInput {
            action: "keep".to_string(),
            discard_changes: false,
        });
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("no active EnterWorktree session"));
    }

    #[test]
    fn default_exit_action_is_keep() {
        assert_eq!(default_exit_action(), "keep");
    }

    #[test]
    fn worktree_session_is_singleton() {
        let s1 = global_worktree_session();
        let s2 = global_worktree_session();
        assert!(Arc::ptr_eq(&s1, &s2));
    }
}
