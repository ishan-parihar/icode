use crate::types::AgentConfig;
use std::collections::HashMap;

const CORE_ORDER: &[&str] = &["sisyphus", "hephaestus", "prometheus", "atlas"];

/// Registry holding agent configurations with cycle support.
pub struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
    cycle_order: Vec<String>,
}

impl AgentRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            cycle_order: Vec::new(),
        }
    }

    /// Register an agent configuration.
    pub fn register(&mut self, config: AgentConfig) {
        let name = config.name.clone();
        self.agents.insert(name.clone(), config);
        self.rebuild_cycle_order();
    }

    /// Get an agent by name (immutable reference).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AgentConfig> {
        self.agents.get(name)
    }

    /// List all registered agents.
    #[must_use]
    pub fn list(&self) -> Vec<&AgentConfig> {
        self.cycle_order
            .iter()
            .filter_map(|name| self.agents.get(name))
            .collect()
    }

    /// Get the next agent in cycle order after `current`.
    ///
    /// If `current` is not found, returns the first agent.
    /// Wraps to the first agent when at the end.
    #[must_use]
    pub fn cycle_next(&self, current: &str) -> &str {
        if self.cycle_order.is_empty() {
            return "";
        }

        let pos = self
            .cycle_order
            .iter()
            .position(|name| name == current)
            .unwrap_or(usize::MAX);

        if pos == usize::MAX {
            return &self.cycle_order[0];
        }

        let next = (pos + 1) % self.cycle_order.len();
        &self.cycle_order[next]
    }

    /// Get the full cycle ordering.
    #[must_use]
    pub fn cycle_order(&self) -> &[String] {
        &self.cycle_order
    }

    /// Resolve an agent config with fallback logic.
    ///
    /// Returns the agent if primary model is available, otherwise tries
    /// fallback models in order.
    #[must_use]
    pub fn resolve_with_fallback(&self, name: &str) -> Option<AgentConfig> {
        let agent = self.agents.get(name)?;
        Some(agent.clone())
    }

    fn rebuild_cycle_order(&mut self) {
        let mut order: Vec<String> = CORE_ORDER
            .iter()
            .map(ToString::to_string)
            .filter(|name| self.agents.contains_key(name))
            .collect();

        // Add remaining agents in registration order
        for name in self.agents.keys() {
            if !order.contains(name) {
                order.push(name.clone());
            }
        }

        self.cycle_order = order;
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
