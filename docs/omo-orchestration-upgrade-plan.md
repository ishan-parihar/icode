# oh-my-openagent → icode: Orchestration System Upgrade Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the full agent orchestration system from oh-my-openagent (OmO) into icode's Rust-native architecture, making ultrawork, category+skill delegation, planning workflows, and subsystem-driven execution operational as native icode features.

**Architecture:** Each OmO concept (agents, categories, hooks, workflows) maps to native Rust primitives: structured enums, trait-based agents, event-driven hooks, and a state-machine orchestrator. The system lives as new crates in the icode Rust workspace, preserving icode's memory-safe, event-native design while adopting OmO's operational patterns.

**Tech Stack:** Rust 2021, serde, tokio (async runtime), ratatui (TUI extensions), existing icode crates (runtime, tools, api, commands, plugins).

---

## Architecture Overview

### OmO vs icode: Conceptual Mapping

| OmO Concept | OmO Implementation | icode Native Equivalent |
|---|---|---|
| **Agent** | TypeScript object with prompt/model/permissions | Rust enum + struct with `AgentConfig`, model routing |
| **Category** | JSON config → model resolution | Rust enum `Category` with model fallback chains |
| **Hook** | 52 lifecycle interceptors (TS functions) | Rust trait `Hook` with event enum dispatch |
| **Workflow** | Keyword detection + prompt injection | State machine + `/command` triggers |
| **Background Agent** | OpenCode session spawning | tokio task + session subprocess |
| **Skill** | SKILL.md + embedded MCP | Existing icode skill system + MCP injection |
| **Plan/Boulder** | `.sisyphus/plans/*.md` + `boulder.json` | Same file format, Rust state machine |
| **Delegation (task)** | `task(category/skill)` → new session | `task()` tool → spawn agent subprocess |

### New Crate Structure

```
rust/crates/
├── orchestration/          # NEW - Core orchestration engine
│   ├── agents/             # Agent definitions, registry, model routing
│   ├── categories/         # Category system + model resolution
│   ├── delegation/         # task() tool implementation
│   ├── background/         # Background agent manager
│   └── workflows/          # ultrawork, ralph-loop, state machines
├── hooks-engine/           # NEW - Hook system (52 hooks)
│   ├── lifecycle.rs        # Event types, hook trait, dispatcher
│   ├── core-hooks/         # 24 session-level hooks
│   ├── tool-guard-hooks/   # 14 tool-guard hooks
│   ├── transform-hooks/    # 5 transform hooks
│   ├── continuation-hooks/ # 7 continuation hooks
│   └── skill-hooks/        # 2 skill hooks
├── planning/               # NEW - Planning system
│   ├── plan-parser.rs      # Parse .sisyphus/plans/*.md
│   ├── boulder.rs          # Boulder state management
│   ├── notepad.rs          # Wisdom accumulation system
│   └── plan-commands.rs    # /start-work, /handoff, etc.
├── hashline/               # NEW - Hash-anchored edit tool
├── ast-grep-tools/         # NEW - AST-grep search/replace
├── session-manager/        # NEW - Session list/read/search/info
└── [existing crates stay unchanged]
```

### File Map: What Gets Created/Modified

| Phase | New Files | Modified Files |
|---|---|---|
| Phase 1 | `orchestration/` crate (~20 files) | `Cargo.toml` (workspace), `runtime/` (agent registry) |
| Phase 2 | `hooks-engine/` crate (~30 files) | `runtime/` (hook integration), `tools/` (hook triggers) |
| Phase 3 | `planning/` crate (~10 files) | `commands/` (new slash commands), `icode-cli/` (TUI) |
| Phase 4 | `hashline/`, `ast-grep-tools/` crates | `tools/` (new tool specs), `runtime/` (tool registry) |
| Phase 5 | `session-manager/` crate | `api/` (session API), `icode-cli/` (TUI session view) |
| Phase 6 | Config extensions | `config/` (JSONC support, schema expansion) |

---

## Phase 1: Agent Orchestration Engine

### Task 1.1: Agent Registry & Model Routing

**Files:**
- Create: `rust/crates/orchestration/src/lib.rs`
- Create: `rust/crates/orchestration/src/agent_registry.rs`
- Create: `rust/crates/orchestration/src/agent_config.rs`
- Create: `rust/crates/orchestration/src/model_router.rs`
- Create: `rust/crates/orchestration/src/types.rs`
- Test: `rust/crates/orchestration/tests/agent_registry_test.rs`

- [ ] **Step 1: Define core types**

```rust
// rust/crates/orchestration/src/types.rs

/// Agent mode: primary (respects UI model), subagent (own fallback chain), all
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AgentMode {
    Primary,
    Subagent,
    All,
}

/// Agent permission configuration
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AgentPermissions {
    pub question: PermissionMode,
    pub call_omo_agent: PermissionMode,
    #[serde(flatten)]
    pub tool_overrides: std::collections::HashMap<String, PermissionMode>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PermissionMode {
    Allow,
    Deny,
}

/// Fallback model configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FallbackModel {
    pub model: String,
    pub variant: Option<String>,     // "xhigh", "medium", "max"
    pub thinking: Option<ThinkingConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThinkingConfig {
    pub r#type: String,             // "enabled"
    pub budget_tokens: u32,
}

/// Complete agent configuration (maps to OmO's AgentConfig)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub mode: AgentMode,
    pub model: String,
    pub max_tokens: u32,
    pub prompt: String,
    pub color: String,               // hex color for TUI
    pub permissions: AgentPermissions,
    pub fallback_models: Vec<FallbackModel>,
    pub reasoning_effort: Option<String>,  // "low", "medium", "high", "xhigh"
    pub temperature: Option<f64>,
    pub disabled_tools: Vec<String>,
}
```

- [ ] **Step 2: Build the agent registry**

```rust
// rust/crates/orchestration/src/agent_registry.rs

use std::collections::HashMap;
use crate::types::AgentConfig;

pub struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
    /// Deterministic tab-cycling order (core agents first)
    cycle_order: Vec<String>,
}

impl AgentRegistry {
    pub fn new() -> Self { ... }

    pub fn register(&mut self, config: AgentConfig) { ... }

    pub fn get(&self, name: &str) -> Option<&AgentConfig> { ... }

    pub fn list(&self) -> Vec<&AgentConfig> { ... }

    /// Get next agent in cycle (for Tab switching in TUI)
    pub fn cycle_next(&self, current: &str) -> &str { ... }

    /// Core agent ordering: Sisyphus(1), Hephaestus(2), Prometheus(3), Atlas(4), rest
    pub fn cycle_order(&self) -> &[String] { &self.cycle_order }

    /// Resolve agent with fallback chain
    pub fn resolve_with_fallback(&self, name: &str) -> Option<AgentConfig> { ... }
}
```

- [ ] **Step 3: Build model router (category → model resolution)**

```rust
// rust/crates/orchestration/src/model_router.rs

use crate::types::AgentConfig;

/// Model resolution: override → category-default → provider-fallback → system-default
pub struct ModelRouter {
    available_models: Vec<String>,
    connected_providers: Vec<String>,
}

impl ModelRouter {
    pub fn resolve(&self, agent: &AgentConfig) -> String {
        // 1. Check if primary model is available
        // 2. Walk fallback_models chain
        // 3. Fall back to system default
    }

    pub fn available_providers(&self) -> &[String] { ... }
}
```

- [ ] **Step 4: Wire into existing runtime**

Modify `rust/crates/runtime/src/lib.rs` to accept an `AgentRegistry` reference and use it for agent resolution during session startup.

- [ ] **Step 5: Write tests**

Test agent registration, cycle ordering, model resolution with fallback chains, and permission enforcement.

Run: `cargo test -p orchestration -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add rust/crates/orchestration/ rust/Cargo.toml
git commit -m "feat: add agent registry and model routing crate"
```

---

### Task 1.2: Builtin Agent Definitions

**Files:**
- Create: `rust/crates/orchestration/src/agents/builtin.rs`
- Create: `rust/crates/orchestration/src/agents/prompts/sisyphus.md`
- Create: `rust/crates/orchestration/src/agents/prompts/hephaestus.md`
- Create: `rust/crates/orchestration/src/agents/prompts/oracle.md`
- Create: `rust/crates/orchestration/src/agents/prompts/librarian.md`
- Create: `rust/crates/orchestration/src/agents/prompts/explore.md`
- Create: `rust/crates/orchestration/src/agents/prompts/atlas.md`
- Create: `rust/crates/orchestration/src/agents/prompts/prometheus.md`
- Create: `rust/crates/orchestration/src/agents/prompts/metis.md`
- Create: `rust/crates/orchestration/src/agents/prompts/momus.md`
- Create: `rust/crates/orchestration/src/agents/prompts/junior.md`
- Create: `rust/crates/orchestration/src/agents/prompts/multimodal_looker.md`
- Modify: `rust/crates/orchestration/src/lib.rs` (re-export agents)
- Test: `rust/crates/orchestration/tests/builtin_agents_test.rs`

- [ ] **Step 1: Define builtin agent factory**

```rust
// rust/crates/orchestration/src/agents/builtin.rs

use crate::types::AgentConfig;

/// All 11 builtin agents from oh-my-openagent
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
    AgentConfig {
        name: "sisyphus".into(),
        description: "Powerful AI orchestrator. Plans obsessively with todos, \
            assesses search complexity before exploration, delegates strategically \
            via category+skills combinations.".into(),
        mode: AgentMode::Primary,
        model: "claude-opus-4-6".into(),
        max_tokens: 64000,
        prompt: include_str!("prompts/sisyphus.md").into(),
        color: "#00CED1".into(),
        permissions: AgentPermissions {
            question: PermissionMode::Allow,
            call_omo_agent: PermissionMode::Deny,
            tool_overrides: HashMap::new(),
        },
        fallback_models: vec![
            FallbackModel { model: "kimi-k2.5".into(), variant: None, thinking: None },
            FallbackModel { model: "glm-5".into(), variant: None, thinking: None },
            FallbackModel { model: "gpt-5.4".into(), variant: Some("medium".into()), thinking: None },
        ],
        reasoning_effort: Some("medium".into()),
        temperature: Some(0.1),
        disabled_tools: vec![],
    }
}

// ... implement remaining 10 agents using prompts from oh-my-openagent
```

- [ ] **Step 2: Create prompt templates**

Copy and adapt the 11 agent prompts from oh-my-openagent:
- `sisyphus.md`: From `/oh-my-openagent/src/agents/sisyphus.ts` (the full 552-line prompt)
- `hephaestus.md`: From `/oh-my-openagent/src/agents/hephaestus/agent.ts`
- `oracle.md`: From `/oh-my-openagent/src/agents/oracle.ts`
- `atlas.md`: From `/oh-my-openagent/src/agents/atlas/default.ts`
- `prometheus.md`: From `/oh-my-openagent/src/agents/prometheus/system-prompt.ts`
- `junior.md`: From `/oh-my-openagent/src/agents/sisyphus-junior/`
- etc.

Key adaptation: Replace TypeScript-specific references (e.g., `task(subagent_type=...)`) with icode's tool-call format.

- [ ] **Step 3: Wire into registry**

Update `lib.rs` to auto-register builtin agents on initialization.

- [ ] **Step 4: Write tests**

Test that all 11 agents have valid configs, prompt templates load correctly, and permission configs are correct for read-only agents (oracle, librarian, explore, multimodal-looker).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/orchestration/src/agents/
git commit -m "feat: add 11 builtin agent definitions with prompts"
```

---

### Task 1.3: Category System

**Files:**
- Create: `rust/crates/orchestration/src/categories/mod.rs`
- Create: `rust/crates/orchestration/src/categories/builtin.rs`
- Create: `rust/crates/orchestration/src/categories/resolver.rs`
- Test: `rust/crates/orchestration/tests/categories_test.rs`

- [ ] **Step 1: Define category types**

```rust
// rust/crates/orchestration/src/categories/mod.rs

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CategoryConfig {
    pub name: String,
    pub description: String,
    pub model: String,
    pub variant: Option<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub prompt_append: Option<String>,
    pub thinking: Option<ThinkingConfig>,
    pub reasoning_effort: Option<String>,
    pub text_verbosity: Option<String>,
    pub max_tokens: Option<u32>,
    pub disabled_tools: Vec<String>,
    pub is_unstable_agent: bool,
}

/// Built-in categories (8 from oh-my-openagent)
pub fn builtin_categories() -> Vec<CategoryConfig> {
    vec![
        CategoryConfig {
            name: "visual-engineering".into(),
            description: "Frontend, UI/UX, design, styling, animation".into(),
            model: "google/gemini-3.1-pro".into(),
            variant: None,
            temperature: None,
            top_p: None,
            prompt_append: None,
            thinking: None,
            reasoning_effort: None,
            text_verbosity: None,
            max_tokens: None,
            disabled_tools: vec![],
            is_unstable_agent: false,
        },
        CategoryConfig {
            name: "ultrabrain".into(),
            description: "Deep logical reasoning, complex architecture decisions".into(),
            model: "openai/gpt-5.4".into(),
            variant: Some("xhigh".into()),
            ..Default::default()
        },
        // deep, artistry, quick, unspecified-low, unspecified-high, writing
    ]
}
```

- [ ] **Step 2: Category → AgentConfig resolver**

```rust
// rust/crates/orchestration/src/categories/resolver.rs

use crate::categories::CategoryConfig;
use crate::types::AgentConfig;

/// Resolves a category + skills into a complete AgentConfig for delegation
pub struct CategoryResolver {
    categories: Vec<CategoryConfig>,
    /// User-defined category overrides
    overrides: std::collections::HashMap<String, CategoryConfig>,
}

impl CategoryResolver {
    /// Resolve category name to AgentConfig
    pub fn resolve(&self, category_name: &str, skills: &[String]) -> Option<AgentConfig> {
        // 1. Find category (override → builtin)
        // 2. Apply skill prompt injection
        // 3. Return complete AgentConfig ready for task() delegation
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add rust/crates/orchestration/src/categories/
git commit -m "feat: add category system with model routing"
```

---

## Phase 2: Delegation & Background Agent System

### Task 2.1: task() Tool Implementation

**Files:**
- Create: `rust/crates/orchestration/src/delegation/mod.rs`
- Create: `rust/crates/orchestration/src/delegation/task_tool.rs`
- Create: `rust/crates/orchestration/src/delegation/task_schema.rs`
- Create: `rust/crates/orchestration/src/delegation/prompt_builder.rs`
- Create: `rust/crates/orchestration/src/delegation/executor.rs`
- Test: `rust/crates/orchestration/tests/delegation_test.rs`

- [ ] **Step 1: Define task() tool schema**

```rust
// rust/crates/orchestration/src/delegation/task_schema.rs

/// task() tool input - maps to OmO's delegate-task tool
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskInput {
    /// Category for model routing (mutually exclusive with agent)
    pub category: Option<String>,
    /// Direct agent name (mutually exclusive with category)
    pub subagent_type: Option<String>,
    /// Task prompt - MUST include TASK, EXPECTED OUTCOME, MUST DO, MUST NOT DO, CONTEXT
    pub prompt: String,
    /// Skills to inject (e.g., ["frontend-ui-ux", "git-master"])
    pub load_skills: Vec<String>,
    /// Run in background (default: false)
    pub run_in_background: Option<bool>,
    /// Short description for tracking
    pub description: Option<String>,
    /// Resume existing session
    pub session_id: Option<String>,
    /// Task dependencies (for task system)
    pub blocked_by: Option<Vec<String>>,
    pub blocks: Option<Vec<String>>,
}

/// task() tool output
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskOutput {
    pub task_id: String,          // "bg_abc123" for background, "sync_xyz789" for sync
    pub session_id: String,        // For session continuity
    pub status: TaskStatus,
    pub result: Option<String>,    // Populated on completion (sync only)
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum TaskStatus {
    Spawned,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}
```

- [ ] **Step 2: Implement task executor**

```rust
// rust/crates/orchestration/src/delegation/executor.rs

use tokio::process::Command;

/// Executes a task by spawning a new icode agent subprocess
pub struct TaskExecutor {
    registry: Arc<AgentRegistry>,
    category_resolver: Arc<CategoryResolver>,
    background_manager: Arc<BackgroundManager>,
}

impl TaskExecutor {
    /// Execute a sync task (blocking)
    pub async fn execute_sync(&self, input: TaskInput) -> Result<TaskOutput, Error> {
        // 1. Resolve category/agent → AgentConfig
        // 2. Build prompt (inject skill instructions)
        // 3. Spawn icode subprocess with agent config
        // 4. Wait for completion
        // 5. Return result
    }

    /// Execute a background task (non-blocking)
    pub async fn execute_background(&self, input: TaskInput) -> Result<TaskOutput, Error> {
        // 1. Same resolution as sync
        // 2. Spawn as tokio background task
        // 3. Register with BackgroundManager
        // 4. Return immediately with task_id
    }
}
```

- [ ] **Step 3: Register as tool spec**

Add `task` to the tool surface in `rust/crates/tools/`. The tool spec describes the JSON schema for the LLM.

- [ ] **Step 4: Commit**

```bash
git add rust/crates/orchestration/src/delegation/
git commit -m "feat: implement task() delegation tool"
```

---

### Task 2.2: Background Agent Manager

**Files:**
- Create: `rust/crates/orchestration/src/background/mod.rs`
- Create: `rust/crates/orchestration/src/background/manager.rs`
- Create: `rust/crates/orchestration/src/background/spawner.rs`
- Create: `rust/crates/orchestration/src/background/poller.rs`
- Create: `rust/crates/orchestration/src/background/concurrency.rs`
- Create: `rust/crates/orchestration/src/background/circuit_breaker.rs`
- Create: `rust/crates/orchestration/src/background/types.rs`
- Create: `rust/crates/orchestration/src/background/cleanup.rs`
- Test: `rust/crates/orchestration/tests/background_manager_test.rs`

- [ ] **Step 1: Define background task types**

```rust
// rust/crates/orchestration/src/background/types.rs

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackgroundTask {
    pub id: String,                        // "bg_abc123"
    pub description: String,
    pub session_id: String,
    pub status: BackgroundTaskStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub model: String,
    pub provider: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum BackgroundTaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Stale,  // Session went idle without completing
}
```

- [ ] **Step 2: Implement BackgroundManager**

```rust
// rust/crates/orchestration/src/background/manager.rs

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct BackgroundManager {
    tasks: Arc<RwLock<HashMap<String, BackgroundTask>>>,
    /// Max concurrent tasks per model/provider
    concurrency_limits: HashMap<String, usize>,  // default: 5
    /// Circuit breaker for failing providers
    circuit_breaker: CircuitBreaker,
    /// Notification callback (fires when task completes)
    on_complete: Option<Box<dyn Fn(&BackgroundTask) + Send + Sync>>,
}

impl BackgroundManager {
    pub async fn register(&self, task: BackgroundTask) -> Result<(), Error> {
        // Check concurrency limits
        // Check circuit breaker state
        // Register task
        // Start polling
    }

    pub async fn get_output(&self, task_id: &str) -> Option<&BackgroundTask> { ... }
    pub async fn cancel(&self, task_id: &str) -> Result<(), Error> { ... }
    pub async fn list(&self) -> Vec<&BackgroundTask> { ... }

    /// Called by system when background task completes
    pub async fn on_task_complete(&self, task_id: &str, result: String) { ... }
}
```

- [ ] **Step 3: Implement background_output and background_cancel tools**

Add `background_output` and `background_cancel` to the tool surface.

- [ ] **Step 4: Implement concurrency limiter + circuit breaker**

- [ ] **Step 5: Commit**

---

### Task 2.3: background_output & background_cancel Tools

**Files:**
- Create: `rust/crates/orchestration/src/background/tools.rs`
- Modify: `rust/crates/tools/src/lib.rs` (export new tools)

- [ ] **Step 1: Implement background_output tool spec**

```rust
// Parameters: task_id, full_session (bool), include_thinking (bool),
// include_tool_results (bool), timeout (ms), block (bool),
// since_message_id, message_limit, thinking_max_chars
```

- [ ] **Step 2: Implement background_cancel tool spec**

```rust
// Parameters: taskId (string), all (bool)
```

- [ ] **Step 3: Commit**

---

## Phase 3: Hook System

### Task 3.1: Hook Engine Core

**Files:**
- Create: `rust/crates/hooks-engine/src/lib.rs`
- Create: `rust/crates/hooks-engine/src/trait.rs`
- Create: `rust/crates/hooks-engine/src/event_types.rs`
- Create: `rust/crates/hooks-engine/src/dispatcher.rs`
- Create: `rust/crates/hooks-engine/src/registry.rs`
- Test: `rust/crates/hooks-engine/tests/hook_engine_test.rs`

- [ ] **Step 1: Define hook trait and event types**

```rust
// rust/crates/hooks-engine/src/trait.rs

#[async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn events(&self) -> Vec<HookEvent>;
    fn priority(&self) -> u8 { 50 }  // Lower = higher priority

    /// Called BEFORE tool execution. Can block or modify input.
    async fn on_pre_tool_use(&self, ctx: &mut HookContext, input: &mut ToolInput) -> HookResult {
        Ok(())
    }

    /// Called AFTER tool execution. Can modify output or inject messages.
    async fn on_post_tool_use(&self, ctx: &mut HookContext, output: &mut ToolOutput) -> HookResult {
        Ok(())
    }

    /// Called during message processing. Can transform content.
    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        Ok(())
    }

    /// Called on session lifecycle events.
    async fn on_event(&self, ctx: &mut HookContext, event: &SessionEvent) -> HookResult {
        Ok(())
    }

    /// Called during context transformation.
    async fn on_transform(&self, ctx: &mut HookContext) -> HookResult {
        Ok(())
    }

    /// Called when setting API parameters.
    async fn on_params(&self, ctx: &mut HookContext, params: &mut ApiParams) -> HookResult {
        Ok(())
    }
}

pub type HookResult = Result<(), HookError>;

pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Message,
    SessionEvent,
    Transform,
    Params,
}
```

- [ ] **Step 2: Implement hook dispatcher**

```rust
// rust/crates/hooks-engine/src/dispatcher.rs

pub struct HookDispatcher {
    hooks: Vec<Arc<dyn Hook>>,
    disabled_hooks: HashSet<String>,
}

impl HookDispatcher {
    pub fn register(&mut self, hook: Arc<dyn Hook>) { ... }
    pub fn disable(&mut self, name: &str) { ... }

    /// Execute all hooks for a given event, in priority order
    pub async fn dispatch_pre_tool_use(&self, ctx: &mut HookContext, input: &mut ToolInput) -> HookResult { ... }
    pub async fn dispatch_post_tool_use(&self, ctx: &mut HookContext, output: &mut ToolOutput) -> HookResult { ... }
    // ... etc for all 6 event types
}
```

- [ ] **Step 3: Wire into runtime**

Modify `rust/crates/runtime/` to call hook dispatcher at the appropriate lifecycle points (before/after tool execution, during message processing, on session events).

- [ ] **Step 4: Commit**

---

### Task 3.2: Core Hooks (10 Priority Hooks)

**Files:**
- Create: `rust/crates/hooks-engine/src/hooks/mod.rs`
- Create: `rust/crates/hooks-engine/src/hooks/keyword_detector.rs`
- Create: `rust/crates/hooks-engine/src/hooks/ultrawork.rs`
- Create: `rust/crates/hooks-engine/src/hooks/todo_continuation_enforcer.rs`
- Create: `rust/crates/hooks-engine/src/hooks/comment_checker.rs`
- Create: `rust/crates/hooks-engine/src/hooks/session_recovery.rs`
- Create: `rust/crates/hooks-engine/src/hooks/context_window_monitor.rs`
- Create: `rust/crates/hooks-engine/src/hooks/rules_injector.rs`
- Create: `rust/crates/hooks-engine/src/hooks/tool_output_truncator.rs`
- Create: `rust/crates/hooks-engine/src/hooks/ralph_loop.rs`
- Create: `rust/crates/hooks-engine/src/hooks/start_work.rs`
- Create: `rust/crates/hooks-engine/src/hooks/think_mode.rs`
- Test: `rust/crates/hooks-engine/tests/core_hooks_test.rs`

- [ ] **Step 1: Keyword Detector + Ultrawork Mode**

Port OmO's keyword detector (`/oh-my-openagent/src/hooks/keyword-detector/`):
- Detect `ultrawork`/`ulw` → inject ultrawork prompt
- Detect `search`/`find` → activate search mode (parallel exploration)
- Detect `analyze`/`investigate` → activate analysis mode

```rust
// rust/crates/hooks-engine/src/hooks/keyword_detector.rs

static ULTRAWORK_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(ultrawork|ulw)\b").unwrap());

pub struct KeywordDetector;

impl Hook for KeywordDetector {
    fn name(&self) -> &str { "keyword-detector" }
    fn events(&self) -> Vec<HookEvent> { vec![HookEvent::Message, HookEvent::Params] }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        if ULTRAWORK_PATTERN.is_match(&message.content) {
            ctx.inject_system_message(include_str!("ultrawork_prompt.md"));
        }
        // ... detect search, analyze keywords
        Ok(())
    }
}
```

- [ ] **Step 2: Ultrawork prompt**

Copy from `/oh-my-openagent/src/hooks/keyword-detector/ultrawork/default.ts` (the full ULTRAWORK_DEFAULT_MESSAGE, ~297 lines). Adapt for icode's tool-call format.

- [ ] **Step 3: Todo Continuation Enforcer**

Port OmO's `todo-continuation-enforcer`: When agent goes idle with incomplete todos, inject a system reminder forcing continuation.

- [ ] **Step 4: Comment Checker**

Port OmO's `comment-checker`: Post-tool-use hook that reminds agents to reduce excessive comments (skips BDD, directives, docstrings).

- [ ] **Step 5: Session Recovery**

Port OmO's `session-recovery`: Recover from missing tool results, thinking block issues, empty messages, context window limits.

- [ ] **Step 6: Context Window Monitor**

Track token consumption, warn when approaching limits.

- [ ] **Step 7: Rules Injector**

Port OmO's `rules-injector`: Auto-inject rules from `.sisyphus/rules/` when conditions match.

- [ ] **Step 8: Tool Output Truncator**

Truncate output from Grep, Glob, LSP, AST-grep tools. Dynamically adjust based on context window.

- [ ] **Step 9: Ralph Loop**

Port OmO's ralph-loop hook: Self-referential development loop. Detects `<promise>DONE</promise>` to know when complete.

- [ ] **Step 10: Think Mode**

Auto-detect "think deeply", "ultrathink" keywords and adjust model settings for extended thinking.

- [ ] **Step 11: Start-Work Hook**

Handles `/start-work` command: reads boulder.json, calculates progress, injects continuation prompt.

- [ ] **Step 12: Commit**

```bash
git add rust/crates/hooks-engine/src/hooks/
git commit -m "feat: add 10 core lifecycle hooks"
```

---

## Phase 4: Planning System

### Task 4.1: Plan Parser & Boulder State

**Files:**
- Create: `rust/crates/planning/src/lib.rs`
- Create: `rust/crates/planning/src/plan_parser.rs`
- Create: `rust/crates/planning/src/boulder.rs`
- Create: `rust/crates/planning/src/notepad.rs`
- Create: `rust/crates/planning/src/types.rs`
- Test: `rust/crates/planning/tests/plan_parser_test.rs`

- [ ] **Step 1: Define types (port OmO's boulder-state/types.ts)**

```rust
// rust/crates/planning/src/types.rs

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BoulderState {
    pub active_plan: String,          // Absolute path to plan file
    pub started_at: String,            // ISO timestamp
    pub session_ids: Vec<String>,      // All sessions that worked on this plan
    pub plan_name: String,
    pub agent: Option<String>,         // "atlas"
    pub worktree_path: Option<String>,
    pub task_sessions: HashMap<String, TaskSessionState>,
}

#[derive(Debug, Clone)]
pub struct PlanProgress {
    pub total: usize,
    pub completed: usize,
    pub is_complete: bool,
}

#[derive(Debug, Clone)]
pub struct TopLevelTask {
    pub key: String,                   // "todo:1" or "final-wave:F1"
    pub section: TaskSection,
    pub label: String,                 // "1" or "F1"
    pub title: String,
}

#[derive(Debug, Clone)]
pub enum TaskSection {
    Todo,
    FinalWave,
}
```

- [ ] **Step 2: Implement plan parser**

Parse `.sisyphus/plans/*.md` files, extracting:
- Plan metadata (title, description)
- Checkbox tasks (`- [ ]`, `- [x]`)
- Final wave tasks
- Task dependencies

- [ ] **Step 3: Implement boulder state manager**

```rust
// rust/crates/planning/src/boulder.rs

pub struct BoulderManager {
    state_path: PathBuf,  // .sisyphus/boulder.json
}

impl BoulderManager {
    pub fn load(&self) -> Option<BoulderState> { ... }
    pub fn save(&self, state: &BoulderState) -> Result<(), Error> { ... }
    pub fn clear(&self) -> Result<(), Error> { ... }

    /// Calculate progress from plan file
    pub fn calculate_progress(&self) -> Option<PlanProgress> { ... }
}
```

- [ ] **Step 4: Implement notepad system**

```rust
// rust/crates/planning/src/notepad.rs

/// .sisyphus/notepads/{plan-name}/
/// - learnings.md: Patterns, conventions, successful approaches
/// - decisions.md: Architectural choices and rationales
/// - issues.md: Problems, blockers, gotchas
/// - verification.md: Test results, validation outcomes
/// - problems.md: Unresolved issues, technical debt
```

- [ ] **Step 5: Commit**

---

### Task 4.2: Slash Commands

**Files:**
- Create: `rust/crates/commands/src/start_work.rs`
- Create: `rust/crates/commands/src/handoff.rs`
- Create: `rust/crates/commands/src/init_deep.rs`
- Create: `rust/crates/commands/src/ralph_loop.rs`
- Create: `rust/crates/commands/src/ulw_loop.rs`
- Create: `rust/crates/commands/src/stop_continuation.rs`
- Modify: `rust/crates/commands/src/lib.rs` (export new commands)

- [ ] **Step 1: /start-work command**

```rust
// rust/crates/commands/src/start_work.rs

/// Behavior:
/// 1. Check if .sisyphus/boulder.json exists
/// 2. If YES (RESUME): Read state, calculate progress, inject continuation
/// 3. If NO (INIT): Find most recent plan, create boulder.json, begin execution
/// 4. Switch session agent to Atlas
```

- [ ] **Step 2: /handoff command**

Create structured context summary for continuing work in a new session.

- [ ] **Step 3: /init-deep command**

Generate hierarchical AGENTS.md files throughout the project.

- [ ] **Step 4: /ralph-loop and /ulw-loop commands**

Self-referential dev loop with max iterations and completion detection.

- [ ] **Step 5: /stop-continuation command**

Halt all continuation mechanisms.

- [ ] **Step 6: Commit**

---

## Phase 5: Tool Enhancements

### Task 5.1: Hashline Edit Tool

**Files:**
- Create: `rust/crates/hashline/src/lib.rs`
- Create: `rust/crates/hashline/src/enhance.rs`
- Create: `rust/crates/hashline/src/validate.rs`
- Create: `rust/crates/hashline/src/apply.rs`
- Test: `rust/crates/hashline/tests/hashline_test.rs`

- [ ] **Step 1: Implement hashline read enhancer**

When `read_file` returns content, tag each line with a content hash:
```
11#VK| function hello() {
22#XJ|   return "world";
33#MB| }
```

- [ ] **Step 2: Implement hashline edit validator**

When `edit_file` is called with hash references, validate that the content hash matches the current file state. Reject if stale.

- [ ] **Step 3: Implement hashline edit applier**

Apply edits by hash reference, not by content matching. This eliminates stale-line errors.

- [ ] **Step 4: Register as tool**

Add `hashline_edit` to the tool surface.

- [ ] **Step 5: Commit**

---

### Task 5.2: AST-grep Tools

**Files:**
- Create: `rust/crates/ast-grep-tools/src/lib.rs`
- Create: `rust/crates/ast-grep-tools/src/search.rs`
- Create: `rust/crates/ast-grep-tools/src/replace.rs`
- Test: `rust/crates/ast-grep-tools/tests/ast_grep_test.rs`

- [ ] **Step 1: Add ast-grep dependency**

```toml
# rust/crates/ast-grep-tools/Cargo.toml
[dependencies]
ast-grep-core = "0.30"
ast-grep-config = "0.30"
```

- [ ] **Step 2: Implement ast_grep_search tool**

AST-aware code pattern search across 25 languages with meta-variable support ($VAR, $$$).

- [ ] **Step 3: Implement ast_grep_replace tool**

AST-aware code replacement with pattern rewriting.

- [ ] **Step 4: Commit**

---

### Task 5.3: Session Manager Tools

**Files:**
- Create: `rust/crates/session-manager/src/lib.rs`
- Create: `rust/crates/session-manager/src/list.rs`
- Create: `rust/crates/session-manager/src/read.rs`
- Create: `rust/crates/session-manager/src/search.rs`
- Create: `rust/crates/session-manager/src/info.rs`
- Test: `rust/crates/session-manager/tests/session_manager_test.rs`

- [ ] **Step 1: Implement session_list tool**

List all sessions with metadata (message count, date range, agents used).

- [ ] **Step 2: Implement session_read tool**

Read messages and history from a specific session.

- [ ] **Step 3: Implement session_search tool**

Full-text search across session messages.

- [ ] **Step 4: Implement session_info tool**

Get metadata and statistics about a session.

- [ ] **Step 5: Commit**

---

## Phase 6: Config System Extensions

### Task 6.1: JSONC Support & Schema Expansion

**Files:**
- Modify: `rust/crates/config/src/loader.rs`
- Modify: `rust/crates/config/src/schema.rs`
- Create: `rust/crates/config/src/jsonc.rs`
- Test: `rust/crates/config/tests/jsonc_test.rs`

- [ ] **Step 1: Add JSONC parsing**

```toml
# rust/crates/config/Cargo.toml
[dependencies]
json5 = "0.4"  # Supports comments and trailing commas
```

- [ ] **Step 2: Expand config schema**

Add fields for:
- `agents`: Per-agent overrides (model, temperature, fallback_models, permissions)
- `categories`: Custom category definitions
- `disabled_hooks`: Array of hook names to disable
- `disabled_agents`: Array of agent names to disable
- `background_tasks`: Concurrency limits
- `sisyphus_agent`: sisyphus_agent.disabled, planner_enabled, replace_plan
- `ralph_loop`: enabled, default_max_iterations
- `tmux`: enabled, layout
- `file://` prompt loading

- [ ] **Step 3: Implement file:// prompt loading**

```rust
pub fn load_prompt(value: &str) -> Result<String, Error> {
    if let Some(path) = value.strip_prefix("file://") {
        let expanded = expand_tilde(path);
        std::fs::read_to_string(&expanded)
    } else {
        Ok(value.to_string())
    }
}
```

- [ ] **Step 4: Commit**

---

## Verification & Acceptance Criteria

### Per-Phase Acceptance

| Phase | Acceptance Criteria |
|---|---|
| **Phase 1** | All 11 agents registered and loadable. Tab cycling works in TUI. Model resolution with fallback chains works. |
| **Phase 2** | `task(category="quick")` spawns correct model. `task(run_in_background=true)` returns immediately. `background_output` retrieves results. |
| **Phase 3** | `ultrawork` keyword activates max-performance mode. Todo enforcer yanks idle agents back. Session recovery handles missing tool results. |
| **Phase 4** | `/start-work` reads plan and begins execution. `/handoff` creates context summary. Boulder state persists across sessions. |
| **Phase 5** | Hashline edit rejects stale content. AST-grep searches work across 25 languages. Session manager lists/reads/searches sessions. |
| **Phase 6** | JSONC config with comments loads correctly. `file://` prompts load from disk. Agent overrides apply correctly. |

### System-Wide Acceptance

1. **ultrawork workflow**: User types "ulw fix the auth bug" → keyword detector activates ultrawork mode → agent explores codebase → delegates to specialists → verifies → reports done
2. **Plan → execute workflow**: User describes work to Prometheus → plan created in `.sisyphus/plans/` → `/start-work` → Atlas executes tasks systematically
3. **Ralph loop**: `/ralph-loop "Build REST API"` → agent works continuously → detects DONE → or continues until max iterations
4. **Background agents**: Fire 5+ explore/librarian agents in parallel → continue working → results arrive → synthesize
5. **Category+skill delegation**: `task(category="visual-engineering", load_skills=["frontend-ui-ux"])` → Gemini 3.1 Pro with UI/UX instructions

### Build & Test Commands

```bash
# Full workspace build
cargo build --workspace

# All tests
cargo test --workspace

# Clippy (strict)
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all -- --check
```

All must pass before considering the upgrade complete.

---

## Implementation Order & Dependencies

```
Phase 1 (Agent Registry)
    ↓
Phase 2 (Delegation + Background) ── depends on Phase 1
    ↓
Phase 3 (Hook Engine) ── depends on Phase 1
    ↓
Phase 4 (Planning) ── depends on Phase 2 + Phase 3
    ↓
Phase 5 (Tool Enhancements) ── depends on Phase 1 + Phase 2
    ↓
Phase 6 (Config Extensions) ── can run in parallel with Phases 1-5
```

**Parallelizable:** Phase 6 can start immediately (config work is independent). Phase 5.3 (Session Manager) can start immediately.

**Estimated total:** ~40-60 focused tasks across 6 phases, each 2-5 minutes of implementation time.
