//! Coordinator Mode — AI orchestrates work via workers rather than coding directly.
//!
//! When `ICODE_COORDINATOR_MODE` is set, the AI operates as an orchestrator:
//! it breaks work into sub-tasks and delegates to workers instead of writing
//! code itself. Tool surfaces are filtered so the coordinator only sees
//! delegation/management tools, while workers only see execution tools.

use serde::{Deserialize, Serialize};

/// Operating mode for the AI agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    /// Standard mode — the AI writes code and uses the full tool surface.
    #[default]
    Default,
    /// Coordinator mode — the AI orchestrates work via workers.
    Coordinator,
}

/// Configuration for coordinator-mode behaviour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Whether coordinator mode is active.
    pub enabled: bool,
    /// Prefix prepended to the system prompt in coordinator mode.
    pub system_prompt_prefix: String,
    /// Tools that must be hidden from the coordinator.
    pub restricted_tools: Vec<String>,
    /// Tools the coordinator is allowed to see.
    pub allowed_worker_tools: Vec<String>,
}

/// Runtime state for a coordinator session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoordinatorState {
    /// Current agent mode.
    pub mode: AgentMode,
    /// Number of tasks dispatched to workers.
    pub tasks_dispatched: usize,
    /// Number of tasks completed by workers.
    pub tasks_completed: usize,
}

/// Tools available to the coordinator (delegation/management surface).
const COORDINATOR_ALLOWED_TOOLS: &[&str] = &[
    "TaskCreate",
    "TaskGet",
    "TaskList",
    "TaskStop",
    "TaskUpdate",
    "TaskOutput",
    "RunTaskPacket",
    "TeamCreate",
    "TeamDelete",
    "WorkerCreate",
    "WorkerGet",
    "WorkerObserve",
    "WorkerResolveTrust",
    "WorkerAwaitReady",
    "WorkerSendPrompt",
    "WorkerRestart",
    "WorkerTerminate",
    "TodoWrite",
    "ToolSearch",
    "SendUserMessage",
];

/// Tools available to workers (execution surface).
const WORKER_ALLOWED_TOOLS: &[&str] = &[
    "bash",
    "read_file",
    "write_file",
    "edit_file",
    "glob_search",
    "grep_search",
    "TodoWrite",
    "Sleep",
    "LSP",
    "WebFetch",
    "WebSearch",
    "Skill",
];

/// Orchestrator system-prompt prefix.
const COORDINATOR_PROMPT_PREFIX: &str =
    "You are an ORCHESTRATOR. Do NOT write code directly. Break work into sub-tasks and delegate using TaskCreate/WorkerCreate/TeamCreate tools. Review results and synthesize findings.";

/// Check whether coordinator mode is enabled via environment variable.
///
/// Reads `ICODE_COORDINATOR_MODE`; returns `true` when the value is `"1"`,
/// `"true"`, or `"yes"` (case-insensitive).
#[must_use]
pub fn is_coordinator_mode_enabled() -> bool {
    match std::env::var("ICODE_COORDINATOR_MODE") {
        Ok(val) => {
            let lowered = val.to_ascii_lowercase();
            lowered == "1" || lowered == "true" || lowered == "yes"
        }
        Err(_) => false,
    }
}

/// Build the full system prompt for coordinator mode.
///
/// Prepends the orchestrator directive to `base_prompt`. If the base prompt
/// already contains the prefix it is returned unchanged.
#[must_use]
pub fn build_coordinator_system_prompt(base_prompt: &str) -> String {
    if base_prompt.contains(COORDINATOR_PROMPT_PREFIX) {
        return base_prompt.to_string();
    }
    format!("{COORDINATOR_PROMPT_PREFIX}\n\n{base_prompt}")
}

/// Filter the full tool list down to the coordinator-allowed subset.
///
/// Only tools present in both `all_tools` and [`COORDINATOR_ALLOWED_TOOLS`]
/// are returned, preserving the order of `all_tools`.
#[must_use]
pub fn filter_tools_for_coordinator(all_tools: &[String]) -> Vec<String> {
    all_tools
        .iter()
        .filter(|tool| COORDINATOR_ALLOWED_TOOLS.contains(&tool.as_str()))
        .cloned()
        .collect()
}

/// Filter the full tool list down to the worker-allowed subset.
///
/// Only tools present in both `all_tools` and [`WORKER_ALLOWED_TOOLS`]
/// are returned, preserving the order of `all_tools`.
#[must_use]
pub fn filter_tools_for_worker(all_tools: &[String]) -> Vec<String> {
    all_tools
        .iter()
        .filter(|tool| WORKER_ALLOWED_TOOLS.contains(&tool.as_str()))
        .cloned()
        .collect()
}

/// Construct a [`CoordinatorConfig`] with sensible defaults.
///
/// The defaults reflect a typical coordinator setup:
/// - `enabled`: `false` (must be opted in)
/// - `system_prompt_prefix`: the standard orchestrator directive
/// - `restricted_tools`: all code-editing and direct-execution tools
/// - `allowed_worker_tools`: the coordinator delegation surface
#[must_use]
pub fn default_coordinator_config() -> CoordinatorConfig {
    CoordinatorConfig {
        enabled: false,
        system_prompt_prefix: COORDINATOR_PROMPT_PREFIX.to_string(),
        restricted_tools: vec![
            "bash".to_string(),
            "read_file".to_string(),
            "write_file".to_string(),
            "edit_file".to_string(),
            "glob_search".to_string(),
            "grep_search".to_string(),
            "LSP".to_string(),
            "WebFetch".to_string(),
            "WebSearch".to_string(),
            "Skill".to_string(),
            "Sleep".to_string(),
        ],
        allowed_worker_tools: COORDINATOR_ALLOWED_TOOLS
            .iter()
            .map(|&s| s.to_owned())
            .collect(),
    }
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        default_coordinator_config()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Environment variable detection ────────────────────────────────

    #[test]
    fn env_var_absent_returns_false() {
        // Ensure the var is not set (tests run in isolation, but be safe).
        std::env::remove_var("ICODE_COORDINATOR_MODE");
        assert!(!is_coordinator_mode_enabled());
    }

    #[test]
    fn env_var_set_to_1_returns_true() {
        let _lock = crate::test_env_lock();
        std::env::set_var("ICODE_COORDINATOR_MODE", "1");
        assert!(is_coordinator_mode_enabled());
        std::env::remove_var("ICODE_COORDINATOR_MODE");
    }

    #[test]
    fn env_var_set_to_true_returns_true() {
        let _lock = crate::test_env_lock();
        std::env::set_var("ICODE_COORDINATOR_MODE", "true");
        assert!(is_coordinator_mode_enabled());
        std::env::remove_var("ICODE_COORDINATOR_MODE");
    }

    #[test]
    fn env_var_set_to_yes_returns_true() {
        let _lock = crate::test_env_lock();
        std::env::set_var("ICODE_COORDINATOR_MODE", "yes");
        assert!(is_coordinator_mode_enabled());
        std::env::remove_var("ICODE_COORDINATOR_MODE");
    }

    #[test]
    fn env_var_case_insensitive() {
        let _lock = crate::test_env_lock();
        std::env::set_var("ICODE_COORDINATOR_MODE", "TRUE");
        assert!(is_coordinator_mode_enabled());
        std::env::set_var("ICODE_COORDINATOR_MODE", "Yes");
        assert!(is_coordinator_mode_enabled());
        std::env::set_var("ICODE_COORDINATOR_MODE", "TRUE");
        std::env::remove_var("ICODE_COORDINATOR_MODE");
    }

    #[test]
    fn env_var_set_to_invalid_value_returns_false() {
        let _lock = crate::test_env_lock();
        std::env::set_var("ICODE_COORDINATOR_MODE", "no");
        assert!(!is_coordinator_mode_enabled());
        std::env::set_var("ICODE_COORDINATOR_MODE", "0");
        assert!(!is_coordinator_mode_enabled());
        std::env::set_var("ICODE_COORDINATOR_MODE", "false");
        assert!(!is_coordinator_mode_enabled());
        std::env::remove_var("ICODE_COORDINATOR_MODE");
    }

    // ── System prompt generation ──────────────────────────────────────

    #[test]
    fn build_prompt_prepends_orchestrator_directive() {
        let base = "You are a helpful assistant.";
        let result = build_coordinator_system_prompt(base);
        assert!(result.starts_with(COORDINATOR_PROMPT_PREFIX));
        assert!(result.contains(base));
    }

    #[test]
    fn build_prompt_does_not_duplicate_prefix() {
        let base = format!("{COORDINATOR_PROMPT_PREFIX}\n\nAlready has prefix.");
        let result = build_coordinator_system_prompt(&base);
        assert_eq!(result, base);
    }

    #[test]
    fn build_prompt_with_empty_base() {
        let result = build_coordinator_system_prompt("");
        assert_eq!(result, format!("{COORDINATOR_PROMPT_PREFIX}\n\n"));
    }

    // ── Tool filtering — coordinator ──────────────────────────────────

    #[test]
    fn filter_coordinator_keeps_only_allowed_tools() {
        let all_tools = vec![
            "TaskCreate".to_string(),
            "bash".to_string(),
            "WorkerCreate".to_string(),
            "edit_file".to_string(),
            "TodoWrite".to_string(),
            "TeamCreate".to_string(),
        ];
        let filtered = filter_tools_for_coordinator(&all_tools);
        assert_eq!(
            filtered,
            vec![
                "TaskCreate".to_string(),
                "WorkerCreate".to_string(),
                "TodoWrite".to_string(),
                "TeamCreate".to_string(),
            ]
        );
    }

    #[test]
    fn filter_coordinator_empty_input() {
        let filtered = filter_tools_for_coordinator(&[]);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_coordinator_preserves_all_allowed() {
        let all_tools: Vec<String> = COORDINATOR_ALLOWED_TOOLS
            .iter()
            .map(|&s| s.to_owned())
            .collect();
        let filtered = filter_tools_for_coordinator(&all_tools);
        assert_eq!(filtered.len(), COORDINATOR_ALLOWED_TOOLS.len());
    }

    // ── Tool filtering — worker ───────────────────────────────────────

    #[test]
    fn filter_worker_keeps_only_allowed_tools() {
        let all_tools = vec![
            "bash".to_string(),
            "TaskCreate".to_string(),
            "read_file".to_string(),
            "WorkerCreate".to_string(),
            "edit_file".to_string(),
            "Skill".to_string(),
        ];
        let filtered = filter_tools_for_worker(&all_tools);
        assert_eq!(
            filtered,
            vec![
                "bash".to_string(),
                "read_file".to_string(),
                "edit_file".to_string(),
                "Skill".to_string(),
            ]
        );
    }

    #[test]
    fn filter_worker_empty_input() {
        let filtered = filter_tools_for_worker(&[]);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_worker_preserves_all_allowed() {
        let all_tools: Vec<String> = WORKER_ALLOWED_TOOLS.iter().map(|&s| s.to_owned()).collect();
        let filtered = filter_tools_for_worker(&all_tools);
        assert_eq!(filtered.len(), WORKER_ALLOWED_TOOLS.len());
    }

    // ── Default config ────────────────────────────────────────────────

    #[test]
    fn default_config_has_expected_values() {
        let config = CoordinatorConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.system_prompt_prefix, COORDINATOR_PROMPT_PREFIX);
        assert!(!config.restricted_tools.is_empty());
        assert!(!config.allowed_worker_tools.is_empty());
    }

    #[test]
    fn default_config_matches_builder_function() {
        let from_default = CoordinatorConfig::default();
        let from_fn = default_coordinator_config();
        assert_eq!(from_default, from_fn);
    }

    #[test]
    fn default_config_restricted_tools_includes_execution_tools() {
        let config = CoordinatorConfig::default();
        assert!(config.restricted_tools.contains(&"bash".to_string()));
        assert!(config.restricted_tools.contains(&"edit_file".to_string()));
        assert!(config.restricted_tools.contains(&"write_file".to_string()));
    }

    // ── AgentMode defaults ────────────────────────────────────────────

    #[test]
    fn agent_mode_default_is_default_variant() {
        let mode = AgentMode::default();
        assert_eq!(mode, AgentMode::Default);
    }

    // ── CoordinatorState defaults ─────────────────────────────────────

    #[test]
    fn coordinator_state_default() {
        let state = CoordinatorState::default();
        assert_eq!(state.mode, AgentMode::Default);
        assert_eq!(state.tasks_dispatched, 0);
        assert_eq!(state.tasks_completed, 0);
    }
}
