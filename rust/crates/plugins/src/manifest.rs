use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::PluginSourceKind;

pub trait Plugin {
    fn id(&self) -> &str;
    fn manifest(&self) -> &PluginManifest;
    fn source_kind(&self) -> PluginSourceKind;
    fn root(&self) -> Option<&Path>;

    fn resolved_hooks(&self) -> PluginHooks {
        self.manifest().hooks.resolve(self.root())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginHooks {
    #[serde(rename = "PreToolUse", alias = "preToolUse", default)]
    pub pre_tool_use: Vec<String>,
    #[serde(rename = "PostToolUse", alias = "postToolUse", default)]
    pub post_tool_use: Vec<String>,
}

impl PluginHooks {
    #[must_use]
    pub fn resolve(&self, root: Option<&Path>) -> Self {
        let Some(root) = root else {
            return self.clone();
        };
        let replacement = root.display().to_string();
        Self {
            pre_tool_use: self
                .pre_tool_use
                .iter()
                .map(|value| value.replace("${PLUGIN_DIR}", &replacement))
                .collect(),
            post_tool_use: self
                .post_tool_use
                .iter()
                .map(|value| value.replace("${PLUGIN_DIR}", &replacement))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub default_enabled: bool,
    #[serde(default)]
    pub hooks: PluginHooks,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("plugin manifest name must not be empty".to_string());
        }
        if self.description.trim().is_empty() {
            return Err(format!(
                "plugin manifest description must not be empty for {}",
                self.name
            ));
        }
        if self.version.trim().is_empty() {
            return Err(format!(
                "plugin manifest version must not be empty for {}",
                self.name
            ));
        }
        if self
            .hooks
            .pre_tool_use
            .iter()
            .chain(self.hooks.post_tool_use.iter())
            .any(|hook| hook.trim().is_empty())
        {
            return Err(format!(
                "plugin manifest hook entries must not be empty for {}",
                self.name
            ));
        }
        Ok(())
    }
}

fn default_version() -> String {
    "0.1.0".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPlugin {
    pub id: String,
    pub source_kind: PluginSourceKind,
    pub manifest: PluginManifest,
    pub root: Option<PathBuf>,
    pub origin: Option<PathBuf>,
}

impl LoadedPlugin {
    #[must_use]
    pub fn new(
        id: String,
        source_kind: PluginSourceKind,
        manifest: PluginManifest,
        root: Option<PathBuf>,
        origin: Option<PathBuf>,
    ) -> Self {
        Self {
            id,
            source_kind,
            manifest,
            root,
            origin,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.manifest.name
    }
}

impl Plugin for LoadedPlugin {
    fn id(&self) -> &str {
        &self.id
    }

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn source_kind(&self) -> PluginSourceKind {
        self.source_kind
    }

    fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::{PluginHooks, PluginManifest};
    use std::path::Path;

    #[test]
    fn validates_manifest_fields() {
        let manifest = PluginManifest {
            name: "demo".to_string(),
            description: "demo plugin".to_string(),
            version: "1.2.3".to_string(),
            default_enabled: false,
            hooks: PluginHooks::default(),
        };
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn resolves_plugin_dir_placeholders() {
        let hooks = PluginHooks {
            pre_tool_use: vec!["echo ${PLUGIN_DIR}/pre".to_string()],
            post_tool_use: vec!["echo ${PLUGIN_DIR}/post".to_string()],
        };
        let resolved = hooks.resolve(Some(Path::new("/tmp/plugin")));
        assert_eq!(resolved.pre_tool_use, vec!["echo /tmp/plugin/pre"]);
        assert_eq!(resolved.post_tool_use, vec!["echo /tmp/plugin/post"]);
    }
}
