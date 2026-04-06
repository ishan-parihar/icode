use crate::types::{
    AgentConfig, AgentMode, AgentPermissions, FallbackModel, PermissionMode, ThinkingConfig,
};

/// Builder for constructing [`AgentConfig`] with sensible defaults.
#[derive(Debug, Default)]
pub struct AgentConfigBuilder {
    name: Option<String>,
    description: Option<String>,
    mode: AgentMode,
    model: Option<String>,
    max_tokens: u32,
    prompt: Option<String>,
    color: String,
    permissions: AgentPermissions,
    fallback_models: Vec<FallbackModel>,
    reasoning_effort: Option<String>,
    temperature: Option<f64>,
    disabled_tools: Vec<String>,
}

impl AgentConfigBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_tokens: 4096,
            color: String::from("#888888"),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn mode(mut self, mode: AgentMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    #[must_use]
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    #[must_use]
    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.color = color.into();
        self
    }

    #[must_use]
    pub fn permissions(mut self, permissions: AgentPermissions) -> Self {
        self.permissions = permissions;
        self
    }

    #[must_use]
    pub fn add_fallback(mut self, model: impl Into<String>) -> Self {
        self.fallback_models.push(FallbackModel {
            model: model.into(),
            variant: None,
            thinking: None,
        });
        self
    }

    #[must_use]
    pub fn add_fallback_with_thinking(
        mut self,
        model: impl Into<String>,
        budget_tokens: u32,
    ) -> Self {
        self.fallback_models.push(FallbackModel {
            model: model.into(),
            variant: None,
            thinking: Some(ThinkingConfig {
                r#type: String::from("enabled"),
                budget_tokens,
            }),
        });
        self
    }

    #[must_use]
    pub fn reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    #[must_use]
    pub fn temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    #[must_use]
    pub fn disabled_tools(mut self, tools: Vec<String>) -> Self {
        self.disabled_tools = tools;
        self
    }

    /// Build the [`AgentConfig`].
    ///
    /// # Panics
    ///
    /// Panics if `name`, `description`, `model`, or `prompt` are not set.
    #[must_use]
    pub fn build(self) -> AgentConfig {
        AgentConfig {
            name: self.name.expect("agent name is required"),
            description: self.description.expect("agent description is required"),
            mode: self.mode,
            model: self.model.expect("agent model is required"),
            max_tokens: self.max_tokens,
            prompt: self.prompt.expect("agent prompt is required"),
            color: self.color,
            permissions: self.permissions,
            fallback_models: self.fallback_models,
            reasoning_effort: self.reasoning_effort,
            temperature: self.temperature,
            disabled_tools: self.disabled_tools,
        }
    }
}

/// Create a default `AgentPermissions` that allows everything.
#[must_use]
pub fn allow_all_permissions() -> AgentPermissions {
    AgentPermissions {
        question: PermissionMode::Allow,
        call_omo_agent: PermissionMode::Allow,
        tool_overrides: std::collections::HashMap::new(),
    }
}

/// Create a default `AgentPermissions` that denies everything.
#[must_use]
pub fn deny_all_permissions() -> AgentPermissions {
    AgentPermissions {
        question: PermissionMode::Deny,
        call_omo_agent: PermissionMode::Deny,
        tool_overrides: std::collections::HashMap::new(),
    }
}
