use std::path::Path;

use serde::{Deserialize, Serialize};

/// Result of a session revert operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertResult {
    /// Number of messages removed from the session.
    pub messages_removed: usize,
    /// Number of messages remaining.
    pub messages_remaining: usize,
    /// Whether git rollback was performed.
    pub git_rollback_performed: bool,
    /// Files affected by the rollback.
    pub affected_files: Vec<String>,
}

/// Revert a session's messages to a specific message index.
///
/// This removes all messages after the given index from the session's message list.
/// The caller is responsible for persisting the updated session.
pub fn revert_messages(
    messages: &mut Vec<crate::session::ConversationMessage>,
    target_index: usize,
) -> Result<RevertResult, String> {
    if target_index >= messages.len() {
        return Err(format!(
            "target index {} is out of bounds ({} messages)",
            target_index,
            messages.len()
        ));
    }

    let removed_count = messages.len() - 1 - target_index;
    messages.truncate(target_index + 1);

    Ok(RevertResult {
        messages_removed: removed_count,
        messages_remaining: messages.len(),
        git_rollback_performed: false,
        affected_files: Vec::new(),
    })
}

/// Rollback git-tracked files to a specific commit or state.
///
/// This runs `git checkout <ref> -- <paths>` to restore files.
pub fn git_rollback(
    repo_path: &Path,
    ref_spec: &str,
    paths: Option<&[String]>,
) -> Result<RevertResult, String> {
    // Verify the repo path exists and is a git repo
    if !repo_path.join(".git").exists() {
        return Err(format!("not a git repository: {}", repo_path.display()));
    }

    // Determine which files to rollback
    let affected = if let Some(p) = paths {
        p.to_vec()
    } else {
        // Get list of modified files since ref
        get_modified_files(repo_path, ref_spec)?
    };

    // Run git checkout for each file
    for file in &affected {
        run_git_checkout(repo_path, ref_spec, file)?;
    }

    Ok(RevertResult {
        messages_removed: 0,
        messages_remaining: 0,
        git_rollback_performed: true,
        affected_files: affected,
    })
}

/// Get list of files modified since a given ref.
fn get_modified_files(repo_path: &Path, ref_spec: &str) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", ref_spec])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git diff failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();

    Ok(files)
}

/// Run git checkout for a single file.
fn run_git_checkout(repo_path: &Path, ref_spec: &str, file: &str) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args(["checkout", ref_spec, "--", file])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git checkout failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git checkout {} -- {} failed: {}",
            ref_spec,
            file,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Create a git snapshot (commit) of the current state for later rollback.
/// Returns the commit hash.
pub fn create_snapshot(repo_path: &Path, message: &str) -> Result<String, String> {
    if !repo_path.join(".git").exists() {
        return Err(format!("not a git repository: {}", repo_path.display()));
    }

    // Add all changes
    let output = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git add failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Commit
    let output = std::process::Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git commit failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Get the commit hash
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git rev-parse failed: {e}"))?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_messages_removes_tail_messages() {
        use crate::session::ConversationMessage;

        let mut messages: Vec<ConversationMessage> = (0..10)
            .map(|i| ConversationMessage::user_text(format!("message {}", i)))
            .collect();

        let result = revert_messages(&mut messages, 4).expect("should succeed");

        assert_eq!(result.messages_removed, 5);
        assert_eq!(result.messages_remaining, 5);
        assert_eq!(messages.len(), 5);
    }

    #[test]
    fn revert_messages_rejects_out_of_bounds() {
        use crate::session::ConversationMessage;

        let mut messages: Vec<ConversationMessage> = (0..3)
            .map(|i| ConversationMessage::user_text(format!("msg {}", i)))
            .collect();

        let err = revert_messages(&mut messages, 10).expect_err("should fail");
        assert!(err.contains("out of bounds"));
    }

    #[test]
    fn revert_to_last_message_removes_nothing() {
        use crate::session::ConversationMessage;

        let mut messages: Vec<ConversationMessage> = (0..5)
            .map(|i| ConversationMessage::user_text(format!("msg {}", i)))
            .collect();

        let result = revert_messages(&mut messages, 4).expect("should succeed");
        assert_eq!(result.messages_removed, 0);
        assert_eq!(result.messages_remaining, 5);
    }

    #[test]
    fn git_rollback_rejects_non_git_repo() {
        let result = git_rollback(Path::new("/tmp"), "HEAD", None);
        assert!(result.is_err());
    }
}
