use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{Hook, HookResult, Message};

const TODO_WARNING: &str =
    "WARNING: You marked work as done but there are still incomplete todos. Do NOT mark tasks complete until all TODOs are resolved. Continue working on pending items.";

#[derive(Debug)]
pub struct TodoContinuationEnforcer;

fn has_completion_phrase(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("done") || lower.contains("complete") || lower.contains("finished")
}

fn has_pending_todos(ctx: &HookContext) -> bool {
    ctx.get_metadata("pending_todos")
        .and_then(|v| v.parse::<u32>().ok())
        .is_some_and(|n| n > 0)
}

#[async_trait]
impl Hook for TodoContinuationEnforcer {
    fn name(&self) -> &'static str {
        "todo-continuation-enforcer"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Message]
    }

    fn priority(&self) -> u8 {
        20
    }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        if message.role == "assistant"
            && has_completion_phrase(&message.content)
            && has_pending_todos(ctx)
        {
            ctx.inject_message(TODO_WARNING.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_on_done_with_pending() {
        let hook = TodoContinuationEnforcer;
        let mut ctx = HookContext::new();
        ctx.set_metadata("pending_todos".into(), "3".into());
        let mut msg = Message {
            role: "assistant".into(),
            content: "Done with the implementation".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn no_warning_without_pending() {
        let hook = TodoContinuationEnforcer;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "assistant".into(),
            content: "Done with the implementation".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }

    #[test]
    fn no_warning_for_user_messages() {
        let hook = TodoContinuationEnforcer;
        let mut ctx = HookContext::new();
        ctx.set_metadata("pending_todos".into(), "3".into());
        let mut msg = Message {
            role: "user".into(),
            content: "Done".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }

    #[test]
    fn detects_finished_keyword() {
        let hook = TodoContinuationEnforcer;
        let mut ctx = HookContext::new();
        ctx.set_metadata("pending_todos".into(), "1".into());
        let mut msg = Message {
            role: "assistant".into(),
            content: "Finished all tasks".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }
}
