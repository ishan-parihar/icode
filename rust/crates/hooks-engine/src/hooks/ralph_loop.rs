use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{Hook, HookResult, Message};

const CONTINUATION_PROMPT: &str =
    "SYSTEM: The agent claims to be done but there is still work remaining. Do NOT stop. Continue executing until all objectives are fully completed. Use /ralph-loop if needed for self-referential development.";

#[derive(Debug)]
pub struct RalphLoop;

fn has_completion_claim(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("done")
        || lower.contains("complete")
        || lower.contains("finished")
        || lower.contains("all done")
        || lower.contains("work is done")
}

fn has_continuation_indicator(ctx: &HookContext) -> bool {
    ctx.get_metadata("work_remaining")
        .is_some_and(|v| !v.is_empty() && v != "0")
}

#[async_trait]
impl Hook for RalphLoop {
    fn name(&self) -> &'static str {
        "ralph-loop"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Message]
    }

    fn priority(&self) -> u8 {
        25
    }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        if message.role == "assistant"
            && has_completion_claim(&message.content)
            && has_continuation_indicator(ctx)
        {
            ctx.inject_message(CONTINUATION_PROMPT.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_when_done_but_work_remains() {
        let hook = RalphLoop;
        let mut ctx = HookContext::new();
        ctx.set_metadata("work_remaining".into(), "3 tasks left".into());
        let mut msg = Message {
            role: "assistant".into(),
            content: "All done with the work".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn no_injection_when_truly_done() {
        let hook = RalphLoop;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "assistant".into(),
            content: "All done with the work".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }

    #[test]
    fn no_injection_for_user_messages() {
        let hook = RalphLoop;
        let mut ctx = HookContext::new();
        ctx.set_metadata("work_remaining".into(), "more".into());
        let mut msg = Message {
            role: "user".into(),
            content: "I'm done".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }
}
