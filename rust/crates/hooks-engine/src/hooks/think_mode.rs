use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{ApiParams, Hook, HookResult, Message};

const THINK_INSTRUCTION: &str =
    "SYSTEM: Extended thinking mode activated. Think deeply about the problem before responding. Break down complex issues, consider edge cases, and reason step by step.";

#[derive(Debug)]
pub struct ThinkMode;

fn has_think_keyword(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("think deeply")
        || lower.contains("ultrathink")
        || lower.contains("think hard")
        || lower.contains("reason step by step")
        || lower.contains("deep thinking")
}

#[async_trait]
impl Hook for ThinkMode {
    fn name(&self) -> &'static str {
        "think-mode"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Message, HookEvent::Params]
    }

    fn priority(&self) -> u8 {
        15
    }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        if has_think_keyword(&message.content) {
            ctx.inject_message(THINK_INSTRUCTION.to_string());
            ctx.set_metadata("thinking_mode".into(), "extended".into());
        }
        Ok(())
    }

    async fn on_params(&self, ctx: &mut HookContext, params: &mut ApiParams) -> HookResult {
        if ctx.get_metadata("thinking_mode") == Some("extended") {
            params.reasoning_effort = Some("high".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activates_on_think_deeply() {
        let hook = ThinkMode;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Think deeply about this".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn activates_on_ultrathink() {
        let hook = ThinkMode;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "ultrathink this problem".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn sets_high_reasoning_effort() {
        let hook = ThinkMode;
        let mut ctx = HookContext::new();
        ctx.set_metadata("thinking_mode".into(), "extended".into());
        let mut params = ApiParams {
            model: "sonnet".into(),
            temperature: None,
            max_tokens: None,
            reasoning_effort: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_params(&mut ctx, &mut params)).unwrap();
        assert_eq!(params.reasoning_effort.as_deref(), Some("high"));
    }

    #[test]
    fn no_activation_for_normal_message() {
        let hook = ThinkMode;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Write a function".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }
}
