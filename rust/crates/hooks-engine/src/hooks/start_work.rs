use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{Hook, HookResult, Message};

const START_WORK_PROMPT: &str =
    "SYSTEM: /start-work command detected. Read the current boulder state, calculate remaining progress, and inject a continuation prompt to resume work from the last checkpoint.";

#[derive(Debug)]
pub struct StartWork;

fn is_start_work_command(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("/start-work") || lower.contains("start work")
}

#[async_trait]
impl Hook for StartWork {
    fn name(&self) -> &'static str {
        "start-work"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Message]
    }

    fn priority(&self) -> u8 {
        10
    }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        if is_start_work_command(&message.content) {
            ctx.inject_message(START_WORK_PROMPT.to_string());

            if let Some(state) = ctx.get_metadata("boulder_state") {
                ctx.set_metadata("last_checkpoint".into(), state.to_string());
            }

            let progress = ctx.get_metadata("work_progress").unwrap_or("0");
            ctx.set_metadata(
                "remaining_work".into(),
                format!("Progress so far: {progress}. Continue from here."),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_slash_command() {
        let hook = StartWork;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "/start-work".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn detects_text_command() {
        let hook = StartWork;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Let's start work".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn sets_checkpoint_from_boulder() {
        let hook = StartWork;
        let mut ctx = HookContext::new();
        ctx.set_metadata("boulder_state".into(), "phase-3-complete".into());
        let mut msg = Message {
            role: "user".into(),
            content: "/start-work".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(
            ctx.get_metadata("last_checkpoint"),
            Some("phase-3-complete")
        );
    }

    #[test]
    fn no_action_for_unrelated_message() {
        let hook = StartWork;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "What time is it".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }
}
