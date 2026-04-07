use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub models: Vec<String>,
}

/// A provider registry that maps model names to provider configurations.
#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    providers: HashMap<String, ProviderConfig>,
    aliases: HashMap<String, String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            aliases: HashMap::new(),
        };

        // Register default providers
        registry.register_provider(ProviderConfig {
            name: "anthropic".to_string(),
            api_key: None,
            base_url: Some("https://api.anthropic.com".to_string()),
            models: vec![
                "claude-opus-4-20250514".to_string(),
                "claude-sonnet-4-20250514".to_string(),
                "claude-haiku-3-5-20241022".to_string(),
            ],
        });

        registry.register_provider(ProviderConfig {
            name: "openai".to_string(),
            api_key: None,
            base_url: Some("https://api.openai.com".to_string()),
            models: vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "o1".to_string(),
                "o3-mini".to_string(),
            ],
        });

        registry.register_provider(ProviderConfig {
            name: "google".to_string(),
            api_key: None,
            base_url: Some("https://generativelanguage.googleapis.com".to_string()),
            models: vec!["gemini-2.5-pro".to_string(), "gemini-2.5-flash".to_string()],
        });

        // Set up model aliases
        registry.add_alias("opus", "claude-opus-4-20250514");
        registry.add_alias("sonnet", "claude-sonnet-4-20250514");
        registry.add_alias("haiku", "claude-haiku-3-5-20241022");
        registry.add_alias("gpt4o", "gpt-4o");
        registry.add_alias("gpt4o-mini", "gpt-4o-mini");
        registry.add_alias("gemini", "gemini-2.5-pro");
        registry.add_alias("flash", "gemini-2.5-flash");

        registry
    }

    pub fn register_provider(&mut self, config: ProviderConfig) {
        self.providers.insert(config.name.clone(), config);
    }

    pub fn add_alias(&mut self, alias: &str, model: &str) {
        self.aliases.insert(alias.to_string(), model.to_string());
    }

    /// Resolve a model name (alias or full name) to a provider config.
    ///
    /// Returns the provider config and the resolved canonical model name.
    /// For unknown models, defaults to the anthropic provider and returns
    /// the original model name unchanged.
    pub fn resolve(&self, model_name: &str) -> Option<(&ProviderConfig, String)> {
        let resolved = self
            .aliases
            .get(model_name)
            .cloned()
            .unwrap_or_else(|| model_name.to_string());

        for (_name, config) in &self.providers {
            if config.models.iter().any(|m| *m == resolved) {
                return Some((config, resolved));
            }
        }

        self.providers
            .get("anthropic")
            .map(|config| (config, model_name.to_string()))
    }

    /// Get all registered model names.
    pub fn list_models(&self) -> Vec<String> {
        self.providers
            .values()
            .flat_map(|p| p.models.iter().cloned())
            .collect()
    }

    /// Get all providers.
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_anthropic_models() {
        let registry = ProviderRegistry::new();
        let (config, model) = registry
            .resolve("claude-sonnet-4-20250514")
            .expect("should resolve");
        assert_eq!(config.name, "anthropic");
        assert_eq!(model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn resolves_aliases() {
        let registry = ProviderRegistry::new();
        let (config, model) = registry.resolve("opus").expect("should resolve");
        assert_eq!(config.name, "anthropic");
        assert_eq!(model, "claude-opus-4-20250514");
    }

    #[test]
    fn defaults_to_anthropic_for_unknown_model() {
        let registry = ProviderRegistry::new();
        let (config, model) = registry.resolve("unknown-model").expect("should default");
        assert_eq!(config.name, "anthropic");
        assert_eq!(model, "unknown-model");
    }

    #[test]
    fn registers_custom_providers() {
        let mut registry = ProviderRegistry::new();
        registry.register_provider(ProviderConfig {
            name: "groq".to_string(),
            api_key: None,
            base_url: Some("https://api.groq.com".to_string()),
            models: vec!["llama-3-70b".to_string()],
        });

        let (config, model) = registry.resolve("llama-3-70b").expect("should resolve");
        assert_eq!(config.name, "groq");
        assert_eq!(model, "llama-3-70b");
    }

    #[test]
    fn lists_all_models() {
        let registry = ProviderRegistry::new();
        let models = registry.list_models();
        assert!(models.contains(&"claude-sonnet-4-20250514".to_string()));
        assert!(models.contains(&"gpt-4o".to_string()));
        assert!(models.contains(&"gemini-2.5-pro".to_string()));
    }

    #[test]
    fn lists_all_providers() {
        let registry = ProviderRegistry::new();
        let providers = registry.list_providers();
        assert!(providers.contains(&"anthropic"));
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"google"));
    }
}
