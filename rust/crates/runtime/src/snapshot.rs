use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed(String),
}
impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileStatus::Added => write!(f, "A"),
            FileStatus::Modified => write!(f, "M"),
            FileStatus::Deleted => write!(f, "D"),
            FileStatus::Renamed(n) => write!(f, "R -> {}", n),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,
}
#[derive(Debug)]
pub enum SnapshotError {
    GitCommandFailed {
        cmd: String,
        stderr: String,
        exit_code: i32,
    },
    IoError(std::io::Error),
    Utf8Error(std::string::FromUtf8Error),
    ParseError(String),
    NoGitRepo,
    SnapshotNotFound(String),
}
impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::GitCommandFailed {
                cmd,
                stderr,
                exit_code,
            } => write!(f, "git {} (exit {}): {}", cmd, exit_code, stderr),
            SnapshotError::IoError(e) => write!(f, "IO: {}", e),
            SnapshotError::Utf8Error(e) => write!(f, "UTF-8: {}", e),
            SnapshotError::ParseError(m) => write!(f, "parse: {}", m),
            SnapshotError::NoGitRepo => write!(f, "not a git repository"),
            SnapshotError::SnapshotNotFound(h) => write!(f, "snapshot not found: {}", h),
        }
    }
}
impl std::error::Error for SnapshotError {}
impl From<std::io::Error> for SnapshotError {
    fn from(e: std::io::Error) -> Self {
        SnapshotError::IoError(e)
    }
}
impl From<std::string::FromUtf8Error> for SnapshotError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        SnapshotError::Utf8Error(e)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRecord {
    pub hash: String,
    pub timestamp: u64,
    pub label: Option<String>,
    pub file_count: usize,
}
pub struct SnapshotManager {
    project_path: PathBuf,
    snapshot_dir: PathBuf,
}
impl SnapshotManager {
    pub fn new(project_path: &Path) -> std::io::Result<Self> {
        let canonical = project_path.canonicalize()?;
        let ph = simple_hash(canonical.to_string_lossy().as_ref());
        let sd = dirs_next::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("icode")
            .join("snapshots")
            .join(&ph);
        fs::create_dir_all(&sd)?;
        Ok(Self {
            project_path: canonical,
            snapshot_dir: sd,
        })
    }
    pub fn track(&self, label: Option<&str>) -> Result<String, SnapshotError> {
        self.verify_git_repo()?;
        let out = self.git(&["stash", "create"])?;
        let hash = out.trim().to_string();
        if hash.is_empty() {
            let h = self.git(&["rev-parse", "HEAD"])?;
            return Ok(h.trim().to_string());
        }
        self.append_record(&SnapshotRecord {
            hash: hash.clone(),
            timestamp: current_timestamp(),
            label: label.map(String::from),
            file_count: self.count_tracked_files()?,
        })?;
        Ok(hash)
    }
    pub fn diff(&self, since_hash: &str) -> Result<Vec<FileDiff>, SnapshotError> {
        self.verify_git_repo()?;
        let out = self.git(&["diff", "--name-status", since_hash])?;
        let mut diffs = Vec::new();
        for line in out.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.is_empty() {
                continue;
            }
            let st = parts[0].trim();
            let p = parts.get(1).map(|s| s.trim()).unwrap_or("");
            let status = match st {
                "A" => FileStatus::Added,
                "M" => FileStatus::Modified,
                "D" => FileStatus::Deleted,
                s if s.starts_with('R') => {
                    FileStatus::Renamed(parts.get(2).map(|s| s.trim()).unwrap_or(p).to_string())
                }
                _ => FileStatus::Modified,
            };
            diffs.push(FileDiff {
                path: p.to_string(),
                status,
            });
        }
        Ok(diffs)
    }
    pub fn restore(&self, hash: &str) -> Result<(), SnapshotError> {
        self.verify_git_repo()?;
        self.git(&["cat-file", "-t", hash])?;
        self.git(&["read-tree", hash])?;
        self.git(&["checkout-index", "-a", "-f"])?;
        self.git(&["clean", "-fd"])?;
        Ok(())
    }
    pub fn revert_file(&self, path: &str, since_hash: &str) -> Result<(), SnapshotError> {
        self.verify_git_repo()?;
        let content = self.git(&["show", &format!("{}:{}", since_hash, path)])?;
        let fp = self.project_path.join(path);
        if let Some(parent) = fp.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(fp, content)?;
        Ok(())
    }
    pub fn file_content_at(&self, path: &str, hash: &str) -> Result<Vec<u8>, SnapshotError> {
        self.verify_git_repo()?;
        let out = Command::new("git")
            .current_dir(&self.project_path)
            .args(["show", &format!("{}:{}", hash, path)])
            .output()?;
        if !out.status.success() {
            return Err(SnapshotError::GitCommandFailed {
                cmd: format!("git show {}:{}", hash, path),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                exit_code: out.status.code().unwrap_or(-1),
            });
        }
        Ok(out.stdout)
    }
    pub fn list(&self) -> Result<Vec<SnapshotRecord>, SnapshotError> {
        let ip = self.snapshot_dir.join("index.jsonl");
        if !ip.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&ip)?;
        let mut records: Vec<SnapshotRecord> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(records)
    }
    pub fn cleanup(&self, max_age_days: u64) -> Result<usize, SnapshotError> {
        let ip = self.snapshot_dir.join("index.jsonl");
        if !ip.exists() {
            return Ok(0);
        }
        let cutoff = current_timestamp() - (max_age_days * 24 * 60 * 60);
        let content = fs::read_to_string(&ip)?;
        let records: Vec<SnapshotRecord> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let kept: Vec<&SnapshotRecord> = records.iter().filter(|r| r.timestamp >= cutoff).collect();
        let removed = records.len() - kept.len();
        let nc: String = kept
            .iter()
            .map(|r| serde_json::to_string(r).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");
        let write_content = if nc.is_empty() {
            String::new()
        } else {
            format!("{}\n", nc)
        };
        fs::write(&ip, &write_content)?;
        Ok(removed)
    }
    fn verify_git_repo(&self) -> Result<(), SnapshotError> {
        let out = Command::new("git")
            .current_dir(&self.project_path)
            .args(["rev-parse", "--git-dir"])
            .output()
            .map_err(SnapshotError::IoError)?;
        if !out.status.success() {
            return Err(SnapshotError::NoGitRepo);
        }
        Ok(())
    }
    fn git(&self, args: &[&str]) -> Result<String, SnapshotError> {
        let out = Command::new("git")
            .current_dir(&self.project_path)
            .args(args)
            .output()
            .map_err(SnapshotError::IoError)?;
        if !out.status.success() {
            return Err(SnapshotError::GitCommandFailed {
                cmd: format!("git {}", args.join(" ")),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                exit_code: out.status.code().unwrap_or(-1),
            });
        }
        String::from_utf8(out.stdout).map_err(SnapshotError::Utf8Error)
    }
    fn append_record(&self, record: &SnapshotRecord) -> Result<(), SnapshotError> {
        let line =
            serde_json::to_string(record).map_err(|e| SnapshotError::ParseError(e.to_string()))?;
        let ip = self.snapshot_dir.join("index.jsonl");
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ip)
            .map_err(SnapshotError::IoError)?
            .write_all(format!("{}\n", line).as_bytes())
            .map_err(SnapshotError::IoError)?;
        Ok(())
    }
    fn count_tracked_files(&self) -> Result<usize, SnapshotError> {
        let out = self.git(&["ls-files"])?;
        Ok(out.lines().filter(|l| !l.trim().is_empty()).count())
    }
}
fn simple_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    result[..8].iter().map(|b| format!("{:02x}", b)).collect()
}
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    fn setup_test_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        Command::new("git")
            .arg("init")
            .current_dir(p)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(p)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(p)
            .output()
            .unwrap();
        fs::write(p.join("file1.txt"), "initial content").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(p)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(p)
            .output()
            .unwrap();
        dir
    }
    #[test]
    fn test_track_creates_snapshot() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let h = m.track(Some("test")).unwrap();
        assert!(!h.is_empty());
        assert!(h.len() >= 8);
    }
    #[test]
    fn test_track_on_clean_repo_returns_head() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        assert!(!m.track(None).unwrap().is_empty());
    }
    #[test]
    fn test_diff_detects_modification() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let before = m.track(Some("before")).unwrap();
        fs::write(dir.path().join("file1.txt"), "modified").unwrap();
        assert!(m
            .diff(&before)
            .unwrap()
            .iter()
            .any(|d| d.path.contains("file1.txt") && matches!(d.status, FileStatus::Modified)));
    }
    #[test]
    fn test_diff_detects_addition() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let before = m.track(Some("before")).unwrap();
        fs::write(dir.path().join("file2.txt"), "new").unwrap();
        assert!(m
            .diff(&before)
            .unwrap()
            .iter()
            .any(|d| d.path.contains("file2.txt") && matches!(d.status, FileStatus::Added)));
    }
    #[test]
    fn test_diff_detects_deletion() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let before = m.track(Some("before")).unwrap();
        fs::remove_file(dir.path().join("file1.txt")).unwrap();
        assert!(m
            .diff(&before)
            .unwrap()
            .iter()
            .any(|d| d.path.contains("file1.txt") && matches!(d.status, FileStatus::Deleted)));
    }
    #[test]
    fn test_restore_reverts_working_tree() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let before = m.track(Some("before")).unwrap();
        fs::write(dir.path().join("file1.txt"), "changed").unwrap();
        fs::write(dir.path().join("file2.txt"), "new").unwrap();
        m.restore(&before).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("file1.txt")).unwrap(),
            "initial content"
        );
        assert!(!dir.path().join("file2.txt").exists());
    }
    #[test]
    fn test_revert_single_file() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let before = m.track(Some("before")).unwrap();
        fs::write(dir.path().join("file1.txt"), "changed").unwrap();
        m.revert_file("file1.txt", &before).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("file1.txt")).unwrap(),
            "initial content"
        );
    }
    #[test]
    fn test_cleanup_removes_old_records() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        m.track(Some("s1")).unwrap();
        m.track(Some("s2")).unwrap();
        assert_eq!(m.cleanup(0).unwrap(), 2);
        assert!(m.list().unwrap().is_empty());
    }
    #[test]
    fn test_list_returns_sorted_records() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        m.track(Some("s1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        m.track(Some("s2")).unwrap();
        let r = m.list().unwrap();
        assert_eq!(r.len(), 2);
        assert!(r[0].timestamp >= r[1].timestamp);
    }
    #[test]
    fn test_file_content_at_retrieves_old_content() {
        let dir = setup_test_repo();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let h = m.track(Some("before")).unwrap();
        fs::write(dir.path().join("file1.txt"), "changed").unwrap();
        assert_eq!(
            String::from_utf8(m.file_content_at("file1.txt", &h).unwrap()).unwrap(),
            "initial content"
        );
    }
    #[test]
    fn test_no_git_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let m = SnapshotManager::new(dir.path()).unwrap();
        let r = m.track(None);
        assert!(r.is_err());
        assert!(matches!(r.unwrap_err(), SnapshotError::NoGitRepo));
    }
    #[test]
    fn test_simple_hash_produces_hex_string() {
        let h = simple_hash("test_input");
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
