use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::session::Session;

/// A snapshot of the git state at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSnapshot {
    /// Milliseconds since `UNIX_EPOCH` when this snapshot was created.
    pub timestamp_ms: u64,
    /// ID of the message that triggered the snapshot.
    pub message_id: String,
    /// Git commit hash (if snapshot was a commit — currently unused; stash only).
    pub commit_hash: Option<String>,
    /// Git stash index at capture time (if snapshot was a stash).
    pub stash_index: Option<i32>,
}

/// Result of a successful revert operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertResult {
    /// Number of messages that were removed from the session.
    pub reverted_message_count: usize,
    /// Whether file state was restored from a stash.
    pub files_restored: bool,
}

/// Errors that can occur during session revert operations.
#[derive(Debug)]
pub enum RevertError {
    /// No snapshots available to revert.
    NoSnapshots,
    /// A git command failed with the given error message.
    GitError(String),
    /// An IO error occurred.
    IoError(io::Error),
    /// A stash pop conflict occurred with the given details.
    StashPopConflict(String),
}

impl fmt::Display for RevertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSnapshots => write!(f, "No snapshots available to revert"),
            Self::GitError(msg) => write!(f, "Git error: {msg}"),
            Self::IoError(err) => write!(f, "IO error: {err}"),
            Self::StashPopConflict(details) => write!(f, "Stash pop conflict: {details}"),
        }
    }
}

impl std::error::Error for RevertError {}

impl From<io::Error> for RevertError {
    fn from(err: io::Error) -> Self {
        Self::IoError(err)
    }
}

/// Manages git snapshots for session-level revert functionality.
///
/// Captures the working tree state before file-editing turns and
/// can restore both file state and conversation history.
pub struct SessionReverter {
    snapshots: VecDeque<GitSnapshot>,
    workspace_root: PathBuf,
}

impl SessionReverter {
    /// Create a new `SessionReverter` rooted at the given workspace path.
    #[must_use]
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            snapshots: VecDeque::new(),
            workspace_root,
        }
    }

    /// Return the workspace root this reverter operates on.
    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Return the number of snapshots currently stored.
    #[must_use]
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Capture a git snapshot of the current working tree state.
    ///
    /// If there are uncommitted changes, creates a git stash with a message
    /// identifying the triggering message. If no changes exist, records an
    /// empty snapshot for message-boundary tracking.
    pub fn capture(&mut self, message_id: &str) -> Result<(), RevertError> {
        let has_changes = self.has_uncommitted_changes()?;
        let stash_index = if has_changes {
            Some(self.create_stash(message_id)?)
        } else {
            None
        };

        let snapshot = GitSnapshot {
            timestamp_ms: current_time_millis(),
            message_id: message_id.to_string(),
            commit_hash: None,
            stash_index,
        };

        self.snapshots.push_back(snapshot);
        Ok(())
    }

    /// Revert to the most recent snapshot.
    ///
    /// Pops the most recent snapshot, restores the file state if a stash
    /// was captured, and removes messages that were added after the snapshot
    /// point from the session.
    pub fn revert(&mut self, session: &mut Session) -> Result<RevertResult, RevertError> {
        let snapshot = self.snapshots.pop_back().ok_or(RevertError::NoSnapshots)?;
        let message_count_before = session.messages.len();

        let truncated = find_truncation_point(session, &snapshot.message_id);
        let reverted_count = message_count_before.saturating_sub(truncated);
        session.messages.truncate(truncated);

        let files_restored = if let Some(index) = snapshot.stash_index {
            self.pop_stash(index)?;
            true
        } else {
            false
        };

        Ok(RevertResult {
            reverted_message_count: reverted_count,
            files_restored,
        })
    }

    /// Check if there are uncommitted changes in the workspace.
    fn has_uncommitted_changes(&self) -> Result<bool, RevertError> {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.workspace_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RevertError::GitError(format!(
                "git status failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.trim().is_empty())
    }

    /// Create a git stash with the given message. Returns the stash index
    /// by looking it up in the stash list (not assumed to be 0).
    fn create_stash(&self, message_id: &str) -> Result<i32, RevertError> {
        let stash_message = format!("iCode snapshot: {message_id}");

        let output = Command::new("git")
            .args(["stash", "push", "-m", &stash_message])
            .current_dir(&self.workspace_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RevertError::GitError(format!(
                "git stash push failed: {stderr}"
            )));
        }

        // Look up the stash index by message, not by assumed position.
        self.find_stash_index(message_id)
            .ok_or_else(|| RevertError::GitError("Created stash but could not find it".into()))
    }

    /// Find a stash index by matching its message in the stash list.
    fn find_stash_index(&self, message_id: &str) -> Option<i32> {
        let stash_message = format!("iCode snapshot: {message_id}");
        let output = Command::new("git")
            .args(["stash", "list", "--format=%gd:%s"])
            .current_dir(&self.workspace_root)
            .output()
            .ok()?;
        let list = String::from_utf8_lossy(&output.stdout);
        for (idx, line) in list.lines().enumerate() {
            if line.contains(&stash_message) {
                return Some(idx as i32);
            }
        }
        None
    }

    /// Pop a specific stash by index.
    fn pop_stash(&self, index: i32) -> Result<(), RevertError> {
        let stash_ref = format!("stash@{{{index}}}");

        let output = Command::new("git")
            .args(["stash", "pop", &stash_ref])
            .current_dir(&self.workspace_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_str = stderr.to_string();

            // Detect conflict errors
            if stderr_str.contains("conflict") || stderr_str.contains("CONFLICT") {
                return Err(RevertError::StashPopConflict(stderr_str));
            }

            return Err(RevertError::GitError(format!(
                "git stash pop failed: {stderr_str}"
            )));
        }

        Ok(())
    }
}

fn find_truncation_point(session: &Session, message_id: &str) -> usize {
    session
        .messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, msg)| {
            msg.blocks.iter().any(|block| {
                if let crate::session::ContentBlock::Text { text } = block {
                    text.trim() == message_id.trim()
                } else {
                    false
                }
            })
        })
        .map_or(session.messages.len(), |(idx, _)| idx + 1)
}

/// Walk up the directory tree from `path` looking for a `.git` directory.
///
/// Returns the path to the git root if found, `None` otherwise.
#[must_use]
pub fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.canonicalize().ok()?;

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }

        if !current.pop() {
            return None;
        }
    }
}

fn current_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{find_git_root, find_truncation_point, RevertError, SessionReverter};
    use crate::session::{ContentBlock, ConversationMessage, Session};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("revert-test-{nanos}"))
    }

    fn init_git_repo(path: &PathBuf) {
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init should succeed");
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .expect("git config email should succeed");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .expect("git config name should succeed");
    }

    fn commit_all(path: &PathBuf) {
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .expect("git add should succeed");
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .expect("git commit should succeed");
    }

    #[test]
    #[cfg_attr(not(feature = "revert-tests"), ignore = "requires git worktree setup")]
    fn capture_snapshot_with_uncommitted_changes() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        init_git_repo(&dir);

        // Create and commit a file
        fs::write(dir.join("test.txt"), "initial\n").expect("file write should succeed");
        commit_all(&dir);

        // Make an uncommitted change
        fs::write(dir.join("test.txt"), "modified\n").expect("file write should succeed");

        let mut reverter = SessionReverter::new(dir.clone());
        reverter
            .capture("msg-1")
            .expect("capture with changes should succeed");

        assert_eq!(reverter.snapshot_count(), 1);

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "revert-tests"), ignore = "requires git worktree setup")]
    fn revert_restores_stash() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        init_git_repo(&dir);

        // Create and commit a file
        let original_content = "hello world\n";
        fs::write(dir.join("test.txt"), original_content).expect("file write should succeed");
        commit_all(&dir);

        // Make an uncommitted change
        let modified_content = "modified content\n";
        fs::write(dir.join("test.txt"), modified_content).expect("file write should succeed");

        let mut reverter = SessionReverter::new(dir.clone());
        reverter.capture("msg-1").expect("capture should succeed");

        // Verify file is back to original (stashed)
        let content_after_capture =
            fs::read_to_string(dir.join("test.txt")).expect("file should be readable");
        assert_eq!(
            content_after_capture, original_content,
            "file should be restored to original after stash capture"
        );

        let mut session = Session::new();
        let result = reverter
            .revert(&mut session)
            .expect("revert should succeed");

        assert!(result.files_restored);

        // Verify file is back to modified (stash popped)
        let content_after_revert =
            fs::read_to_string(dir.join("test.txt")).expect("file should be readable");
        assert_eq!(
            content_after_revert, modified_content,
            "file should be restored to modified state after stash pop"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "revert-tests"), ignore = "requires git worktree setup")]
    fn revert_removes_messages() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        init_git_repo(&dir);

        // Create and commit a file so there are no uncommitted changes
        fs::write(dir.join("test.txt"), "initial\n").expect("file write should succeed");
        commit_all(&dir);

        let mut session = Session::new();
        session
            .push_user_text("first message")
            .expect("push should succeed");
        session
            .push_message(ConversationMessage::assistant(vec![ContentBlock::Text {
                text: "first response".to_string(),
            }]))
            .expect("push should succeed");

        let mut reverter = SessionReverter::new(dir.clone());
        reverter
            .capture("first message")
            .expect("capture should succeed");

        // Add more messages after the snapshot
        session
            .push_user_text("second message")
            .expect("push should succeed");
        session
            .push_message(ConversationMessage::assistant(vec![ContentBlock::Text {
                text: "second response".to_string(),
            }]))
            .expect("push should succeed");

        assert_eq!(session.messages.len(), 4);

        let result = reverter
            .revert(&mut session)
            .expect("revert should succeed");

        // Snapshot was at "first message" (index 0), so we keep 1 message.
        // 4 - 1 = 3 removed.
        assert_eq!(result.reverted_message_count, 3);
        assert_eq!(session.messages.len(), 1);
        assert!(!result.files_restored);

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "revert-tests"), ignore = "requires git worktree setup")]
    fn revert_with_no_snapshots_returns_error() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        init_git_repo(&dir);
        fs::write(dir.join("test.txt"), "initial\n").expect("file write should succeed");
        commit_all(&dir);

        let mut reverter = SessionReverter::new(dir.clone());
        let mut session = Session::new();

        let result = reverter.revert(&mut session);
        assert!(
            matches!(result, Err(RevertError::NoSnapshots)),
            "revert with no snapshots should return NoSnapshots error"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "revert-tests"), ignore = "requires git worktree setup")]
    fn multiple_captures_then_revert_only_reverts_last() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        init_git_repo(&dir);

        // Create and commit a file
        fs::write(dir.join("test.txt"), "initial\n").expect("file write should succeed");
        commit_all(&dir);

        let mut reverter = SessionReverter::new(dir.clone());

        // First capture
        reverter
            .capture("msg-1")
            .expect("first capture should succeed");

        // Second capture
        reverter
            .capture("msg-2")
            .expect("second capture should succeed");

        assert_eq!(reverter.snapshot_count(), 2);

        let mut session = Session::new();
        let result = reverter
            .revert(&mut session)
            .expect("revert should succeed");

        // Only the last snapshot should be removed
        assert_eq!(reverter.snapshot_count(), 1);
        assert!(!result.files_restored); // no changes were stashed

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_git_root_finds_git_directory() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");
        init_git_repo(&dir);

        // Create a nested subdirectory
        let nested = dir.join("src").join("lib");
        fs::create_dir_all(&nested).expect("nested dir should be created");

        let git_root = find_git_root(&nested).expect("should find git root");
        assert_eq!(
            git_root,
            dir.canonicalize().expect("dir should be canonicalizable")
        );

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_git_root_returns_none_without_git() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).expect("temp dir should be created");

        let result = find_git_root(&dir);
        assert!(
            result.is_none(),
            "find_git_root should return None when no .git directory exists"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_truncation_point_finds_matching_message() {
        let mut session = Session::new();
        session
            .push_user_text("hello world")
            .expect("push should succeed");
        session
            .push_message(ConversationMessage::assistant(vec![ContentBlock::Text {
                text: "response".to_string(),
            }]))
            .expect("push should succeed");
        session
            .push_user_text("second message")
            .expect("push should succeed");

        let kept = find_truncation_point(&session, "hello world");
        assert_eq!(kept, 1);

        let kept = find_truncation_point(&session, "second message");
        assert_eq!(kept, 3);

        let kept = find_truncation_point(&session, "nonexistent");
        assert_eq!(kept, 3);
    }
}
