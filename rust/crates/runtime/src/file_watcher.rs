use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::event_bus::{events, publish_event};

/// A file system event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvent {
    pub path: String,
    pub kind: FileEventKind,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEventKind {
    Created,
    Modified,
    Deleted,
}

/// Snapshot of a directory's state for change detection.
#[derive(Debug, Clone)]
pub struct DirSnapshot {
    pub files: HashMap<PathBuf, FileMeta>,
    pub taken_at: Instant,
}

#[derive(Debug, Clone)]
pub struct FileMeta {
    pub size: u64,
    pub modified: std::time::SystemTime,
}

/// A polling-based file watcher for directories.
pub struct FileWatcher {
    watched_dirs: RwLock<Vec<PathBuf>>,
    running: AtomicBool,
    last_snapshot: RwLock<Option<DirSnapshot>>,
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            watched_dirs: RwLock::new(Vec::new()),
            running: AtomicBool::new(false),
            last_snapshot: RwLock::new(None),
        }
    }

    /// Start watching a directory.
    pub fn watch(&self, dir: &Path) {
        let mut dirs = self.watched_dirs.write().unwrap_or_else(|e| e.into_inner());
        if !dirs.contains(&dir.to_path_buf()) {
            dirs.push(dir.to_path_buf());
        }
    }

    /// Stop watching all directories.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Take a snapshot of all watched directories.
    pub fn snapshot(&self) -> DirSnapshot {
        let dirs = self.watched_dirs.read().unwrap_or_else(|e| e.into_inner());
        let mut files = HashMap::new();

        for dir in dirs.iter() {
            Self::walk_dir(dir, &mut files);
        }

        DirSnapshot {
            files,
            taken_at: Instant::now(),
        }
    }

    /// Compare two snapshots and return the list of changes.
    pub fn diff(old: &DirSnapshot, new: &DirSnapshot) -> Vec<FileEvent> {
        let mut events = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Find created and modified files
        for (path, new_meta) in &new.files {
            if let Some(old_meta) = old.files.get(path) {
                if old_meta.size != new_meta.size || old_meta.modified != new_meta.modified {
                    events.push(FileEvent {
                        path: path.to_string_lossy().to_string(),
                        kind: FileEventKind::Modified,
                        timestamp: now,
                    });
                }
            } else {
                events.push(FileEvent {
                    path: path.to_string_lossy().to_string(),
                    kind: FileEventKind::Created,
                    timestamp: now,
                });
            }
        }

        // Find deleted files
        for path in old.files.keys() {
            if !new.files.contains_key(path) {
                events.push(FileEvent {
                    path: path.to_string_lossy().to_string(),
                    kind: FileEventKind::Deleted,
                    timestamp: now,
                });
            }
        }

        events
    }

    /// Walk a directory and collect file metadata.
    fn walk_dir(dir: &Path, files: &mut HashMap<PathBuf, FileMeta>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories and common ignore patterns
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.')
                            || name == "node_modules"
                            || name == "target"
                            || name == ".git"
                        {
                            continue;
                        }
                    }
                    Self::walk_dir(&path, files);
                } else if path.is_file() {
                    if let Ok(metadata) = entry.metadata() {
                        files.insert(
                            path.clone(),
                            FileMeta {
                                size: metadata.len(),
                                modified: metadata
                                    .modified()
                                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                            },
                        );
                    }
                }
            }
        }
    }

    /// Check for changes since the last snapshot. Returns events if changes detected.
    pub fn check_changes(&self) -> Option<Vec<FileEvent>> {
        let new_snapshot = self.snapshot();
        let mut last = self
            .last_snapshot
            .write()
            .unwrap_or_else(|e| e.into_inner());

        let events = if let Some(old) = last.as_ref() {
            Self::diff(old, &new_snapshot)
        } else {
            Vec::new()
        };

        *last = Some(new_snapshot);

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }

    /// Check for changes and publish them to the global event bus if any are found.
    /// Returns the list of events that were published.
    pub fn check_and_publish(&self) -> Option<Vec<FileEvent>> {
        if let Some(events) = self.check_changes() {
            let event_paths: Vec<String> = events.iter().map(|e| e.path.clone()).collect();
            publish_event(
                &events::FILE_WATCHER_UPDATED,
                serde_json::json!({
                    "events": events,
                    "count": event_paths.len(),
                }),
            );
            Some(events)
        } else {
            None
        }
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Global file watcher instance.
static GLOBAL_WATCHER: std::sync::OnceLock<FileWatcher> = std::sync::OnceLock::new();

pub fn global_watcher() -> &'static FileWatcher {
    GLOBAL_WATCHER.get_or_init(FileWatcher::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn detects_created_file() {
        let dir = std::env::temp_dir().join(format!(
            "fw-create-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let watcher = FileWatcher::new();
        watcher.watch(&dir);

        // Take initial snapshot
        let _ = watcher.check_changes();

        // Create a file
        fs::write(dir.join("new.txt"), "hello").unwrap();
        thread::sleep(Duration::from_millis(10));

        // Check for changes
        let events = watcher.check_changes().expect("should detect change");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, FileEventKind::Created);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn detects_deleted_file() {
        let dir = std::env::temp_dir().join(format!(
            "fw-delete-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("todelete.txt"), "bye").unwrap();

        let watcher = FileWatcher::new();
        watcher.watch(&dir);
        let _ = watcher.check_changes();

        fs::remove_file(dir.join("todelete.txt")).unwrap();
        thread::sleep(Duration::from_millis(10));

        let events = watcher.check_changes().expect("should detect deletion");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, FileEventKind::Deleted);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_changes_when_unchanged() {
        let dir = std::env::temp_dir().join(format!(
            "fw-unchanged-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("stable.txt"), "unchanged").unwrap();

        let watcher = FileWatcher::new();
        watcher.watch(&dir);
        let _ = watcher.check_changes();

        let events = watcher.check_changes();
        assert!(events.is_none(), "should detect no changes");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn global_watcher_is_singleton() {
        let a = global_watcher();
        let b = global_watcher();
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn skips_hidden_dirs_and_node_modules() {
        let dir = std::env::temp_dir().join(format!(
            "fw-skip-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::create_dir_all(dir.join("node_modules")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();

        let watcher = FileWatcher::new();
        watcher.watch(&dir);
        let snapshot = watcher.snapshot();

        // Should only have src/main.rs
        assert_eq!(snapshot.files.len(), 1);
        assert!(snapshot.files.contains_key(&dir.join("src/main.rs")));

        let _ = fs::remove_dir_all(dir);
    }
}
