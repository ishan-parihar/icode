use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use runtime::{RuntimeConfig, RuntimeHookConfig};
use serde::{Deserialize, Serialize};

use crate::manifest::{LoadedPlugin, Plugin, PluginHooks, PluginManifest};
use crate::registry::PluginRegistry;
use crate::settings::{read_settings_file, write_plugin_state, write_settings_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginSourceKind {
    Builtin,
    Bundled,
    External,
}

impl PluginSourceKind {
    fn suffix(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Bundled => "bundled",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPluginRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source_kind: PluginSourceKind,
    pub source_path: String,
    pub install_path: String,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginListEntry {
    pub plugin: LoadedPlugin,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginOperationResult {
    pub plugin_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginError {
    message: String,
}

impl PluginError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for PluginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PluginError {}

impl From<String> for PluginError {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<std::io::Error> for PluginError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLoader {
    registry_path: PathBuf,
}

impl PluginLoader {
    #[must_use]
    pub fn new(config_home: impl Into<PathBuf>) -> Self {
        let config_home = config_home.into();
        Self {
            registry_path: config_home.join("plugins").join("installed.json"),
        }
    }

    pub fn discover(&self) -> Result<Vec<LoadedPlugin>, PluginError> {
        let mut plugins = builtin_plugins();
        plugins.extend(bundled_plugins());
        plugins.extend(self.load_external_plugins()?);
        plugins.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(plugins)
    }

    fn load_external_plugins(&self) -> Result<Vec<LoadedPlugin>, PluginError> {
        let registry = PluginRegistry::load(&self.registry_path)?;
        registry
            .plugins
            .into_iter()
            .map(|record| {
                let install_path = PathBuf::from(&record.install_path);
                let (manifest, root) = load_manifest_from_source(&install_path)?;
                Ok(LoadedPlugin::new(
                    record.id,
                    PluginSourceKind::External,
                    manifest,
                    Some(root),
                    Some(PathBuf::from(record.source_path)),
                ))
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManager {
    cwd: PathBuf,
    config_home: PathBuf,
}

impl PluginManager {
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
        let config_home = std::env::var_os("CLAUDE_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")))
            .unwrap_or_else(|| PathBuf::from(".claude"));
        Self { cwd, config_home }
    }

    #[must_use]
    pub fn loader(&self) -> PluginLoader {
        PluginLoader::new(&self.config_home)
    }

    pub fn discover_plugins(&self) -> Result<Vec<LoadedPlugin>, PluginError> {
        self.loader().discover()
    }

    pub fn list_plugins(
        &self,
        runtime_config: &RuntimeConfig,
    ) -> Result<Vec<PluginListEntry>, PluginError> {
        self.discover_plugins().map(|plugins| {
            plugins
                .into_iter()
                .map(|plugin| {
                    let enabled = is_plugin_enabled(&plugin, runtime_config);
                    PluginListEntry { plugin, enabled }
                })
                .collect()
        })
    }

    pub fn active_hook_config(
        &self,
        runtime_config: &RuntimeConfig,
    ) -> Result<RuntimeHookConfig, PluginError> {
        let mut hooks = PluginHooks::default();
        for plugin in self.list_plugins(runtime_config)? {
            if plugin.enabled {
                let resolved = plugin.plugin.resolved_hooks();
                hooks.pre_tool_use.extend(resolved.pre_tool_use);
                hooks.post_tool_use.extend(resolved.post_tool_use);
            }
        }
        Ok(RuntimeHookConfig::new(hooks.pre_tool_use, hooks.post_tool_use))
    }

    pub fn validate_plugin(&self, source: impl AsRef<Path>) -> Result<PluginManifest, PluginError> {
        let (manifest, _) = load_manifest_from_source(source.as_ref())?;
        Ok(manifest)
    }

    pub fn install_plugin(
        &self,
        source: impl AsRef<Path>,
    ) -> Result<PluginOperationResult, PluginError> {
        let (manifest, root) = load_manifest_from_source(source.as_ref())?;
        let plugin_id = external_plugin_id(&manifest.name);
        let install_path = self.installs_root().join(sanitize_plugin_id(&plugin_id));
        let canonical_source = fs::canonicalize(root)?;

        copy_dir_recursive(&canonical_source, &install_path)?;

        let now = iso8601_now();
        let mut registry = self.load_registry()?;
        let installed_at = registry
            .find(&plugin_id)
            .map(|record| record.installed_at.clone())
            .unwrap_or_else(|| now.clone());
        registry.upsert(InstalledPluginRecord {
            id: plugin_id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            source_kind: PluginSourceKind::External,
            source_path: canonical_source.display().to_string(),
            install_path: install_path.display().to_string(),
            installed_at,
            updated_at: now,
        });
        self.save_registry(&registry)?;
        self.write_enabled_state(&plugin_id, Some(true))?;

        Ok(PluginOperationResult {
            plugin_id: plugin_id.clone(),
            message: format!(
                "Installed plugin {} from {}",
                plugin_id,
                canonical_source.display()
            ),
        })
    }

    pub fn enable_plugin(&self, plugin_ref: &str) -> Result<PluginOperationResult, PluginError> {
        let plugin = self.resolve_plugin(plugin_ref)?;
        self.write_enabled_state(plugin.id(), Some(true))?;
        Ok(PluginOperationResult {
            plugin_id: plugin.id().to_string(),
            message: format!("Enabled plugin {}", plugin.id()),
        })
    }

    pub fn disable_plugin(&self, plugin_ref: &str) -> Result<PluginOperationResult, PluginError> {
        let plugin = self.resolve_plugin(plugin_ref)?;
        self.write_enabled_state(plugin.id(), Some(false))?;
        Ok(PluginOperationResult {
            plugin_id: plugin.id().to_string(),
            message: format!("Disabled plugin {}", plugin.id()),
        })
    }

    pub fn uninstall_plugin(
        &self,
        plugin_ref: &str,
    ) -> Result<PluginOperationResult, PluginError> {
        let plugin = self.resolve_plugin(plugin_ref)?;
        if plugin.source_kind != PluginSourceKind::External {
            return Err(PluginError::new(format!(
                "plugin {} is {} and cannot be uninstalled",
                plugin.id(),
                plugin.source_kind.suffix()
            )));
        }

        let mut registry = self.load_registry()?;
        let Some(record) = registry.remove(plugin.id()) else {
            return Err(PluginError::new(format!(
                "plugin {} is not installed",
                plugin.id()
            )));
        };
        self.save_registry(&registry)?;
        self.write_enabled_state(plugin.id(), None)?;

        let install_path = PathBuf::from(record.install_path);
        if install_path.exists() {
            fs::remove_dir_all(install_path)?;
        }

        Ok(PluginOperationResult {
            plugin_id: plugin.id().to_string(),
            message: format!("Uninstalled plugin {}", plugin.id()),
        })
    }

    pub fn update_plugin(&self, plugin_ref: &str) -> Result<PluginOperationResult, PluginError> {
        let plugin = self.resolve_plugin(plugin_ref)?;
        match plugin.source_kind {
            PluginSourceKind::Builtin | PluginSourceKind::Bundled => Ok(PluginOperationResult {
                plugin_id: plugin.id().to_string(),
                message: format!(
                    "Plugin {} is {} and already managed by the CLI",
                    plugin.id(),
                    plugin.source_kind.suffix()
                ),
            }),
            PluginSourceKind::External => {
                let registry = self.load_registry()?;
                let record = registry.find(plugin.id()).ok_or_else(|| {
                    PluginError::new(format!("plugin {} is not installed", plugin.id()))
                })?;
                self.install_plugin(PathBuf::from(&record.source_path)).map(|_| PluginOperationResult {
                    plugin_id: plugin.id().to_string(),
                    message: format!("Updated plugin {}", plugin.id()),
                })
            }
        }
    }

    fn resolve_plugin(&self, plugin_ref: &str) -> Result<LoadedPlugin, PluginError> {
        let plugins = self.discover_plugins()?;
        if let Some(plugin) = plugins.iter().find(|plugin| plugin.id == plugin_ref) {
            return Ok(plugin.clone());
        }
        let mut matches = plugins
            .into_iter()
            .filter(|plugin| plugin.name() == plugin_ref)
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Err(PluginError::new(format!("plugin {plugin_ref} was not found"))),
            1 => Ok(matches.remove(0)),
            _ => Err(PluginError::new(format!(
                "plugin name {plugin_ref} is ambiguous; use a full plugin id"
            ))),
        }
    }

    fn settings_path(&self) -> PathBuf {
        let _ = &self.cwd;
        self.config_home.join("settings.json")
    }

    fn installs_root(&self) -> PathBuf {
        self.config_home.join("plugins").join("installs")
    }

    fn registry_path(&self) -> PathBuf {
        self.config_home.join("plugins").join("installed.json")
    }

    fn load_registry(&self) -> Result<PluginRegistry, PluginError> {
        PluginRegistry::load(&self.registry_path()).map_err(PluginError::from)
    }

    fn save_registry(&self, registry: &PluginRegistry) -> Result<(), PluginError> {
        registry.save(&self.registry_path()).map_err(PluginError::from)
    }

    fn write_enabled_state(
        &self,
        plugin_id: &str,
        enabled: Option<bool>,
    ) -> Result<(), PluginError> {
        let settings_path = self.settings_path();
        let mut settings = read_settings_file(&settings_path)?;
        write_plugin_state(&mut settings, plugin_id, enabled);
        write_settings_file(&settings_path, &settings)?;
        Ok(())
    }
}

fn builtin_plugins() -> Vec<LoadedPlugin> {
    let manifest = PluginManifest {
        name: "tool-guard".to_string(),
        description: "Example built-in plugin with optional tool hook messages".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        default_enabled: false,
        hooks: PluginHooks {
            pre_tool_use: vec!["printf 'builtin tool-guard saw %s' \"$HOOK_TOOL_NAME\"".to_string()],
            post_tool_use: Vec::new(),
        },
    };
    vec![LoadedPlugin::new(
        format!("{}@builtin", manifest.name),
        PluginSourceKind::Builtin,
        manifest,
        None,
        None,
    )]
}

fn bundled_plugins() -> Vec<LoadedPlugin> {
    let manifest = PluginManifest {
        name: "tool-audit".to_string(),
        description: "Example bundled plugin with optional post-tool hooks".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        default_enabled: false,
        hooks: PluginHooks {
            pre_tool_use: Vec::new(),
            post_tool_use: vec!["printf 'bundled tool-audit saw %s' \"$HOOK_TOOL_NAME\"".to_string()],
        },
    };
    vec![LoadedPlugin::new(
        format!("{}@bundled", manifest.name),
        PluginSourceKind::Bundled,
        manifest,
        None,
        None,
    )]
}

fn is_plugin_enabled(plugin: &LoadedPlugin, runtime_config: &RuntimeConfig) -> bool {
    runtime_config.plugins().state_for(&plugin.id, plugin.manifest.default_enabled)
}

fn external_plugin_id(name: &str) -> String {
    format!("{}@external", name.trim())
}

fn sanitize_plugin_id(plugin_id: &str) -> String {
    plugin_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn load_manifest_from_source(source: &Path) -> Result<(PluginManifest, PathBuf), PluginError> {
    let (manifest_path, root) = resolve_manifest_path(source)?;
    let contents = fs::read_to_string(&manifest_path).map_err(|error| {
        PluginError::new(format!(
            "failed to read plugin manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: PluginManifest = serde_json::from_str(&contents).map_err(|error| {
        PluginError::new(format!(
            "failed to parse plugin manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    manifest.validate().map_err(PluginError::new)?;
    Ok((manifest, root))
}

fn resolve_manifest_path(source: &Path) -> Result<(PathBuf, PathBuf), PluginError> {
    if source.is_file() {
        let file_name = source.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if file_name != "plugin.json" {
            return Err(PluginError::new(format!(
                "plugin manifest file must be named plugin.json: {}",
                source.display()
            )));
        }
        let root = source
            .parent()
            .and_then(|parent| parent.parent().filter(|candidate| parent.file_name() == Some(std::ffi::OsStr::new(".claude-plugin"))))
            .map_or_else(
                || source.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
                Path::to_path_buf,
            );
        return Ok((source.to_path_buf(), root));
    }

    let nested = source.join(".claude-plugin").join("plugin.json");
    if nested.exists() {
        return Ok((nested, source.to_path_buf()));
    }

    let direct = source.join("plugin.json");
    if direct.exists() {
        return Ok((direct, source.to_path_buf()));
    }

    Err(PluginError::new(format!(
        "plugin manifest not found in {}",
        source.display()
    )))
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), PluginError> {
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn iso8601_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{seconds}")
}

#[cfg(test)]
mod tests {
    use super::{PluginLoader, PluginManager, PluginSourceKind};
    use runtime::ConfigLoader;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("plugins-manager-{nanos}"))
    }

    fn write_external_plugin(root: &Path, version: &str, hook_body: &str) {
        fs::create_dir_all(root.join(".claude-plugin")).expect("plugin dir should exist");
        fs::write(
            root.join(".claude-plugin").join("plugin.json"),
            format!(
                r#"{{
  "name": "sample-plugin",
  "description": "sample external plugin",
  "version": "{version}",
  "hooks": {{
    "PreToolUse": ["printf 'pre from ${PLUGIN_DIR} {hook_body}'"]
  }}
}}"#
            ),
        )
        .expect("plugin manifest should write");
        fs::write(root.join("README.md"), "sample").expect("payload should write");
    }

    #[test]
    fn discovers_builtin_and_bundled_plugins() {
        let root = temp_dir();
        let home = root.join("home").join(".claude");
        let loader = PluginLoader::new(&home);
        let plugins = loader.discover().expect("plugins should load");
        assert!(plugins.iter().any(|plugin| plugin.source_kind == PluginSourceKind::Builtin));
        assert!(plugins.iter().any(|plugin| plugin.source_kind == PluginSourceKind::Bundled));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn installs_and_lists_external_plugins() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".claude");
        let source = root.join("source-plugin");
        fs::create_dir_all(&cwd).expect("cwd should exist");
        write_external_plugin(&source, "1.0.0", "v1");

        let manager = PluginManager::new(&cwd, &home);
        let result = manager.install_plugin(&source).expect("install should succeed");
        assert_eq!(result.plugin_id, "sample-plugin@external");

        let runtime_config = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");
        let plugins = manager
            .list_plugins(&runtime_config)
            .expect("plugins should list");
        let external = plugins
            .iter()
            .find(|plugin| plugin.plugin.id == "sample-plugin@external")
            .expect("external plugin should exist");
        assert!(external.enabled);

        let hook_config = manager
            .active_hook_config(&runtime_config)
            .expect("hook config should build");
        assert_eq!(hook_config.pre_tool_use().len(), 1);
        assert!(hook_config.pre_tool_use()[0].contains("sample-plugin-external"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn disables_enables_updates_and_uninstalls_external_plugins() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".claude");
        let source = root.join("source-plugin");
        fs::create_dir_all(&cwd).expect("cwd should exist");
        write_external_plugin(&source, "1.0.0", "v1");

        let manager = PluginManager::new(&cwd, &home);
        manager.install_plugin(&source).expect("install should succeed");
        manager
            .disable_plugin("sample-plugin")
            .expect("disable should succeed");
        let runtime_config = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");
        let plugins = manager
            .list_plugins(&runtime_config)
            .expect("plugins should list");
        assert!(!plugins
            .iter()
            .find(|plugin| plugin.plugin.id == "sample-plugin@external")
            .expect("external plugin should exist")
            .enabled);

        manager
            .enable_plugin("sample-plugin@external")
            .expect("enable should succeed");
        write_external_plugin(&source, "2.0.0", "v2");
        manager
            .update_plugin("sample-plugin@external")
            .expect("update should succeed");

        let loader = PluginLoader::new(&home);
        let plugins = loader.discover().expect("plugins should load");
        let external = plugins
            .iter()
            .find(|plugin| plugin.id == "sample-plugin@external")
            .expect("external plugin should exist");
        assert_eq!(external.manifest.version, "2.0.0");

        manager
            .uninstall_plugin("sample-plugin@external")
            .expect("uninstall should succeed");
        let plugins = loader.discover().expect("plugins should reload");
        assert!(!plugins
            .iter()
            .any(|plugin| plugin.id == "sample-plugin@external"));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
