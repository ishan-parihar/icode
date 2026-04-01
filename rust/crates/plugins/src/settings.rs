use std::path::Path;

use runtime::RuntimePluginConfig;
use serde_json::{Map, Value};

pub fn read_settings_file(path: &Path) -> Result<Map<String, Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            if contents.trim().is_empty() {
                return Ok(Map::new());
            }
            serde_json::from_str::<Value>(&contents)
                .map_err(|error| error.to_string())?
                .as_object()
                .cloned()
                .ok_or_else(|| "settings file must contain a JSON object".to_string())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn write_settings_file(path: &Path, root: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub fn read_enabled_plugin_map(root: &Map<String, Value>) -> Map<String, Value> {
    root.get("enabledPlugins")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub fn write_plugin_state(
    root: &mut Map<String, Value>,
    plugin_id: &str,
    enabled: Option<bool>,
) {
    let mut enabled_plugins = read_enabled_plugin_map(root);
    match enabled {
        Some(value) => {
            enabled_plugins.insert(plugin_id.to_string(), Value::Bool(value));
        }
        None => {
            enabled_plugins.remove(plugin_id);
        }
    }
    if enabled_plugins.is_empty() {
        root.remove("enabledPlugins");
    } else {
        root.insert("enabledPlugins".to_string(), Value::Object(enabled_plugins));
    }
}

pub fn config_from_settings(root: &Map<String, Value>) -> RuntimePluginConfig {
    let mut config = RuntimePluginConfig::default();
    if let Some(enabled_plugins) = root.get("enabledPlugins").and_then(Value::as_object) {
        for (plugin_id, enabled) in enabled_plugins {
            match enabled.as_bool() {
                Some(value) => config.set_plugin_state(plugin_id.clone(), value),
                None => {}
            }
        }
    }
    config
}

#[cfg(test)]
mod tests {
    use super::{config_from_settings, write_plugin_state};
    use serde_json::{json, Map, Value};

    #[test]
    fn writes_and_removes_enabled_plugin_state() {
        let mut root = Map::new();
        write_plugin_state(&mut root, "demo@external", Some(true));
        assert_eq!(
            root.get("enabledPlugins"),
            Some(&json!({"demo@external": true}))
        );
        write_plugin_state(&mut root, "demo@external", None);
        assert_eq!(root.get("enabledPlugins"), None);
    }

    #[test]
    fn converts_settings_to_runtime_plugin_config() {
        let mut root = Map::<String, Value>::new();
        root.insert(
            "enabledPlugins".to_string(),
            json!({"demo@external": true, "off@bundled": false}),
        );
        let config = config_from_settings(&root);
        assert_eq!(
            config.enabled_plugins().get("demo@external"),
            Some(&true)
        );
        assert_eq!(config.enabled_plugins().get("off@bundled"), Some(&false));
    }
}
