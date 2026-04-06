use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::event_types::SessionEvent;

pub type HookResult = Result<(), String>;

/// Input for `PreToolUse` hooks — can modify or block tool input
pub struct ToolInput {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// Output for `PostToolUse` hooks — can modify tool output
pub struct ToolOutput {
    pub tool_name: String,
    pub result: String,
}

/// A message being processed
pub struct Message {
    pub role: String,
    pub content: String,
}

/// API parameters being configured
pub struct ApiParams {
    pub model: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub reasoning_effort: Option<String>,
}

#[async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn events(&self) -> Vec<HookEvent>;
    fn priority(&self) -> u8 {
        50
    }

    async fn on_pre_tool_use(&self, _ctx: &mut HookContext, _input: &mut ToolInput) -> HookResult {
        Ok(())
    }

    async fn on_post_tool_use(
        &self,
        _ctx: &mut HookContext,
        _output: &mut ToolOutput,
    ) -> HookResult {
        Ok(())
    }

    async fn on_message(&self, _ctx: &mut HookContext, _message: &mut Message) -> HookResult {
        Ok(())
    }

    async fn on_event(&self, _ctx: &mut HookContext, _event: &SessionEvent) -> HookResult {
        Ok(())
    }

    async fn on_transform(&self, _ctx: &mut HookContext) -> HookResult {
        Ok(())
    }

    async fn on_params(&self, _ctx: &mut HookContext, _params: &mut ApiParams) -> HookResult {
        Ok(())
    }
}
