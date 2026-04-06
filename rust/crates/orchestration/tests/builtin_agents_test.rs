use orchestration::{builtin_agents, AgentConfig, AgentMode, AgentPermissions, PermissionMode};

fn agent_by_name<'a>(agents: &'a [AgentConfig], name: &str) -> &'a AgentConfig {
    agents
        .iter()
        .find(|a| a.name == name)
        .expect("agent not found")
}

#[test]
fn builtin_agents_returns_11_agents() {
    let agents = builtin_agents();
    assert_eq!(agents.len(), 11);
}

#[test]
fn each_agent_has_required_fields() {
    let agents = builtin_agents();
    for agent in &agents {
        assert!(!agent.name.is_empty(), "agent name is empty: {:?}", agent);
        assert!(!agent.model.is_empty(), "agent model is empty: {:?}", agent);
        assert!(
            !agent.prompt.is_empty(),
            "agent prompt is empty: {:?}",
            agent
        );
    }
}

#[test]
fn readonly_agents_have_correct_permissions() {
    let agents = builtin_agents();
    let readonly_names = ["oracle", "librarian", "explore", "multimodal_looker"];

    for name in &readonly_names {
        let agent = agent_by_name(&agents, name);
        assert_tool_denied(&agent.permissions, "write_file", name);
        assert_tool_denied(&agent.permissions, "edit_file", name);
    }
}

fn assert_tool_denied(perms: &AgentPermissions, tool: &str, agent_name: &str) {
    let is_allowed = perms
        .tool_overrides
        .get(tool)
        .is_some_and(|m| *m == PermissionMode::Allow);
    assert!(
        !is_allowed,
        "{agent_name} should have {tool} denied, but it was allowed"
    );
}

#[test]
fn sisyphus_is_primary_agent() {
    let agents = builtin_agents();
    let sisyphus = agent_by_name(&agents, "sisyphus");
    assert_eq!(sisyphus.mode, AgentMode::Primary);
}

#[test]
fn agents_have_non_empty_prompts() {
    let agents = builtin_agents();
    for agent in &agents {
        assert!(
            agent.prompt.len() > 50,
            "agent '{}' prompt too short ({})",
            agent.name,
            agent.prompt.len()
        );
    }
}

#[test]
fn agent_names_are_unique() {
    let agents = builtin_agents();
    let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
    let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
    assert_eq!(names.len(), unique.len(), "duplicate agent names detected");
}

#[test]
fn sisyphus_has_fallback_models() {
    let agents = builtin_agents();
    let sisyphus = agent_by_name(&agents, "sisyphus");
    assert!(
        !sisyphus.fallback_models.is_empty(),
        "sisyphus should have fallback models"
    );
    assert_eq!(sisyphus.fallback_models.len(), 3);
}
