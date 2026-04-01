use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::InstalledPluginRecord;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginRegistry {
    #[serde(default)]
    pub plugins: Vec<InstalledPluginRecord>,
}

impl PluginRegistry {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                if contents.trim().is_empty() {
                    return Ok(Self::default());
                }
                serde_json::from_str(&contents).map_err(|error| error.to_string())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(
            path,
            serde_json::to_string_pretty(self).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }

    #[must_use]
    pub fn find(&self, plugin_id: &str) -> Option<&InstalledPluginRecord> {
        self.plugins.iter().find(|plugin| plugin.id == plugin_id)
    }

    pub fn upsert(&mut self, record: InstalledPluginRecord) {
        if let Some(existing) = self.plugins.iter_mut().find(|plugin| plugin.id == record.id) {
            *existing = record;
        } else {
            self.plugins.push(record);
        }
        self.plugins.sort_by(|left, right| left.id.cmp(&right.id));
    }

    pub fn remove(&mut self, plugin_id: &str) -> Option<InstalledPluginRecord> {
        let index = self.plugins.iter().position(|plugin| plugin.id == plugin_id)?;
        Some(self.plugins.remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::PluginRegistry;
    use crate::{InstalledPluginRecord, PluginSourceKind};

    #[test]
    fn upsert_replaces_existing_entries() {
        let mut registry = PluginRegistry::default();
        registry.upsert(InstalledPluginRecord {
            id: "demo@external".to_string(),
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            description: "demo".to_string(),
            source_kind: PluginSourceKind::External,
            source_path: "/src".to_string(),
            install_path: "/install".to_string(),
            installed_at: "t1".to_string(),
            updated_at: "t1".to_string(),
        });
        registry.upsert(InstalledPluginRecord {
            id: "demo@external".to_string(),
            name: "demo".to_string(),
            version: "1.0.1".to_string(),
            description: "updated".to_string(),
            source_kind: PluginSourceKind::External,
            source_path: "/src".to_string(),
            install_path: "/install".to_string(),
            installed_at: "t1".to_string(),
            updated_at: "t2".to_string(),
        });
        assert_eq!(registry.plugins.len(), 1);
        assert_eq!(registry.plugins[0].version, "1.0.1");
    }
}
