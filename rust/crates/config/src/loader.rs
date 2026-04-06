use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::jsonc::parse_config;
use crate::schema::Config;

/// Represents the source of a loaded config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    User,
    Project,
    Local,
}

/// A discovered config file with its source and path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    pub source: ConfigSource,
    pub path: PathBuf,
}

/// Loads config files with JSONC support, merging by precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLoader {
    cwd: PathBuf,
    config_home: PathBuf,
}

impl ConfigLoader {
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>, config_home: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            config_home: config_home.into(),
        }
    }

    #[must_use]
    pub fn default_for(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let config_home = default_config_home();
        Self { cwd, config_home }
    }

    #[must_use]
    pub fn config_home(&self) -> &Path {
        &self.config_home
    }

    /// Discover config file paths in precedence order.
    #[must_use]
    pub fn discover(&self) -> Vec<ConfigEntry> {
        let user_legacy_path = self.config_home.parent().map_or_else(
            || PathBuf::from(".icode.json"),
            |parent| parent.join(".icode.json"),
        );
        vec![
            ConfigEntry {
                source: ConfigSource::User,
                path: user_legacy_path,
            },
            ConfigEntry {
                source: ConfigSource::User,
                path: self.config_home.join("settings.json"),
            },
            ConfigEntry {
                source: ConfigSource::Project,
                path: self.cwd.join(".icode.json"),
            },
            ConfigEntry {
                source: ConfigSource::Project,
                path: self.cwd.join(".icode").join("settings.json"),
            },
            ConfigEntry {
                source: ConfigSource::Local,
                path: self.cwd.join(".icode").join("settings.local.json"),
            },
        ]
    }

    /// Load and merge config files, using JSONC parsing.
    pub fn load(&self) -> Result<LoadedConfig, String> {
        let mut merged = serde_json::Map::new();
        let mut loaded_entries = Vec::new();

        for entry in self.discover() {
            let Some(value) = read_optional_jsonc(&entry.path)? else {
                continue;
            };
            if let serde_json::Value::Object(obj) = value {
                deep_merge(&mut merged, &obj);
            }
            loaded_entries.push(entry);
        }

        let merged_value = serde_json::Value::Object(merged.clone());
        let typed: Config = serde_json::from_value(merged_value).map_err(|e| e.to_string())?;

        Ok(LoadedConfig {
            merged,
            loaded_entries,
            typed,
        })
    }

    /// Load a config file directly as a typed value (JSONC).
    pub fn load_typed<T: DeserializeOwned>(&self, path: &Path) -> Result<T, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        parse_config(&content).and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
    }
}

/// Result of loading config, containing both raw merged JSON and typed config.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    merged: serde_json::Map<String, serde_json::Value>,
    loaded_entries: Vec<ConfigEntry>,
    typed: Config,
}

impl LoadedConfig {
    #[must_use]
    pub fn merged(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.merged
    }

    #[must_use]
    pub fn loaded_entries(&self) -> &[ConfigEntry] {
        &self.loaded_entries
    }

    #[must_use]
    pub fn typed(&self) -> &Config {
        &self.typed
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.merged.get(key)
    }
}

fn default_config_home() -> PathBuf {
    std::env::var_os("CLAW_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".icode")))
        .unwrap_or_else(|| PathBuf::from(".icode"))
}

fn read_optional_jsonc(path: &Path) -> Result<Option<serde_json::Value>, String> {
    let is_legacy_config = path.file_name().and_then(|name| name.to_str()) == Some(".icode.json");
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) if is_legacy_config => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };

    if contents.trim().is_empty() {
        return Ok(Some(serde_json::Value::Object(serde_json::Map::new())));
    }

    match parse_config(&contents) {
        Ok(value) => {
            if value.is_object() {
                Ok(Some(value))
            } else if is_legacy_config {
                Ok(None)
            } else {
                Err(format!(
                    "{}: top-level value must be a JSON object",
                    path.display()
                ))
            }
        }
        Err(_) if is_legacy_config => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn deep_merge(
    target: &mut serde_json::Map<String, serde_json::Value>,
    source: &serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in source {
        match (target.get_mut(key), value) {
            (Some(serde_json::Value::Object(existing)), serde_json::Value::Object(incoming)) => {
                deep_merge(existing, incoming);
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}
