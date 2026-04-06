use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent mode: primary (respects UI model), subagent (own fallback chain), all.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum AgentMode {
    #[default]
    Primary,
    Subagent,
    All,
}

/// Permission mode for individual tools.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum PermissionMode {
    #[default]
    Allow,
    Deny,
}

/// Agent permission configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentPermissions {
    pub question: PermissionMode,
    pub call_omo_agent: PermissionMode,
    #[serde(flatten)]
    pub tool_overrides: HashMap<String, PermissionMode>,
}

/// Fallback model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackModel {
    pub model: String,
    pub variant: Option<String>,
    pub thinking: Option<ThinkingConfig>,
}

/// Thinking budget configuration for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    pub r#type: String,
    pub budget_tokens: u32,
}

/// Complete agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub mode: AgentMode,
    pub model: String,
    pub max_tokens: u32,
    pub prompt: String,
    pub color: String,
    pub permissions: AgentPermissions,
    pub fallback_models: Vec<FallbackModel>,
    pub reasoning_effort: Option<String>,
    pub temperature: Option<f64>,
    pub disabled_tools: Vec<String>,
}
