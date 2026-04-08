use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub description: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub prompt: Option<String>,
    #[serde(default = "default_full_access")]
    pub access: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    pub max_turns: Option<u32>,
    pub color: Option<String>,
}

fn default_full_access() -> String {
    "full".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for AgentDefinition {
    fn default() -> Self {
        Self {
            description: None,
            model: None,
            temperature: None,
            prompt: None,
            access: "full".to_string(),
            visible: true,
            max_turns: None,
            color: None,
        }
    }
}

#[must_use]
pub fn default_agents() -> HashMap<String, AgentDefinition> {
    let mut m = HashMap::new();
    m.insert(
        "build".to_string(),
        AgentDefinition {
            description: Some("Full-access agent for implementing features".to_string()),
            prompt: Some(
                "You are the build agent. You have full access to read, write, and execute."
                    .to_string(),
            ),
            access: "full".to_string(),
            color: Some("cyan".to_string()),
            ..Default::default()
        },
    );
    m.insert(
        "plan".to_string(),
        AgentDefinition {
            description: Some("Read-only agent for analyzing code and planning".to_string()),
            prompt: Some(
                "You are the plan agent. You can read files but cannot write or execute."
                    .to_string(),
            ),
            access: "read-only".to_string(),
            max_turns: Some(20),
            color: Some("yellow".to_string()),
            ..Default::default()
        },
    );
    m.insert(
        "explore".to_string(),
        AgentDefinition {
            description: Some("Fast search-only agent for code exploration".to_string()),
            prompt: Some("You are the explore agent. You can search and read files.".to_string()),
            access: "search-only".to_string(),
            max_turns: Some(15),
            color: Some("green".to_string()),
            ..Default::default()
        },
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_agent_has_full_access() {
        let agent = AgentDefinition::default();
        assert_eq!(agent.access, "full");
        assert!(agent.visible);
        assert!(agent.model.is_none());
    }

    #[test]
    fn default_agents_includes_build_plan_explore() {
        let agents = default_agents();
        assert!(agents.contains_key("build"));
        assert!(agents.contains_key("plan"));
        assert!(agents.contains_key("explore"));
    }

    #[test]
    fn plan_agent_has_read_only_access() {
        let agents = default_agents();
        let plan = agents.get("plan").unwrap();
        assert_eq!(plan.access, "read-only");
        assert_eq!(plan.max_turns, Some(20));
    }
}
