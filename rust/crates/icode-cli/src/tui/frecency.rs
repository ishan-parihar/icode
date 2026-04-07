use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single entry in the frecency store, tracking how often and how recently
/// a piece of text was used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrecencyEntry {
    pub text: String,
    pub frequency: u32,
    pub last_used: u128,
}

impl FrecencyEntry {
    /// Compute the frecency score for this entry.
    ///
    /// Score formula: `frequency * min(30.0, 30.0 / max(1.0, days_since_last_use))`
    ///
    /// - Used today (same day): score = frequency * 30
    /// - Used yesterday: score = frequency * 30
    /// - Used 3 days ago: score = frequency * 10
    /// - Used a week ago: score = frequency * ~4.3
    /// - Used a month ago: score = frequency * 1
    pub fn score(&self) -> f64 {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let elapsed_ms = now_ms.saturating_sub(self.last_used);
        let days_since = (elapsed_ms as f64) / (24.0 * 60.0 * 60.0 * 1000.0);

        let capped_days = days_since.min(30.0).max(1.0);
        let multiplier = 30.0 / capped_days;

        (self.frequency as f64) * multiplier
    }
}

/// A file-backed store for frecency data, supporting load/save from JSON
/// and recording/scoring text entries.
#[derive(Debug)]
pub struct FrecencyStore {
    entries: HashMap<String, FrecencyEntry>,
    path: PathBuf,
    dirty: bool,
}

impl FrecencyStore {
    /// Create a new `FrecencyStore` backed by the given file path.
    /// The store is empty until `load()` is called.
    pub fn new(path: PathBuf) -> Self {
        Self {
            entries: HashMap::new(),
            path,
            dirty: false,
        }
    }

    /// Load entries from the JSON file. If the file doesn't exist, starts empty.
    pub fn load(&mut self) -> Result<(), String> {
        if !self.path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(&self.path)
            .map_err(|e| format!("cannot read frecency from {}: {e}", self.path.display()))?;
        let entries_vec: Vec<FrecencyEntry> = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse frecency from {}: {e}", self.path.display()))?;
        self.entries = entries_vec
            .into_iter()
            .map(|e| (e.text.clone(), e))
            .collect();
        self.dirty = false;
        Ok(())
    }

    /// Persist entries to the JSON file. Only writes if there are unsaved changes.
    pub fn save(&self) -> Result<(), String> {
        let mut entries_vec: Vec<&FrecencyEntry> = self.entries.values().collect();
        entries_vec.sort_by(|a, b| {
            b.score()
                .partial_cmp(&a.score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create dir {}: {e}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(&entries_vec)
            .map_err(|e| format!("failed to serialize frecency: {e}"))?;
        fs::write(&self.path, content)
            .map_err(|e| format!("cannot write frecency to {}: {e}", self.path.display()))?;
        Ok(())
    }

    /// Record a text entry: increment frequency and update last_used timestamp.
    /// If the text already exists, merges by incrementing frequency.
    pub fn record(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        if let Some(entry) = self.entries.get_mut(trimmed) {
            entry.frequency += 1;
            entry.last_used = now_ms;
        } else {
            self.entries.insert(
                trimmed.to_string(),
                FrecencyEntry {
                    text: trimmed.to_string(),
                    frequency: 1,
                    last_used: now_ms,
                },
            );
        }
        self.dirty = true;
    }

    /// Return top-scored entries whose text matches the given prefix.
    pub fn suggestions(&self, prefix: &str, limit: usize) -> Vec<String> {
        if prefix.is_empty() {
            return self.top_entries(limit);
        }
        let prefix_lower = prefix.to_lowercase();
        let mut scored: Vec<&FrecencyEntry> = self
            .entries
            .values()
            .filter(|e| e.text.to_lowercase().starts_with(&prefix_lower))
            .collect();
        scored.sort_by(|a, b| {
            b.score()
                .partial_cmp(&a.score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored
            .into_iter()
            .take(limit)
            .map(|e| e.text.clone())
            .collect()
    }

    /// Return top-scored entries regardless of prefix.
    pub fn top_entries(&self, limit: usize) -> Vec<String> {
        let mut scored: Vec<&FrecencyEntry> = self.entries.values().collect();
        scored.sort_by(|a, b| {
            b.score()
                .partial_cmp(&a.score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored
            .into_iter()
            .take(limit)
            .map(|e| e.text.clone())
            .collect()
    }

    /// Check if there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Return the file path backing this store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of entries in the store.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the store has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "icode-frecency-test-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn entry_with_age(text: &str, frequency: u32, days_ago: f64) -> FrecencyEntry {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let age_ms = (days_ago * 24.0 * 60.0 * 60.0 * 1000.0) as u128;
        FrecencyEntry {
            text: text.to_string(),
            frequency,
            last_used: now_ms.saturating_sub(age_ms),
        }
    }

    #[test]
    fn test_score_used_today() {
        let entry = entry_with_age("hello", 1, 0.01);
        let score = entry.score();
        assert!(
            (score - 30.0).abs() < 0.1,
            "score was {score}, expected ~30"
        );
    }

    #[test]
    fn test_score_used_yesterday() {
        let entry = entry_with_age("hello", 1, 1.0);
        let score = entry.score();
        assert!(
            (score - 30.0).abs() < 0.1,
            "score was {score}, expected ~30"
        );
    }

    #[test]
    fn test_score_used_3_days_ago() {
        let entry = entry_with_age("hello", 1, 3.0);
        let score = entry.score();
        assert!(
            (score - 10.0).abs() < 0.1,
            "score was {score}, expected ~10"
        );
    }

    #[test]
    fn test_score_used_week_ago() {
        let entry = entry_with_age("hello", 1, 7.0);
        let score = entry.score();
        assert!(
            (score - 4.2857).abs() < 0.1,
            "score was {score}, expected ~4.29"
        );
    }

    #[test]
    fn test_score_used_month_ago() {
        let entry = entry_with_age("hello", 1, 30.0);
        let score = entry.score();
        assert!((score - 1.0).abs() < 0.1, "score was {score}, expected ~1");
    }

    #[test]
    fn test_score_used_very_old() {
        let entry = entry_with_age("hello", 1, 365.0);
        let score = entry.score();
        assert!((score - 1.0).abs() < 0.01, "score was {score}, expected ~1");
    }

    #[test]
    fn test_score_frequency_scaling() {
        let entry1 = entry_with_age("hello", 1, 3.0);
        let entry10 = entry_with_age("hello", 10, 3.0);
        assert!(
            (entry10.score() - entry1.score() * 10.0).abs() < 0.1,
            "score10={} should be ~10x score1={}",
            entry10.score(),
            entry1.score()
        );
    }

    #[test]
    fn test_record_new_entry() {
        let mut store = FrecencyStore::new(temp_path());
        store.record("hello world");
        assert_eq!(store.len(), 1);
        let entry = store.entries.get("hello world").unwrap();
        assert_eq!(entry.frequency, 1);
        assert!(entry.last_used > 0);
    }

    #[test]
    fn test_record_duplicates_merge() {
        let mut store = FrecencyStore::new(temp_path());
        store.record("hello world");
        store.record("hello world");
        store.record("hello world");
        assert_eq!(store.len(), 1);
        let entry = store.entries.get("hello world").unwrap();
        assert_eq!(entry.frequency, 3);
    }

    #[test]
    fn test_record_empty_text_ignored() {
        let mut store = FrecencyStore::new(temp_path());
        store.record("");
        store.record("   ");
        store.record("\t\n");
        assert!(store.is_empty());
    }

    #[test]
    fn test_save_load_roundtrip() {
        let path = temp_path();
        {
            let mut store = FrecencyStore::new(path.clone());
            store.record("first command");
            store.record("second command");
            store.record("first command");
            store.save().unwrap();
        }
        {
            let mut store = FrecencyStore::new(path.clone());
            store.load().unwrap();
            assert_eq!(store.len(), 2);
            let first = store.entries.get("first command").unwrap();
            assert_eq!(first.frequency, 2);
            let second = store.entries.get("second command").unwrap();
            assert_eq!(second.frequency, 1);
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let path = temp_path();
        let mut store = FrecencyStore::new(path.clone());
        store.load().unwrap();
        assert!(store.is_empty());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_suggestions_with_prefix() {
        let mut store = FrecencyStore::new(temp_path());
        store.record("cargo build");
        store.record("cargo test");
        store.record("cargo clippy");
        store.record("git status");
        store.record("git log");

        let results = store.suggestions("cargo", 10);
        assert_eq!(results.len(), 3);
        assert!(results
            .iter()
            .all(|s| s.to_lowercase().starts_with("cargo")));

        let results = store.suggestions("git", 10);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|s| s.to_lowercase().starts_with("git")));

        let results = store.suggestions("nonexistent", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_suggestions_sorted_by_score() {
        let mut store = FrecencyStore::new(temp_path());
        for _ in 0..5 {
            store.record("cargo test");
        }
        store.record("cargo build");
        store.record("cargo clippy");

        let results = store.suggestions("cargo", 10);
        assert_eq!(results[0], "cargo test");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_top_entries() {
        let mut store = FrecencyStore::new(temp_path());
        store.record("frequent command");
        store.record("frequent command");
        store.record("frequent command");
        store.record("frequent command");
        store.record("frequent command");
        store.record("rare command");

        let results = store.top_entries(10);
        assert_eq!(results[0], "frequent command");
        assert_eq!(results[1], "rare command");
    }

    #[test]
    fn test_top_entries_respects_limit() {
        let mut store = FrecencyStore::new(temp_path());
        for i in 0..10 {
            store.record(&format!("command {i}"));
        }
        let results = store.top_entries(3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_suggestions_empty_prefix_returns_top() {
        let mut store = FrecencyStore::new(temp_path());
        store.record("alpha");
        for _ in 0..5 {
            store.record("beta");
        }
        store.record("gamma");

        let results = store.suggestions("", 10);
        assert_eq!(results[0], "beta");
    }

    #[test]
    fn test_case_insensitive_prefix_matching() {
        let mut store = FrecencyStore::new(temp_path());
        store.record("Cargo Build");
        store.record("CARGO TEST");

        let results = store.suggestions("cargo", 10);
        assert_eq!(results.len(), 2);

        let results = store.suggestions("CARGO", 10);
        assert_eq!(results.len(), 2);

        let results = store.suggestions("CaRgO", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_dirty_flag() {
        let mut store = FrecencyStore::new(temp_path());
        assert!(!store.is_dirty());
        store.record("test");
        assert!(store.is_dirty());
        store.save().unwrap();
    }
}
