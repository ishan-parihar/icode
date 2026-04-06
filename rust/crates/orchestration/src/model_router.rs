use crate::types::AgentConfig;
use std::collections::HashSet;

/// Router that resolves which model to use for an agent based on availability.
pub struct ModelRouter {
    available_models: HashSet<String>,
}

impl ModelRouter {
    /// Create a new router with the given set of available models.
    #[must_use]
    pub fn new(available_models: Vec<String>) -> Self {
        Self {
            available_models: available_models.into_iter().collect(),
        }
    }

    /// Resolve the model to use for an agent.
    ///
    /// Priority: primary model → `fallback_models` chain → first available model
    /// → if none available, return first fallback model name.
    #[must_use]
    pub fn resolve(&self, agent: &AgentConfig) -> String {
        // Try primary model
        if self.is_model_available(&agent.model) {
            return agent.model.clone();
        }

        // Try fallback chain
        for fallback in &agent.fallback_models {
            if self.is_model_available(&fallback.model) {
                return fallback.model.clone();
            }
        }

        // No models available — return first fallback if any, otherwise primary
        agent
            .fallback_models
            .first()
            .map_or_else(|| agent.model.clone(), |f| f.model.clone())
    }

    /// Check if a model is available.
    #[must_use]
    pub fn is_model_available(&self, model: &str) -> bool {
        self.available_models.contains(model)
    }
}
