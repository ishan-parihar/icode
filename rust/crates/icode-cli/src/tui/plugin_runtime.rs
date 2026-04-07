use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::tui::app::{icode_config_dir, AppState};
use crate::tui::kv::KvStore;
use crate::tui::plugin::{PluginApi, PluginCommand, PluginInfo, TuiPlugin};
use crate::tui::theme::Theme;

#[derive(Debug, Serialize, Deserialize)]
struct PluginStateFile {
    enabled: HashSet<String>,
}

pub struct PluginRuntime {
    plugins: HashMap<String, Box<dyn TuiPlugin>>,
    enabled: HashSet<String>,
    commands: Vec<PluginCommand>,
    kv: KvStore,
}

impl PluginRuntime {
    pub fn new() -> Self {
        let kv_path = icode_config_dir().join("plugin_kv.json");
        Self {
            plugins: HashMap::new(),
            enabled: HashSet::new(),
            commands: Vec::new(),
            kv: KvStore::new(kv_path),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn TuiPlugin>) {
        let id = plugin.id().to_string();
        self.plugins.insert(id, plugin);
    }

    pub fn enable(&mut self, id: &str, state: &mut AppState, theme: &Theme) -> Result<(), String> {
        let plugin = self
            .plugins
            .get(id)
            .ok_or_else(|| format!("plugin not found: {id}"))?;

        if self.enabled.contains(id) {
            return Ok(());
        }

        let mut api = PluginApi {
            state,
            kv: &mut self.kv,
            theme,
        };
        plugin.init(&mut api)?;
        plugin.register_slots(&mut api);
        plugin.register_routes(&mut api);

        let plugin = self
            .plugins
            .get(id)
            .ok_or_else(|| format!("plugin not found: {id}"))?;
        let cmds = plugin.register_commands();
        self.commands.extend(cmds);

        self.enabled.insert(id.to_string());
        let _ = self.save_state();
        Ok(())
    }

    pub fn disable(&mut self, id: &str, state: &mut AppState, theme: &Theme) {
        let Some(plugin) = self.plugins.get(id) else {
            return;
        };

        let mut api = PluginApi {
            state,
            kv: &mut self.kv,
            theme,
        };
        plugin.dispose(&mut api);

        self.enabled.remove(id);
        state.remove_plugin_slot_content(id);
        state.remove_plugin_routes_by_plugin(id);
        self.commands
            .retain(|c| !c.id.starts_with(&format!("{id}:")));
        let _ = self.save_state();
    }

    pub fn list(&self) -> Vec<PluginInfo> {
        self.plugins
            .values()
            .map(|p| {
                let id = p.id();
                let command_count = self
                    .commands
                    .iter()
                    .filter(|c| c.id.starts_with(&format!("{id}:")))
                    .count();
                PluginInfo {
                    id: id.to_string(),
                    name: p.name().to_string(),
                    description: p.description().to_string(),
                    enabled: self.enabled.contains(id),
                    command_count,
                }
            })
            .collect()
    }

    pub fn commands(&self) -> &[PluginCommand] {
        &self.commands
    }

    pub fn execute_command(
        &mut self,
        id: &str,
        state: &mut AppState,
        theme: &Theme,
    ) -> Option<String> {
        let cmd = self.commands.iter().find(|c| c.id == id)?;
        let mut api = PluginApi {
            state,
            kv: &mut self.kv,
            theme,
        };
        (cmd.on_execute)(&mut api)
    }

    pub fn dispose_all(&mut self, state: &mut AppState, theme: &Theme) {
        let ids: Vec<String> = self.enabled.iter().cloned().collect();
        for id in ids {
            self.disable(&id, state, theme);
        }
    }

    pub fn kv_mut(&mut self) -> &mut KvStore {
        &mut self.kv
    }

    pub fn save_state(&self) -> Result<(), String> {
        let path = icode_config_dir().join("plugins.json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create dir {}: {e}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(&PluginStateFile {
            enabled: self.enabled.clone(),
        })
        .map_err(|e| format!("failed to serialize plugin state: {e}"))?;
        fs::write(&path, content)
            .map_err(|e| format!("cannot write plugin state to {}: {e}", path.display()))?;
        Ok(())
    }

    pub fn load_state(&mut self, state: &mut AppState, theme: &Theme) -> Result<(), String> {
        let path = icode_config_dir().join("plugins.json");
        if !path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read plugin state from {}: {e}", path.display()))?;
        let plugin_state: PluginStateFile = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse plugin state: {e}"))?;

        let to_enable: Vec<String> = plugin_state
            .enabled
            .into_iter()
            .filter(|id| self.plugins.contains_key(id))
            .collect();

        for id in to_enable {
            let _ = self.enable(&id, state, theme);
        }

        let _ = self.kv.load();
        Ok(())
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }
}

impl Default for PluginRuntime {
    fn default() -> Self {
        Self::new()
    }
}
