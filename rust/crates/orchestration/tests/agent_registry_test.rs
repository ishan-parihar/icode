use orchestration::{
    agent_config::{allow_all_permissions, deny_all_permissions, AgentConfigBuilder},
    AgentConfig, AgentMode, AgentPermissions, AgentRegistry, ModelRouter, PermissionMode,
};
use serde_json;

fn make_agent(name: &str, model: &str) -> AgentConfig {
    AgentConfigBuilder::new()
        .name(name)
        .description(format!("Test agent {name}"))
        .mode(AgentMode::Primary)
        .model(model)
        .prompt(format!("You are {name}"))
        .color("#000000")
        .permissions(allow_all_permissions())
        .build()
}

// === Register + Retrieve ===

#[test]
fn register_and_get_agent() {
    let mut registry = AgentRegistry::new();
    let config = make_agent("test", "gpt-4");
    registry.register(config);

    let retrieved = registry.get("test").expect("agent should exist");
    assert_eq!(retrieved.name, "test");
    assert_eq!(retrieved.model, "gpt-4");
}

#[test]
fn get_nonexistent_returns_none() {
    let registry = AgentRegistry::new();
    assert!(registry.get("missing").is_none());
}

// === List ===

#[test]
fn list_returns_all_agents() {
    let mut registry = AgentRegistry::new();
    registry.register(make_agent("alpha", "model-a"));
    registry.register(make_agent("beta", "model-b"));
    registry.register(make_agent("sisyphus", "model-s"));

    let agents = registry.list();
    assert_eq!(agents.len(), 3);
}

// === Cycle Ordering ===

#[test]
fn cycle_order_core_first() {
    let mut registry = AgentRegistry::new();
    registry.register(make_agent("zeus", "model-z"));
    registry.register(make_agent("sisyphus", "model-s"));
    registry.register(make_agent("atlas", "model-a"));
    registry.register(make_agent("hephaestus", "model-h"));
    registry.register(make_agent("prometheus", "model-p"));

    let order = registry.cycle_order();
    assert_eq!(order.len(), 5);

    // Core agents first
    assert_eq!(order[0], "sisyphus");
    assert_eq!(order[1], "hephaestus");
    assert_eq!(order[2], "prometheus");
    assert_eq!(order[3], "atlas");
    // Then rest in registration order
    assert_eq!(order[4], "zeus");
}

#[test]
fn cycle_wraps_at_end() {
    let mut registry = AgentRegistry::new();
    registry.register(make_agent("sisyphus", "model-s"));
    registry.register(make_agent("hephaestus", "model-h"));

    // Last element wraps to first
    let next = registry.cycle_next("hephaestus");
    assert_eq!(next, "sisyphus");
}

#[test]
fn cycle_unknown_returns_first() {
    let mut registry = AgentRegistry::new();
    registry.register(make_agent("sisyphus", "model-s"));
    registry.register(make_agent("hephaestus", "model-h"));

    let next = registry.cycle_next("unknown");
    assert_eq!(next, "sisyphus");
}

#[test]
fn cycle_empty_registry_returns_empty_string() {
    let registry = AgentRegistry::new();
    assert_eq!(registry.cycle_next("anything"), "");
}

// === Fallback Resolution ===

#[test]
fn resolve_with_fallback_returns_clone() {
    let mut registry = AgentRegistry::new();
    registry.register(make_agent("test", "model-x"));

    let resolved = registry.resolve_with_fallback("test");
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().name, "test");
}

#[test]
fn resolve_with_fallback_nonexistent_returns_none() {
    let registry = AgentRegistry::new();
    assert!(registry.resolve_with_fallback("missing").is_none());
}

// === Model Router ===

#[test]
fn model_router_primary_available() {
    let router = ModelRouter::new(vec!["gpt-4".to_string(), "gpt-3.5".to_string()]);
    let agent = make_agent("test", "gpt-4");

    let resolved = router.resolve(&agent);
    assert_eq!(resolved, "gpt-4");
}

#[test]
fn model_router_fallback_used() {
    let router = ModelRouter::new(vec!["gpt-3.5".to_string()]);
    let agent = AgentConfigBuilder::new()
        .name("test")
        .description("Test")
        .mode(AgentMode::Primary)
        .model("gpt-4")
        .prompt("You are test")
        .color("#000")
        .permissions(allow_all_permissions())
        .add_fallback("gpt-3.5")
        .build();

    let resolved = router.resolve(&agent);
    assert_eq!(resolved, "gpt-3.5");
}

#[test]
fn model_router_no_fallback_returns_primary() {
    let router = ModelRouter::new(vec![]);
    let agent = make_agent("test", "gpt-4");

    let resolved = router.resolve(&agent);
    assert_eq!(resolved, "gpt-4");
}

#[test]
fn model_router_is_model_available() {
    let router = ModelRouter::new(vec!["gpt-4".to_string()]);
    assert!(router.is_model_available("gpt-4"));
    assert!(!router.is_model_available("gpt-3.5"));
}

#[test]
fn model_router_first_fallback_when_none_available() {
    let router = ModelRouter::new(vec![]);
    let agent = AgentConfigBuilder::new()
        .name("test")
        .description("Test")
        .mode(AgentMode::Primary)
        .model("gpt-4")
        .prompt("You are test")
        .color("#000")
        .permissions(allow_all_permissions())
        .add_fallback("gpt-3.5")
        .add_fallback("claude-sonnet")
        .build();

    let resolved = router.resolve(&agent);
    assert_eq!(resolved, "gpt-3.5");
}

// === Permission Config ===

#[test]
fn permission_allow_serde_roundtrip() {
    let json = serde_json::to_string(&PermissionMode::Allow).unwrap();
    let parsed: PermissionMode = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, PermissionMode::Allow);
}

#[test]
fn permission_deny_serde_roundtrip() {
    let json = serde_json::to_string(&PermissionMode::Deny).unwrap();
    let parsed: PermissionMode = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, PermissionMode::Deny);
}

#[test]
fn permissions_with_tool_overrides_roundtrip() {
    let mut perms = AgentPermissions::default();
    perms
        .tool_overrides
        .insert("bash".to_string(), PermissionMode::Deny);

    let json = serde_json::to_string(&perms).unwrap();
    let parsed: AgentPermissions = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.question, PermissionMode::Allow);
    assert_eq!(
        parsed.tool_overrides.get("bash"),
        Some(&PermissionMode::Deny)
    );
}

// === AgentConfig Serde ===

#[test]
fn agent_config_serde_roundtrip() {
    let agent = AgentConfigBuilder::new()
        .name("sisyphus")
        .description("Primary agent")
        .mode(AgentMode::Subagent)
        .model("claude-sonnet-4-6")
        .max_tokens(8192)
        .prompt("You are Sisyphus")
        .color("#ff6600")
        .permissions(allow_all_permissions())
        .add_fallback("gpt-4o")
        .reasoning_effort("high")
        .temperature(0.7)
        .disabled_tools(vec!["sleep".to_string()])
        .build();

    let json = serde_json::to_string(&agent).unwrap();
    let parsed: AgentConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.name, "sisyphus");
    assert_eq!(parsed.mode, AgentMode::Subagent);
    assert_eq!(parsed.model, "claude-sonnet-4-6");
    assert_eq!(parsed.max_tokens, 8192);
    assert_eq!(parsed.color, "#ff6600");
    assert_eq!(parsed.reasoning_effort, Some("high".to_string()));
    assert_eq!(parsed.temperature, Some(0.7));
    assert_eq!(parsed.disabled_tools, vec!["sleep"]);
    assert_eq!(parsed.fallback_models.len(), 1);
    assert_eq!(parsed.fallback_models[0].model, "gpt-4o");
}

#[test]
fn agent_config_with_thinking_fallback_roundtrip() {
    let agent = AgentConfigBuilder::new()
        .name("test")
        .description("Test")
        .mode(AgentMode::Primary)
        .model("claude-opus")
        .prompt("test")
        .color("#000")
        .permissions(allow_all_permissions())
        .add_fallback_with_thinking("claude-sonnet", 4096)
        .build();

    let json = serde_json::to_string(&agent).unwrap();
    let parsed: AgentConfig = serde_json::from_str(&json).unwrap();

    let thinking = parsed.fallback_models[0].thinking.as_ref().unwrap();
    assert_eq!(thinking.r#type, "enabled");
    assert_eq!(thinking.budget_tokens, 4096);
}

// === Builder Helpers ===

#[test]
fn allow_all_permissions_correct() {
    let perms = allow_all_permissions();
    assert_eq!(perms.question, PermissionMode::Allow);
    assert_eq!(perms.call_omo_agent, PermissionMode::Allow);
    assert!(perms.tool_overrides.is_empty());
}

#[test]
fn deny_all_permissions_correct() {
    let perms = deny_all_permissions();
    assert_eq!(perms.question, PermissionMode::Deny);
    assert_eq!(perms.call_omo_agent, PermissionMode::Deny);
    assert!(perms.tool_overrides.is_empty());
}

#[test]
#[should_panic(expected = "agent name is required")]
fn build_without_name_panics() {
    let _ = AgentConfigBuilder::new()
        .description("Test")
        .model("gpt-4")
        .prompt("test")
        .build();
}
