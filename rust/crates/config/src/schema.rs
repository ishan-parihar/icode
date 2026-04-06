use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub fallback_models: Vec<String>,
    #[serde(default)]
    pub permissions: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HookConfig {
    #[serde(default)]
    pub disabled_hooks: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BackgroundTaskConfig {
    #[serde(default)]
    pub max_concurrent: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RalphLoopConfig {
    pub enabled: bool,
    #[serde(default = "default_max_iterations")]
    pub default_max_iterations: usize,
}

impl Default for RalphLoopConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_max_iterations: default_max_iterations(),
        }
    }
}

fn default_max_iterations() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SisyphusConfig {
    #[serde(default)]
    pub disabled: bool,
    #[serde(default = "default_true")]
    pub planner_enabled: bool,
    #[serde(default)]
    pub replace_plan: bool,
}

impl Default for SisyphusConfig {
    fn default() -> Self {
        Self {
            disabled: false,
            planner_enabled: true,
            replace_plan: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub hooks: Option<HookConfig>,
    #[serde(default)]
    pub mcp_servers: Option<HashMap<String, serde_json::Value>>,

    #[serde(default)]
    pub agents: Option<HashMap<String, AgentConfig>>,
    #[serde(default)]
    pub background_tasks: Option<BackgroundTaskConfig>,
    #[serde(default)]
    pub ralph_loop: Option<RalphLoopConfig>,
    #[serde(default)]
    pub sisyphus: Option<SisyphusConfig>,
}
