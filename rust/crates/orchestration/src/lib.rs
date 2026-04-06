pub mod agent_config;
pub mod agent_registry;
pub mod agents;
pub mod background;
pub mod categories;
pub mod delegation;
pub mod model_router;
pub mod types;

pub use agent_config::{allow_all_permissions, deny_all_permissions, AgentConfigBuilder};
pub use agent_registry::AgentRegistry;
pub use agents::builtin_agents;
pub use background::manager::BackgroundManager;
pub use background::tools::{
    background_cancel_tool_spec, background_output_tool_spec, background_tool_specs,
};
pub use background::types::{BackgroundTask, BackgroundTaskStatus};
pub use categories::{builtin_categories, CategoryConfig, CategoryResolver};
pub use delegation::{PromptBuilder, TaskExecutor, TaskInput, TaskOutput, TaskStatus};
pub use model_router::ModelRouter;
pub use types::{
    AgentConfig, AgentMode, AgentPermissions, FallbackModel, PermissionMode, ThinkingConfig,
};
