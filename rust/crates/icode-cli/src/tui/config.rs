/// Scroll behavior configuration for the TUI.
#[derive(Debug, Clone, Copy)]
pub struct ScrollConfig {
    /// Base scroll speed in lines per scroll event.
    ///
    /// This value is used for Up/Down arrow key scrolls and as the base
    /// for mouse wheel scrolling. Default: 3.0.
    pub speed: f64,
    /// When enabled, consecutive scrolls in the same direction multiply
    /// the speed by 1.2x per event (capped at 10x base speed).
    ///
    /// Resets when direction changes or after 500ms of no scrolling.
    /// Default: false.
    pub acceleration: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            speed: 3.0,
            acceleration: false,
        }
    }
}

/// Tracks acceleration state for scroll events.
#[derive(Debug, Default)]
pub struct ScrollState {
    /// Last scroll direction (true = down, false = up).
    pub last_direction: Option<bool>,
    /// Timestamp of the last scroll event.
    pub last_scroll_time: Option<std::time::Instant>,
    /// Current acceleration multiplier (starts at 1.0).
    pub multiplier: f64,
}

impl ScrollState {
    /// Calculate the effective scroll amount for the given direction.
    ///
    /// Returns `(amount, new_state)` where `amount` is the number of lines
    /// to scroll and `new_state` is the updated scroll state.
    pub fn compute_amount(
        &mut self,
        direction: bool,
        base_amount: f64,
        config: &ScrollConfig,
    ) -> f64 {
        let now = std::time::Instant::now();

        // Reset acceleration if direction changed or timeout exceeded
        let reset = match (self.last_direction, self.last_scroll_time) {
            (Some(last_dir), Some(last_time)) => {
                last_dir != direction
                    || now.duration_since(last_time) > std::time::Duration::from_millis(500)
            }
            _ => true,
        };

        if reset {
            self.multiplier = 1.0;
        } else if config.acceleration {
            // Apply acceleration: multiply by 1.2x, capped at 10x
            self.multiplier = (self.multiplier * 1.2).min(10.0);
        }

        self.last_direction = Some(direction);
        self.last_scroll_time = Some(now);

        base_amount * self.multiplier
    }
}

// ── TUI configuration (tui.json) ────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Keybind overrides mapping action names to key sequences.
///
/// Known action names:
/// `quit`, `scroll_up`, `scroll_down`, `scroll_page_up`, `scroll_page_down`,
/// `model_picker`, `command_palette`, `help`, `toggle_theme`, `new_session`,
/// `undo`, `redo`, `toggle_sidebar`, `toggle_thinking`, `leader_key`
pub type KeybindOverrides = HashMap<String, String>;

/// TUI configuration loaded from `~/.icode/tui.json`.
///
/// All fields have sensible defaults matching current behavior.
/// If the config file is missing or corrupt, defaults are used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    /// Theme name: "dark", "light", or a custom theme identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,

    /// Keybind overrides mapping action names to key sequences.
    #[serde(default, skip_serializing_if = "KeybindOverrides::is_empty")]
    pub keybinds: KeybindOverrides,

    /// Scroll speed multiplier (lines per scroll event).
    #[serde(default = "default_scroll_speed")]
    pub scroll_speed: f64,

    /// Whether scroll acceleration is enabled.
    #[serde(default)]
    pub scroll_acceleration: bool,

    /// Whether the sidebar is visible on startup.
    #[serde(default = "default_true")]
    pub sidebar_visible: bool,

    /// Whether thinking/reasoning blocks are shown by default.
    #[serde(default = "default_true")]
    pub show_thinking: bool,

    /// Whether per-message timestamps are displayed.
    #[serde(default = "default_true")]
    pub show_timestamps: bool,
}

fn default_scroll_speed() -> f64 {
    3.0
}

fn default_true() -> bool {
    true
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            theme: None,
            keybinds: KeybindOverrides::new(),
            scroll_speed: 3.0,
            scroll_acceleration: false,
            sidebar_visible: true,
            show_thinking: true,
            show_timestamps: true,
        }
    }
}

fn tui_config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        return PathBuf::from(path).join("tui.json");
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".icode").join("tui.json")
}

impl TuiConfig {
    /// Load configuration from `~/.icode/tui.json`.
    ///
    /// Returns defaults if:
    /// - The file does not exist
    /// - The file cannot be read
    /// - The file contains invalid JSON
    ///
    /// Logs a warning to stderr if the file exists but is corrupt.
    pub fn load() -> Self {
        let path = tui_config_path();

        if !path.exists() {
            return Self::default();
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[tui config] warning: cannot read {}: {}",
                    path.display(),
                    e
                );
                return Self::default();
            }
        };

        match serde_json::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!(
                    "[tui config] warning: corrupt config in {} ({}), using defaults",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Save the current configuration to `~/.icode/tui.json`.
    ///
    /// Creates the `~/.icode/` directory if it does not exist.
    /// Silently ignores errors (non-critical operation).
    pub fn save(&self) {
        let path = tui_config_path();

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let content = match serde_json::to_string_pretty(self) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[tui config] warning: cannot serialize config: {}", e);
                return;
            }
        };

        if let Err(e) = fs::write(&path, content) {
            eprintln!(
                "[tui config] warning: cannot write {}: {}",
                path.display(),
                e
            );
        }
    }

    /// Return the path to the TUI config file (for display purposes).
    pub fn config_path() -> PathBuf {
        tui_config_path()
    }
}

#[cfg(test)]
mod tui_config_tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = TuiConfig::default();
        assert!(config.theme.is_none());
        assert!(config.keybinds.is_empty());
        assert!((config.scroll_speed - 3.0).abs() < f64::EPSILON);
        assert!(!config.scroll_acceleration);
        assert!(config.sidebar_visible);
        assert!(config.show_thinking);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut config = TuiConfig::default();
        config.theme = Some("dark".into());
        config.scroll_speed = 5.0;
        config.scroll_acceleration = true;

        let json = serde_json::to_string(&config).unwrap();
        let loaded: TuiConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.theme, Some("dark".into()));
        assert!((loaded.scroll_speed - 5.0).abs() < f64::EPSILON);
        assert!(loaded.scroll_acceleration);
    }

    #[test]
    fn test_deserialize_with_missing_fields() {
        let json = r#"{"sidebar_visible": false}"#;
        let config: TuiConfig = serde_json::from_str(json).unwrap();

        assert!(config.theme.is_none());
        assert!(config.keybinds.is_empty());
        assert!((config.scroll_speed - 3.0).abs() < f64::EPSILON);
        assert!(!config.scroll_acceleration);
        assert!(!config.sidebar_visible);
        assert!(config.show_thinking);
    }

    #[test]
    fn test_deserialize_corrupt_json() {
        let json = r#"{"broken": json}"#;
        let result = serde_json::from_str::<TuiConfig>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_keybind_overrides() {
        let mut config = TuiConfig::default();
        config.keybinds.insert("quit".into(), "Ctrl+Q".into());
        config
            .keybinds
            .insert("toggle_sidebar".into(), "Alt+S".into());

        let json = serde_json::to_string(&config).unwrap();
        let loaded: TuiConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.keybinds.get("quit"), Some(&"Ctrl+Q".to_string()));
        assert_eq!(
            loaded.keybinds.get("toggle_sidebar"),
            Some(&"Alt+S".to_string())
        );
    }
}
