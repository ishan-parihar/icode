use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{Hook, HookResult, Message};

const ULTRAWORK_SYSTEM_MSG: &str =
    "SYSTEM: ULTRAWORK MODE — Execute all tasks with maximum thoroughness. Use parallel agents for independent subtasks. Never mark work complete without verification. Continue until all goals are fully achieved.";

#[derive(Debug)]
pub struct Ultrawork;

#[async_trait]
impl Hook for Ultrawork {
    fn name(&self) -> &'static str {
        "ultrawork"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Message]
    }

    fn priority(&self) -> u8 {
        5
    }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        let lower = message.content.to_lowercase();
        if lower.contains("ultrawork") || lower.contains("ulw") {
            ctx.inject_message(ULTRAWORK_SYSTEM_MSG.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_on_ultrawork() {
        let hook = Ultrawork;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "start ultrawork".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn injects_on_ulw() {
        let hook = Ultrawork;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "ulw mode on".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn no_inject_without_keyword() {
        let hook = Ultrawork;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "normal work".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }
}
