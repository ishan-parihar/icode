use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A change detected in the plugin directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginChange {
    Added(PathBuf),
    Removed(PathBuf),
    Modified(PathBuf),
}

impl PluginChange {
    pub fn path(&self) -> &Path {
        match self {
            PluginChange::Added(p) | PluginChange::Removed(p) | PluginChange::Modified(p) => p,
        }
    }
}

/// Watches a directory for plugin file changes using modification time polling.
///
/// Tracks the modification times of all `.rs` files in the watch directory
/// and its subdirectories. Each call to `check_for_changes()` compares the
/// current filesystem state against the last known state.
pub struct PluginWatcher {
    watch_dir: PathBuf,
    known_files: HashMap<PathBuf, SystemTime>,
}

impl PluginWatcher {
    pub fn new(plugins_dir: PathBuf) -> Self {
        Self {
            watch_dir: plugins_dir,
            known_files: HashMap::new(),
        }
    }

    pub fn watch_dir(&self) -> &Path {
        &self.watch_dir
    }

    /// Scan the plugin directory and return any changes since the last check.
    ///
    /// On the first call, all existing files are recorded as `Added` events.
    /// Subsequent calls detect additions, removals, and modifications.
    pub fn check_for_changes(&mut self) -> Vec<PluginChange> {
        let mut changes = Vec::new();
        let mut current_files: HashMap<PathBuf, SystemTime> = HashMap::new();

        let _ = self.scan_dir(&self.watch_dir, &mut current_files);

        for (path, mtime) in &current_files {
            if let Some(known_mtime) = self.known_files.get(path) {
                if mtime > known_mtime {
                    changes.push(PluginChange::Modified(path.clone()));
                }
            } else {
                changes.push(PluginChange::Added(path.clone()));
            }
        }

        for path in self.known_files.keys() {
            if !current_files.contains_key(path) {
                changes.push(PluginChange::Removed(path.clone()));
            }
        }

        self.known_files = current_files;
        changes
    }

    /// Seed the watcher with the current state without producing events.
    ///
    /// Useful when you want to start watching from the current baseline.
    pub fn seed(&mut self) {
        let mut current_files: HashMap<PathBuf, SystemTime> = HashMap::new();
        let _ = self.scan_dir(&self.watch_dir, &mut current_files);
        self.known_files = current_files;
    }

    fn scan_dir(
        &self,
        dir: &Path,
        files: &mut HashMap<PathBuf, SystemTime>,
    ) -> std::io::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if path.file_name().map_or(false, |n| {
                    n == "target" || n.to_string_lossy().starts_with('.')
                }) {
                    continue;
                }
                let _ = self.scan_dir(&path, files);
            } else if path
                .extension()
                .map_or(false, |ext| ext == "rs" || ext == "toml")
            {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(mtime) = metadata.modified() {
                        files.insert(path, mtime);
                    }
                }
            }
        }

        Ok(())
    }

    /// Reset the watcher, clearing all known state.
    pub fn reset(&mut self) {
        self.known_files.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::thread;
    use std::time::Duration;

    fn temp_dir() -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("plugin-watcher-test-{id}"))
    }

    #[test]
    fn test_initial_scan_detects_added_files() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("plugin_a.rs")).unwrap();
        File::create(dir.join("plugin_b.rs")).unwrap();

        let mut watcher = PluginWatcher::new(dir.clone());
        let changes = watcher.check_for_changes();

        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(
            |c| matches!(c, PluginChange::Added(p) if p.file_name().unwrap() == "plugin_a.rs")
        ));
        assert!(changes.iter().any(
            |c| matches!(c, PluginChange::Added(p) if p.file_name().unwrap() == "plugin_b.rs")
        ));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_no_changes_on_second_call() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("plugin.rs")).unwrap();

        let mut watcher = PluginWatcher::new(dir.clone());
        watcher.check_for_changes();
        let changes = watcher.check_for_changes();

        assert!(changes.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_detects_modified_file() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("plugin.rs");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "initial").unwrap();
        drop(file);

        let mut watcher = PluginWatcher::new(dir.clone());
        watcher.check_for_changes();

        thread::sleep(Duration::from_millis(100));

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "modified").unwrap();
        drop(file);

        let changes = watcher.check_for_changes();
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], PluginChange::Modified(p) if p == &file_path));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_detects_removed_file() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("plugin.rs")).unwrap();

        let mut watcher = PluginWatcher::new(dir.clone());
        watcher.check_for_changes();

        fs::remove_file(dir.join("plugin.rs")).unwrap();

        let changes = watcher.check_for_changes();
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], PluginChange::Removed(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_detects_added_file_after_seed() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("existing.rs")).unwrap();

        let mut watcher = PluginWatcher::new(dir.clone());
        watcher.seed();

        File::create(dir.join("new_plugin.rs")).unwrap();

        let changes = watcher.check_for_changes();
        assert_eq!(changes.len(), 1);
        assert!(
            matches!(&changes[0], PluginChange::Added(p) if p.file_name().unwrap() == "new_plugin.rs")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ignores_non_rust_files() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("plugin.rs")).unwrap();
        File::create(dir.join("readme.md")).unwrap();
        File::create(dir.join("config.json")).unwrap();

        let mut watcher = PluginWatcher::new(dir.clone());
        let changes = watcher.check_for_changes();

        assert_eq!(changes.len(), 1);
        assert!(
            matches!(&changes[0], PluginChange::Added(p) if p.file_name().unwrap() == "plugin.rs")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ignores_target_and_dot_dirs() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        File::create(dir.join("target/build.rs")).unwrap();
        File::create(dir.join(".git/config.rs")).unwrap();
        File::create(dir.join("src/plugin.rs")).unwrap();

        let mut watcher = PluginWatcher::new(dir.clone());
        let changes = watcher.check_for_changes();

        assert_eq!(changes.len(), 1);
        assert!(
            matches!(&changes[0], PluginChange::Added(p) if p.to_string_lossy().contains("src"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_watch_dir_accessor() {
        let dir = temp_dir();
        let watcher = PluginWatcher::new(dir.clone());
        assert_eq!(watcher.watch_dir(), dir);
    }

    #[test]
    fn test_reset_clears_state() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("plugin.rs")).unwrap();

        let mut watcher = PluginWatcher::new(dir.clone());
        watcher.check_for_changes();
        watcher.reset();

        let changes = watcher.check_for_changes();
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], PluginChange::Added(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_plugin_change_path() {
        let path = PathBuf::from("/tmp/test.rs");
        assert_eq!(PluginChange::Added(path.clone()).path(), &path);
        assert_eq!(PluginChange::Removed(path.clone()).path(), &path);
        assert_eq!(PluginChange::Modified(path.clone()).path(), &path);
    }
}
