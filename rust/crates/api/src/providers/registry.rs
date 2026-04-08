use super::ModelCapabilities;
use std::collections::HashMap;

/// Runtime-registered provider with its capabilities.
#[derive(Clone)]
pub struct RegisteredProvider {
    pub kind: crate::providers::ProviderKind,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub capabilities: HashMap<String, ModelCapabilities>,
}

/// Registry that maps provider names to their runtime configuration.
pub struct ProviderRegistry {
    providers: HashMap<String, RegisteredProvider>,
}

impl ProviderRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }
    pub fn register(&mut self, name: String, provider: RegisteredProvider) {
        self.providers.insert(name, provider);
    }
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&RegisteredProvider> {
        self.providers.get(name)
    }
    pub fn list(&self) -> impl Iterator<Item = (&String, &RegisteredProvider)> {
        self.providers.iter()
    }
    #[must_use]
    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
