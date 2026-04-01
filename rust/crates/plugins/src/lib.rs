use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const EXTERNAL_MARKETPLACE: &str = "external";
const BUILTIN_MARKETPLACE: &str = "builtin";
const BUNDLED_MARKETPLACE: &str = "bundled";
const SETTINGS_FILE_NAME: &str = "settings.json";
const REGISTRY_FILE_NAME: &str = "installed.json";
const MANIFEST_RELATIVE_PATH: &str = ".claude-plugin/plugin.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    Builtin,
    Bundled,
    External,
}

impl Display for PluginKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin => write!(f, "builtin"),
            Self::Bundled => write!(f, "bundled"),
            Self::External => write!(f, "external"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: PluginKind,
    pub source: String,
    pub default_enabled: bool,
    pub root: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHooks {
    #[serde(rename = "PreToolUse", default)]
    pub pre_tool_use: Vec<String>,
    #[serde(rename = "PostToolUse", default)]
    pub post_tool_use: Vec<String>,
}

impl PluginHooks {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pre_tool_use.is_empty() && self.post_tool_use.is_empty()
    }

    #[must_use]
    pub fn merged_with(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        merged
            .pre_tool_use
            .extend(other.pre_tool_use.iter().cloned());
        merged
            .post_tool_use
            .extend(other.post_tool_use.iter().cloned());
        merged
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(rename = "defaultEnabled", default)]
    pub default_enabled: bool,
    #[serde(default)]
    pub hooks: PluginHooks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginInstallSource {
    LocalPath { path: PathBuf },
    GitUrl { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPluginRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub install_path: PathBuf,
    pub source: PluginInstallSource,
    pub installed_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPluginRegistry {
    #[serde(default)]
    pub plugins: BTreeMap<String, InstalledPluginRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinPlugin {
    metadata: PluginMetadata,
    hooks: PluginHooks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledPlugin {
    metadata: PluginMetadata,
    hooks: PluginHooks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalPlugin {
    metadata: PluginMetadata,
    hooks: PluginHooks,
}

pub trait Plugin {
    fn metadata(&self) -> &PluginMetadata;
    fn hooks(&self) -> &PluginHooks;
    fn validate(&self) -> Result<(), PluginError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginDefinition {
    Builtin(BuiltinPlugin),
    Bundled(BundledPlugin),
    External(ExternalPlugin),
}

impl Plugin for BuiltinPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn hooks(&self) -> &PluginHooks {
        &self.hooks
    }

    fn validate(&self) -> Result<(), PluginError> {
        Ok(())
    }
}

impl Plugin for BundledPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn hooks(&self) -> &PluginHooks {
        &self.hooks
    }

    fn validate(&self) -> Result<(), PluginError> {
        validate_hook_paths(self.metadata.root.as_deref(), &self.hooks)
    }
}

impl Plugin for ExternalPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn hooks(&self) -> &PluginHooks {
        &self.hooks
    }

    fn validate(&self) -> Result<(), PluginError> {
        validate_hook_paths(self.metadata.root.as_deref(), &self.hooks)
    }
}

impl Plugin for PluginDefinition {
    fn metadata(&self) -> &PluginMetadata {
        match self {
            Self::Builtin(plugin) => plugin.metadata(),
            Self::Bundled(plugin) => plugin.metadata(),
            Self::External(plugin) => plugin.metadata(),
        }
    }

    fn hooks(&self) -> &PluginHooks {
        match self {
            Self::Builtin(plugin) => plugin.hooks(),
            Self::Bundled(plugin) => plugin.hooks(),
            Self::External(plugin) => plugin.hooks(),
        }
    }

    fn validate(&self) -> Result<(), PluginError> {
        match self {
            Self::Builtin(plugin) => plugin.validate(),
            Self::Bundled(plugin) => plugin.validate(),
            Self::External(plugin) => plugin.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSummary {
    pub metadata: PluginMetadata,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManagerConfig {
    pub config_home: PathBuf,
    pub enabled_plugins: BTreeMap<String, bool>,
    pub external_dirs: Vec<PathBuf>,
    pub install_root: Option<PathBuf>,
    pub registry_path: Option<PathBuf>,
    pub bundled_root: Option<PathBuf>,
}

impl PluginManagerConfig {
    #[must_use]
    pub fn new(config_home: impl Into<PathBuf>) -> Self {
        Self {
            config_home: config_home.into(),
            enabled_plugins: BTreeMap::new(),
            external_dirs: Vec::new(),
            install_root: None,
            registry_path: None,
            bundled_root: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManager {
    config: PluginManagerConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub plugin_id: String,
    pub version: String,
    pub install_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOutcome {
    pub plugin_id: String,
    pub old_version: String,
    pub new_version: String,
    pub install_path: PathBuf,
}

#[derive(Debug)]
pub enum PluginError {
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidManifest(String),
    NotFound(String),
    CommandFailed(String),
}

impl Display for PluginError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::InvalidManifest(message)
            | Self::NotFound(message)
            | Self::CommandFailed(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<std::io::Error> for PluginError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl PluginManager {
    #[must_use]
    pub fn new(config: PluginManagerConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn bundled_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bundled")
    }

    #[must_use]
    pub fn install_root(&self) -> PathBuf {
        self.config
            .install_root
            .clone()
            .unwrap_or_else(|| self.config.config_home.join("plugins").join("installed"))
    }

    #[must_use]
    pub fn registry_path(&self) -> PathBuf {
        self.config.registry_path.clone().unwrap_or_else(|| {
            self.config
                .config_home
                .join("plugins")
                .join(REGISTRY_FILE_NAME)
        })
    }

    #[must_use]
    pub fn settings_path(&self) -> PathBuf {
        self.config.config_home.join(SETTINGS_FILE_NAME)
    }

    pub fn list_plugins(&self) -> Result<Vec<PluginSummary>, PluginError> {
        let mut plugins = self
            .discover_plugins()?
            .into_iter()
            .map(|plugin| PluginSummary {
                enabled: self.is_enabled(plugin.metadata()),
                metadata: plugin.metadata().clone(),
            })
            .collect::<Vec<_>>();
        plugins.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
        Ok(plugins)
    }

    pub fn discover_plugins(&self) -> Result<Vec<PluginDefinition>, PluginError> {
        let mut plugins = builtin_plugins();
        plugins.extend(self.discover_bundled_plugins()?);
        plugins.extend(self.discover_external_plugins()?);
        Ok(plugins)
    }

    pub fn aggregated_hooks(&self) -> Result<PluginHooks, PluginError> {
        self.discover_plugins()?
            .into_iter()
            .filter(|plugin| self.is_enabled(plugin.metadata()))
            .try_fold(PluginHooks::default(), |acc, plugin| {
                plugin.validate()?;
                Ok(acc.merged_with(plugin.hooks()))
            })
    }

    pub fn validate_plugin_source(&self, source: &str) -> Result<PluginManifest, PluginError> {
        let path = resolve_local_source(source)?;
        load_manifest_from_root(&path)
    }

    pub fn install(&mut self, source: &str) -> Result<InstallOutcome, PluginError> {
        let install_source = parse_install_source(source)?;
        let temp_root = self.install_root().join(".tmp");
        let staged_source = materialize_source(&install_source, &temp_root)?;
        let cleanup_source = matches!(install_source, PluginInstallSource::GitUrl { .. });
        let manifest = load_manifest_from_root(&staged_source)?;
        validate_manifest(&manifest)?;

        let plugin_id = plugin_id(&manifest.name, EXTERNAL_MARKETPLACE);
        let install_path = self.install_root().join(sanitize_plugin_id(&plugin_id));
        if install_path.exists() {
            fs::remove_dir_all(&install_path)?;
        }
        copy_dir_all(&staged_source, &install_path)?;
        if cleanup_source {
            let _ = fs::remove_dir_all(&staged_source);
        }

        let now = unix_time_ms();
        let record = InstalledPluginRecord {
            id: plugin_id.clone(),
            name: manifest.name,
            version: manifest.version.clone(),
            description: manifest.description,
            install_path: install_path.clone(),
            source: install_source,
            installed_at_unix_ms: now,
            updated_at_unix_ms: now,
        };

        let mut registry = self.load_registry()?;
        registry.plugins.insert(plugin_id.clone(), record);
        self.store_registry(&registry)?;
        self.write_enabled_state(&plugin_id, Some(true))?;
        self.config.enabled_plugins.insert(plugin_id.clone(), true);

        Ok(InstallOutcome {
            plugin_id,
            version: manifest.version,
            install_path,
        })
    }

    pub fn enable(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        self.ensure_known_plugin(plugin_id)?;
        self.write_enabled_state(plugin_id, Some(true))?;
        self.config
            .enabled_plugins
            .insert(plugin_id.to_string(), true);
        Ok(())
    }

    pub fn disable(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        self.ensure_known_plugin(plugin_id)?;
        self.write_enabled_state(plugin_id, Some(false))?;
        self.config
            .enabled_plugins
            .insert(plugin_id.to_string(), false);
        Ok(())
    }

    pub fn uninstall(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        let mut registry = self.load_registry()?;
        let record = registry.plugins.remove(plugin_id).ok_or_else(|| {
            PluginError::NotFound(format!("plugin `{plugin_id}` is not installed"))
        })?;
        if record.install_path.exists() {
            fs::remove_dir_all(&record.install_path)?;
        }
        self.store_registry(&registry)?;
        self.write_enabled_state(plugin_id, None)?;
        self.config.enabled_plugins.remove(plugin_id);
        Ok(())
    }

    pub fn update(&mut self, plugin_id: &str) -> Result<UpdateOutcome, PluginError> {
        let mut registry = self.load_registry()?;
        let record = registry.plugins.get(plugin_id).cloned().ok_or_else(|| {
            PluginError::NotFound(format!("plugin `{plugin_id}` is not installed"))
        })?;

        let temp_root = self.install_root().join(".tmp");
        let staged_source = materialize_source(&record.source, &temp_root)?;
        let cleanup_source = matches!(record.source, PluginInstallSource::GitUrl { .. });
        let manifest = load_manifest_from_root(&staged_source)?;
        validate_manifest(&manifest)?;

        if record.install_path.exists() {
            fs::remove_dir_all(&record.install_path)?;
        }
        copy_dir_all(&staged_source, &record.install_path)?;
        if cleanup_source {
            let _ = fs::remove_dir_all(&staged_source);
        }

        let updated_record = InstalledPluginRecord {
            version: manifest.version.clone(),
            description: manifest.description,
            updated_at_unix_ms: unix_time_ms(),
            ..record.clone()
        };
        registry
            .plugins
            .insert(plugin_id.to_string(), updated_record);
        self.store_registry(&registry)?;

        Ok(UpdateOutcome {
            plugin_id: plugin_id.to_string(),
            old_version: record.version,
            new_version: manifest.version,
            install_path: record.install_path,
        })
    }

    fn discover_bundled_plugins(&self) -> Result<Vec<PluginDefinition>, PluginError> {
        discover_plugin_dirs(
            &self
                .config
                .bundled_root
                .clone()
                .unwrap_or_else(Self::bundled_root),
        )?
        .into_iter()
        .map(|root| {
            load_plugin_definition(
                &root,
                PluginKind::Bundled,
                format!("{BUNDLED_MARKETPLACE}:{}", root.display()),
                BUNDLED_MARKETPLACE,
            )
        })
        .collect()
    }

    fn discover_external_plugins(&self) -> Result<Vec<PluginDefinition>, PluginError> {
        let registry = self.load_registry()?;
        let mut plugins = registry
            .plugins
            .values()
            .map(|record| {
                load_plugin_definition(
                    &record.install_path,
                    PluginKind::External,
                    describe_install_source(&record.source),
                    EXTERNAL_MARKETPLACE,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        for directory in &self.config.external_dirs {
            for root in discover_plugin_dirs(directory)? {
                let plugin = load_plugin_definition(
                    &root,
                    PluginKind::External,
                    root.display().to_string(),
                    EXTERNAL_MARKETPLACE,
                )?;
                if plugins
                    .iter()
                    .all(|existing| existing.metadata().id != plugin.metadata().id)
                {
                    plugins.push(plugin);
                }
            }
        }

        Ok(plugins)
    }

    fn is_enabled(&self, metadata: &PluginMetadata) -> bool {
        self.config
            .enabled_plugins
            .get(&metadata.id)
            .copied()
            .unwrap_or(match metadata.kind {
                PluginKind::External => false,
                PluginKind::Builtin | PluginKind::Bundled => metadata.default_enabled,
            })
    }

    fn ensure_known_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        if self
            .list_plugins()?
            .iter()
            .any(|plugin| plugin.metadata.id == plugin_id)
        {
            Ok(())
        } else {
            Err(PluginError::NotFound(format!(
                "plugin `{plugin_id}` is not installed or discoverable"
            )))
        }
    }

    fn load_registry(&self) -> Result<InstalledPluginRegistry, PluginError> {
        let path = self.registry_path();
        match fs::read_to_string(&path) {
            Ok(contents) => Ok(serde_json::from_str(&contents)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(InstalledPluginRegistry::default())
            }
            Err(error) => Err(PluginError::Io(error)),
        }
    }

    fn store_registry(&self, registry: &InstalledPluginRegistry) -> Result<(), PluginError> {
        let path = self.registry_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(registry)?)?;
        Ok(())
    }

    fn write_enabled_state(
        &self,
        plugin_id: &str,
        enabled: Option<bool>,
    ) -> Result<(), PluginError> {
        update_settings_json(&self.settings_path(), |root| {
            let enabled_plugins = ensure_object(root, "enabledPlugins");
            match enabled {
                Some(value) => {
                    enabled_plugins.insert(plugin_id.to_string(), Value::Bool(value));
                }
                None => {
                    enabled_plugins.remove(plugin_id);
                }
            }
        })
    }
}

#[must_use]
pub fn builtin_plugins() -> Vec<PluginDefinition> {
    vec![PluginDefinition::Builtin(BuiltinPlugin {
        metadata: PluginMetadata {
            id: plugin_id("example-builtin", BUILTIN_MARKETPLACE),
            name: "example-builtin".to_string(),
            version: "0.1.0".to_string(),
            description: "Example built-in plugin scaffold for the Rust plugin system".to_string(),
            kind: PluginKind::Builtin,
            source: BUILTIN_MARKETPLACE.to_string(),
            default_enabled: false,
            root: None,
        },
        hooks: PluginHooks::default(),
    })]
}

fn load_plugin_definition(
    root: &Path,
    kind: PluginKind,
    source: String,
    marketplace: &str,
) -> Result<PluginDefinition, PluginError> {
    let manifest = load_manifest_from_root(root)?;
    validate_manifest(&manifest)?;
    let metadata = PluginMetadata {
        id: plugin_id(&manifest.name, marketplace),
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        kind,
        source,
        default_enabled: manifest.default_enabled,
        root: Some(root.to_path_buf()),
    };
    let hooks = resolve_hooks(root, &manifest.hooks);
    Ok(match kind {
        PluginKind::Builtin => PluginDefinition::Builtin(BuiltinPlugin { metadata, hooks }),
        PluginKind::Bundled => PluginDefinition::Bundled(BundledPlugin { metadata, hooks }),
        PluginKind::External => PluginDefinition::External(ExternalPlugin { metadata, hooks }),
    })
}

fn validate_manifest(manifest: &PluginManifest) -> Result<(), PluginError> {
    if manifest.name.trim().is_empty() {
        return Err(PluginError::InvalidManifest(
            "plugin manifest name cannot be empty".to_string(),
        ));
    }
    if manifest.version.trim().is_empty() {
        return Err(PluginError::InvalidManifest(
            "plugin manifest version cannot be empty".to_string(),
        ));
    }
    if manifest.description.trim().is_empty() {
        return Err(PluginError::InvalidManifest(
            "plugin manifest description cannot be empty".to_string(),
        ));
    }
    Ok(())
}

fn load_manifest_from_root(root: &Path) -> Result<PluginManifest, PluginError> {
    let manifest_path = root.join(MANIFEST_RELATIVE_PATH);
    let contents = fs::read_to_string(&manifest_path).map_err(|error| {
        PluginError::NotFound(format!(
            "plugin manifest not found at {}: {error}",
            manifest_path.display()
        ))
    })?;
    Ok(serde_json::from_str(&contents)?)
}

fn resolve_hooks(root: &Path, hooks: &PluginHooks) -> PluginHooks {
    PluginHooks {
        pre_tool_use: hooks
            .pre_tool_use
            .iter()
            .map(|entry| resolve_hook_entry(root, entry))
            .collect(),
        post_tool_use: hooks
            .post_tool_use
            .iter()
            .map(|entry| resolve_hook_entry(root, entry))
            .collect(),
    }
}

fn validate_hook_paths(root: Option<&Path>, hooks: &PluginHooks) -> Result<(), PluginError> {
    let Some(root) = root else {
        return Ok(());
    };
    for entry in hooks.pre_tool_use.iter().chain(hooks.post_tool_use.iter()) {
        if is_literal_command(entry) {
            continue;
        }
        let path = if Path::new(entry).is_absolute() {
            PathBuf::from(entry)
        } else {
            root.join(entry)
        };
        if !path.exists() {
            return Err(PluginError::InvalidManifest(format!(
                "hook path `{}` does not exist",
                path.display()
            )));
        }
    }
    Ok(())
}

fn resolve_hook_entry(root: &Path, entry: &str) -> String {
    if is_literal_command(entry) {
        entry.to_string()
    } else {
        root.join(entry).display().to_string()
    }
}

fn is_literal_command(entry: &str) -> bool {
    !entry.starts_with("./") && !entry.starts_with("../")
}

fn resolve_local_source(source: &str) -> Result<PathBuf, PluginError> {
    let path = PathBuf::from(source);
    if path.exists() {
        Ok(path)
    } else {
        Err(PluginError::NotFound(format!(
            "plugin source `{source}` was not found"
        )))
    }
}

fn parse_install_source(source: &str) -> Result<PluginInstallSource, PluginError> {
    if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@")
        || source.ends_with(".git")
    {
        Ok(PluginInstallSource::GitUrl {
            url: source.to_string(),
        })
    } else {
        Ok(PluginInstallSource::LocalPath {
            path: resolve_local_source(source)?,
        })
    }
}

fn materialize_source(
    source: &PluginInstallSource,
    temp_root: &Path,
) -> Result<PathBuf, PluginError> {
    fs::create_dir_all(temp_root)?;
    match source {
        PluginInstallSource::LocalPath { path } => Ok(path.clone()),
        PluginInstallSource::GitUrl { url } => {
            let destination = temp_root.join(format!("plugin-{}", unix_time_ms()));
            let output = Command::new("git")
                .arg("clone")
                .arg("--depth")
                .arg("1")
                .arg(url)
                .arg(&destination)
                .output()?;
            if !output.status.success() {
                return Err(PluginError::CommandFailed(format!(
                    "git clone failed for `{url}`: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            Ok(destination)
        }
    }
}

fn discover_plugin_dirs(root: &Path) -> Result<Vec<PathBuf>, PluginError> {
    match fs::read_dir(root) {
        Ok(entries) => {
            let mut paths = Vec::new();
            for entry in entries {
                let path = entry?.path();
                if path.join(MANIFEST_RELATIVE_PATH).exists() {
                    paths.push(path);
                }
            }
            Ok(paths)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(PluginError::Io(error)),
    }
}

fn plugin_id(name: &str, marketplace: &str) -> String {
    format!("{name}@{marketplace}")
}

fn sanitize_plugin_id(plugin_id: &str) -> String {
    plugin_id
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | '@' | ':' => '-',
            other => other,
        })
        .collect()
}

fn describe_install_source(source: &PluginInstallSource) -> String {
    match source {
        PluginInstallSource::LocalPath { path } => path.display().to_string(),
        PluginInstallSource::GitUrl { url } => url.clone(),
    }
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be after epoch")
        .as_millis()
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), PluginError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn update_settings_json(
    path: &Path,
    mut update: impl FnMut(&mut Map<String, Value>),
) -> Result<(), PluginError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut root = match fs::read_to_string(path) {
        Ok(contents) if !contents.trim().is_empty() => serde_json::from_str::<Value>(&contents)?,
        Ok(_) => Value::Object(Map::new()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
        Err(error) => return Err(PluginError::Io(error)),
    };

    let object = root.as_object_mut().ok_or_else(|| {
        PluginError::InvalidManifest(format!(
            "settings file {} must contain a JSON object",
            path.display()
        ))
    })?;
    update(object);
    fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn ensure_object<'a>(root: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), Value::Object(Map::new()));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("object should exist")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("plugins-{label}-{}", unix_time_ms()))
    }

    fn write_external_plugin(root: &Path, name: &str, version: &str) {
        fs::create_dir_all(root.join(".claude-plugin")).expect("manifest dir");
        fs::create_dir_all(root.join("hooks")).expect("hooks dir");
        fs::write(
            root.join("hooks").join("pre.sh"),
            "#!/bin/sh\nprintf 'pre'\n",
        )
        .expect("write pre hook");
        fs::write(
            root.join("hooks").join("post.sh"),
            "#!/bin/sh\nprintf 'post'\n",
        )
        .expect("write post hook");
        fs::write(
            root.join(MANIFEST_RELATIVE_PATH),
            format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"{version}\",\n  \"description\": \"test plugin\",\n  \"hooks\": {{\n    \"PreToolUse\": [\"./hooks/pre.sh\"],\n    \"PostToolUse\": [\"./hooks/post.sh\"]\n  }}\n}}"
            ),
        )
        .expect("write manifest");
    }

    #[test]
    fn validates_manifest_shape() {
        let error = validate_manifest(&PluginManifest {
            name: String::new(),
            version: "1.0.0".to_string(),
            description: "desc".to_string(),
            default_enabled: false,
            hooks: PluginHooks::default(),
        })
        .expect_err("empty name should fail");
        assert!(error.to_string().contains("name cannot be empty"));
    }

    #[test]
    fn discovers_builtin_and_bundled_plugins() {
        let manager = PluginManager::new(PluginManagerConfig::new(temp_dir("discover")));
        let plugins = manager.list_plugins().expect("plugins should list");
        assert!(plugins
            .iter()
            .any(|plugin| plugin.metadata.kind == PluginKind::Builtin));
        assert!(plugins
            .iter()
            .any(|plugin| plugin.metadata.kind == PluginKind::Bundled));
    }

    #[test]
    fn installs_enables_updates_and_uninstalls_external_plugins() {
        let config_home = temp_dir("home");
        let source_root = temp_dir("source");
        write_external_plugin(&source_root, "demo", "1.0.0");

        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let install = manager
            .install(source_root.to_str().expect("utf8 path"))
            .expect("install should succeed");
        assert_eq!(install.plugin_id, "demo@external");
        assert!(manager
            .list_plugins()
            .expect("list plugins")
            .iter()
            .any(|plugin| plugin.metadata.id == "demo@external" && plugin.enabled));

        let hooks = manager.aggregated_hooks().expect("hooks should aggregate");
        assert_eq!(hooks.pre_tool_use.len(), 1);
        assert!(hooks.pre_tool_use[0].contains("pre.sh"));

        manager
            .disable("demo@external")
            .expect("disable should work");
        assert!(manager
            .aggregated_hooks()
            .expect("hooks after disable")
            .is_empty());
        manager.enable("demo@external").expect("enable should work");

        write_external_plugin(&source_root, "demo", "2.0.0");
        let update = manager.update("demo@external").expect("update should work");
        assert_eq!(update.old_version, "1.0.0");
        assert_eq!(update.new_version, "2.0.0");

        manager
            .uninstall("demo@external")
            .expect("uninstall should work");
        assert!(!manager
            .list_plugins()
            .expect("list plugins")
            .iter()
            .any(|plugin| plugin.metadata.id == "demo@external"));

        fs::remove_dir_all(config_home).expect("cleanup home");
        fs::remove_dir_all(source_root).expect("cleanup source");
    }

    #[test]
    fn validates_plugin_source_before_install() {
        let config_home = temp_dir("validate-home");
        let source_root = temp_dir("validate-source");
        write_external_plugin(&source_root, "validator", "1.0.0");
        let manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let manifest = manager
            .validate_plugin_source(source_root.to_str().expect("utf8 path"))
            .expect("manifest should validate");
        assert_eq!(manifest.name, "validator");
        fs::remove_dir_all(config_home).expect("cleanup home");
        fs::remove_dir_all(source_root).expect("cleanup source");
    }
}
