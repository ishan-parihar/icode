use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A simple file-backed key-value store using JSON serialization.
///
/// Used by plugins to persist state (settings, preferences, counters) across sessions.
pub struct KvStore {
    data: HashMap<String, serde_json::Value>,
    path: PathBuf,
    dirty: bool,
}

impl KvStore {
    /// Create a new `KvStore` backed by the given file path.
    ///
    /// The store is empty until `load()` is called. If the file does not exist,
    /// `load()` will create it with an empty object.
    pub fn new(path: PathBuf) -> Self {
        Self {
            data: HashMap::new(),
            path,
            dirty: false,
        }
    }

    /// Get a value by key, returning the default if not found.
    pub fn get<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.data
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(default)
    }

    /// Set a value by key. Marks the store as dirty for persistence.
    pub fn set<T: Serialize>(&mut self, key: &str, value: T) {
        if let Ok(v) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), v);
            self.dirty = true;
        }
    }

    /// Remove a value by key. Returns true if the key existed.
    pub fn remove(&mut self, key: &str) -> bool {
        let removed = self.data.remove(key).is_some();
        if removed {
            self.dirty = true;
        }
        removed
    }

    /// Get all keys currently in the store.
    pub fn keys(&self) -> Vec<&str> {
        self.data.keys().map(String::as_str).collect()
    }

    /// Check if a key exists in the store.
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Save the store to disk. Only writes if there are unsaved changes.
    pub fn save(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create dir {}: {e}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(&self.data)
            .map_err(|e| format!("failed to serialize kv store: {e}"))?;
        fs::write(&self.path, content)
            .map_err(|e| format!("cannot write kv store to {}: {e}", self.path.display()))?;
        self.dirty = false;
        Ok(())
    }

    /// Load the store from disk. If the file doesn't exist, starts empty.
    pub fn load(&mut self) -> Result<(), String> {
        if !self.path.exists() {
            // Start with empty store, no error
            return Ok(());
        }
        let content = fs::read_to_string(&self.path)
            .map_err(|e| format!("cannot read kv store from {}: {e}", self.path.display()))?;
        self.data = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse kv store from {}: {e}", self.path.display()))?;
        self.dirty = false;
        Ok(())
    }

    /// Clear all data from the store.
    pub fn clear(&mut self) {
        self.data.clear();
        self.dirty = true;
    }

    /// Return the file path backing this store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check if there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "icode-kv-test-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn test_get_set_default() {
        let mut store = KvStore::new(temp_path());
        assert_eq!(store.get("missing", 42), 42);
        store.set("answer", 99);
        assert_eq!(store.get("answer", 0), 99);
    }

    #[test]
    fn test_remove() {
        let mut store = KvStore::new(temp_path());
        store.set("key", "value");
        assert!(store.remove("key"));
        assert!(!store.remove("key"));
        assert_eq!(store.get("key", String::from("default")), "default");
    }

    #[test]
    fn test_save_load() {
        let path = temp_path();
        {
            let mut store = KvStore::new(path.clone());
            store.set("theme", "dark");
            store.set("count", 42);
            store.save().unwrap();
        }
        {
            let mut store = KvStore::new(path.clone());
            store.load().unwrap();
            assert_eq!(store.get("theme", String::new()), "dark");
            assert_eq!(store.get("count", 0), 42);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_keys() {
        let mut store = KvStore::new(temp_path());
        store.set("a", 1);
        store.set("b", 2);
        let mut keys = store.keys();
        keys.sort_unstable();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let path = temp_path();
        let mut store = KvStore::new(path.clone());
        // Should not error even if file doesn't exist
        store.load().unwrap();
        assert_eq!(store.keys().len(), 0);
    }

    #[test]
    fn test_dirty_flag() {
        let mut store = KvStore::new(temp_path());
        assert!(!store.is_dirty());
        store.set("x", 1);
        assert!(store.is_dirty());
        store.save().unwrap();
        assert!(!store.is_dirty());
    }
}
