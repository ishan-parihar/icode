use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MODEL_STATE_FILE: &str = "model_state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelState {
    pub current: Option<String>,
    #[serde(default)]
    pub recent: Vec<String>,
    #[serde(default)]
    pub favorites: Vec<String>,
}

impl ModelState {
    fn state_path() -> PathBuf {
        let home = std::env::var("HOME")
            .ok()
            .or_else(|| std::env::var("USERPROFILE").ok())
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join(".icode").join(MODEL_STATE_FILE)
    }

    pub fn load() -> Self {
        let path = Self::state_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = Self::state_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
        }
    }

    pub fn set_current(&mut self, model: &str) {
        self.current = Some(model.to_string());
        self.add_recent(model);
    }

    pub fn add_recent(&mut self, model: &str) {
        let model = model.to_string();
        self.recent.retain(|m| m != &model);
        self.recent.insert(0, model);
        self.recent.truncate(8);
    }

    pub fn toggle_favorite(&mut self, model: &str) -> bool {
        let model = model.to_string();
        if self.favorites.contains(&model) {
            self.favorites.retain(|m| m != &model);
            false
        } else {
            self.favorites.push(model);
            true
        }
    }

    pub fn is_favorite(&self, model: &str) -> bool {
        self.favorites.contains(&model.to_string())
    }

    pub fn resolve_default(&self) -> Option<String> {
        self.current
            .clone()
            .or_else(|| self.recent.first().cloned())
            .or_else(|| self.favorites.first().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_tracks_order_and_dedup() {
        let mut state = ModelState::default();
        state.add_recent("claude-sonnet-4-6");
        state.add_recent("grok-3");
        state.add_recent("claude-sonnet-4-6");
        assert_eq!(state.recent[0], "claude-sonnet-4-6");
        assert_eq!(state.recent[1], "grok-3");
        assert_eq!(state.recent.len(), 2);
    }

    #[test]
    fn favorite_toggle_works() {
        let mut state = ModelState::default();
        assert!(state.toggle_favorite("opus"));
        assert!(state.is_favorite("opus"));
        assert!(!state.toggle_favorite("opus"));
        assert!(!state.is_favorite("opus"));
    }

    #[test]
    fn resolve_default_priority() {
        let mut state = ModelState::default();
        assert!(state.resolve_default().is_none());

        state.add_recent("grok-3");
        state.favorites.push("opus".to_string());
        assert_eq!(state.resolve_default(), Some("grok-3".to_string()));

        state.current = Some("haiku".to_string());
        assert_eq!(state.resolve_default(), Some("haiku".to_string()));
    }

    #[test]
    fn recent_caps_at_8() {
        let mut state = ModelState::default();
        for i in 0..12 {
            state.add_recent(&format!("model-{i}"));
        }
        assert_eq!(state.recent.len(), 8);
        assert_eq!(state.recent[0], "model-11");
    }
}
