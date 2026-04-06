use crate::agent_config::{allow_all_permissions, deny_all_permissions, AgentConfigBuilder};
use crate::types::{AgentConfig, AgentMode, AgentPermissions, PermissionMode};

/// Create a permission config that allows only `read_file`.
fn read_only_permission() -> AgentPermissions {
    let mut perms = deny_all_permissions();
    perms
        .tool_overrides
        .insert("read_file".to_string(), PermissionMode::Allow);
    perms
}

/// Define all builtin agents.
#[must_use]
pub fn builtin_agents() -> Vec<AgentConfig> {
    vec![
        sisyphus(),
        hephaestus(),
        oracle(),
        librarian(),
        explore(),
        atlas(),
        prometheus(),
        metis(),
        momus(),
        multimodal_looker(),
        sisyphus_junior(),
    ]
}

fn sisyphus() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("sisyphus")
        .description("Primary orchestrator agent with delegation capabilities")
        .mode(AgentMode::Primary)
        .model("claude-opus-4-6")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/sisyphus.md"))
        .color("#00CED1")
        .permissions(allow_all_permissions())
        .add_fallback("kimi-k2.5")
        .add_fallback("glm-5")
        .add_fallback_with_thinking("gpt-5.4", 16_000)
        .reasoning_effort("medium")
        .build()
}

fn hephaestus() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("hephaestus")
        .description("Autonomous deep worker for goal-oriented implementation")
        .mode(AgentMode::Primary)
        .model("gpt-5.4")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/hephaestus.md"))
        .color("#FF6600")
        .permissions(allow_all_permissions())
        .reasoning_effort("medium")
        .build()
}

fn oracle() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("oracle")
        .description("Architecture and design expert for deep analysis")
        .mode(AgentMode::Subagent)
        .model("gpt-5.4")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/oracle.md"))
        .color("#9B59B6")
        .permissions(deny_all_permissions())
        .disabled_tools(vec![
            "write_file".to_string(),
            "edit_file".to_string(),
            "task".to_string(),
            "call_omo_agent".to_string(),
        ])
        .reasoning_effort("high")
        .build()
}

fn librarian() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("librarian")
        .description("External documentation and OSS library expert")
        .mode(AgentMode::Subagent)
        .model("minimax-m2.7")
        .max_tokens(32_000)
        .prompt(include_str!("prompts/librarian.md"))
        .color("#3498DB")
        .permissions(deny_all_permissions())
        .disabled_tools(vec![
            "write_file".to_string(),
            "edit_file".to_string(),
            "task".to_string(),
            "call_omo_agent".to_string(),
        ])
        .build()
}

fn explore() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("explore")
        .description("Internal codebase mapping and analysis specialist")
        .mode(AgentMode::Subagent)
        .model("grok-code-fast-1")
        .max_tokens(32_000)
        .prompt(include_str!("prompts/explore.md"))
        .color("#2ECC71")
        .permissions(deny_all_permissions())
        .disabled_tools(vec![
            "write_file".to_string(),
            "edit_file".to_string(),
            "task".to_string(),
            "call_omo_agent".to_string(),
        ])
        .build()
}

fn atlas() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("atlas")
        .description("Todo-driven orchestrator that reads plans and delegates tasks")
        .mode(AgentMode::Primary)
        .model("claude-sonnet-4-6")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/atlas.md"))
        .color("#E67E22")
        .permissions(allow_all_permissions())
        .disabled_tools(vec!["task".to_string(), "call_omo_agent".to_string()])
        .build()
}

fn prometheus() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("prometheus")
        .description("Strategic planner creating comprehensive implementation plans")
        .mode(AgentMode::Primary)
        .model("claude-opus-4-6")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/prometheus.md"))
        .color("#E74C3C")
        .permissions(allow_all_permissions())
        .reasoning_effort("high")
        .build()
}

fn metis() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("metis")
        .description("Plan consultant that finds gaps and edge cases")
        .mode(AgentMode::Subagent)
        .model("claude-opus-4-6")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/metis.md"))
        .color("#8E44AD")
        .permissions(deny_all_permissions())
        .disabled_tools(vec![
            "write_file".to_string(),
            "edit_file".to_string(),
            "task".to_string(),
        ])
        .build()
}

fn momus() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("momus")
        .description("Plan critic validating clarity, verifiability, and completeness")
        .mode(AgentMode::Subagent)
        .model("gpt-5.4")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/momus.md"))
        .color("#F39C12")
        .permissions(deny_all_permissions())
        .disabled_tools(vec![
            "write_file".to_string(),
            "edit_file".to_string(),
            "task".to_string(),
        ])
        .reasoning_effort("high")
        .build()
}

fn multimodal_looker() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("multimodal_looker")
        .description("Visual content analysis specialist for images, PDFs, and diagrams")
        .mode(AgentMode::Subagent)
        .model("gpt-5.4")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/multimodal_looker.md"))
        .color("#1ABC9C")
        .permissions(read_only_permission())
        .disabled_tools(vec![
            "write_file".to_string(),
            "edit_file".to_string(),
            "task".to_string(),
            "call_omo_agent".to_string(),
            "bash".to_string(),
        ])
        .build()
}

fn sisyphus_junior() -> AgentConfig {
    AgentConfigBuilder::new()
        .name("sisyphus_junior")
        .description("Focused executor that implements tasks directly without delegation")
        .mode(AgentMode::Subagent)
        .model("claude-sonnet-4-6")
        .max_tokens(64_000)
        .prompt(include_str!("prompts/junior.md"))
        .color("#00CED1")
        .permissions(allow_all_permissions())
        .build()
}
