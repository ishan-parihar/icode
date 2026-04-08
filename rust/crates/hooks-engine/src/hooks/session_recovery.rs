use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::{HookEvent, SessionEvent, SessionEventType};
use crate::hook_trait::{Hook, HookResult, Message};

const EMPTY_MSG_RECOVERY: &str =
    "RECOVERY: Empty message detected. Please provide meaningful content or use appropriate commands.";
const MALFORMED_RECOVERY: &str =
    "RECOVERY: Malformed message detected. Please resend your message with valid content.";

#[derive(Debug)]
pub struct SessionRecovery;

#[async_trait]
impl Hook for SessionRecovery {
    fn name(&self) -> &'static str {
        "session-recovery"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Message, HookEvent::SessionEvent]
    }

    fn priority(&self) -> u8 {
        5
    }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        if message.content.trim().is_empty() {
            ctx.inject_message(EMPTY_MSG_RECOVERY.to_string());
        } else if message.content.len() > 1_000_000 {
            ctx.inject_message(MALFORMED_RECOVERY.to_string());
        }
        Ok(())
    }

    async fn on_event(&self, ctx: &mut HookContext, event: &SessionEvent) -> HookResult {
        if let SessionEventType::Error(ref reason) = event.r#type {
            ctx.inject_message(format!(
                "RECOVERY: Session error occurred: {reason}. Attempting to recover state."
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_from_empty_message() {
        let hook = SessionRecovery;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: String::new(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn recovers_from_whitespace_only() {
        let hook = SessionRecovery;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "   \n\t  ".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn recovers_from_oversized_message() {
        let hook = SessionRecovery;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "a".repeat(1_000_001),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn recovers_from_session_error() {
        let hook = SessionRecovery;
        let mut ctx = HookContext::new();
        let event = SessionEvent {
            r#type: SessionEventType::Error("timeout".into()),
            session_id: "test".into(),
            timestamp: "now".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_event(&mut ctx, &event)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn no_recovery_for_valid_message() {
        let hook = SessionRecovery;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Hello".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }
}
