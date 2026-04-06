use crate::agent_registry::AgentRegistry;
use crate::categories::CategoryResolver;
use crate::types::AgentConfig;

use super::prompt_builder::PromptBuilder;
use super::task_schema::{TaskInput, TaskOutput, TaskStatus};

use std::sync::atomic::{AtomicU64, Ordering};

static TASK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Executes delegated tasks by validating input, resolving agents, and building prompts.
pub struct TaskExecutor {
    registry: AgentRegistry,
    category_resolver: CategoryResolver,
}

impl TaskExecutor {
    #[must_use]
    pub fn new(registry: AgentRegistry, category_resolver: CategoryResolver) -> Self {
        Self {
            registry,
            category_resolver,
        }
    }

    /// Validate task input: exactly one of `category` or `subagent_type` must be set.
    pub fn validate(input: &TaskInput) -> Result<(), String> {
        match (&input.category, &input.subagent_type) {
            (None, None) => Err("task requires either 'category' or 'subagent_type'".into()),
            (Some(_), Some(_)) => {
                Err("task must specify either 'category' or 'subagent_type', not both".into())
            }
            _ => Ok(()),
        }
    }

    /// Resolve the `AgentConfig` for a task input.
    pub fn resolve_agent(&self, input: &TaskInput) -> Result<AgentConfig, String> {
        if let Some(ref category) = input.category {
            self.category_resolver
                .resolve(category, &input.load_skills, &[])
                .ok_or_else(|| format!("unknown category: {category}"))
        } else if let Some(ref subagent) = input.subagent_type {
            self.registry
                .get_owned(subagent)
                .ok_or_else(|| format!("unknown subagent: {subagent}"))
        } else {
            Err("no category or subagent_type specified".into())
        }
    }

    /// Execute a synchronous task stub.
    ///
    /// Returns a `TaskOutput` with `Spawned` status. The actual LLM API call
    /// is handled by the runtime layer.
    pub fn execute_sync(&self, input: &TaskInput) -> Result<TaskOutput, String> {
        Self::validate(input)?;
        self.resolve_agent(input)?;

        let counter = TASK_COUNTER.fetch_add(1, Ordering::SeqCst);
        let task_id = format!("sync_{counter:04x}");

        Ok(TaskOutput {
            task_id,
            session_id: "pending".into(),
            status: TaskStatus::Spawned,
            result: None,
        })
    }

    /// Build the final prompt for a task with skill injection.
    #[must_use]
    pub fn build_prompt(
        &self,
        base_prompt: &str,
        skills: &[String],
        skill_prompts: &[String],
    ) -> String {
        PromptBuilder::build(base_prompt, skills, skill_prompts)
    }
}
