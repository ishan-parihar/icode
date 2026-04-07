use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{ApiParams, Hook, HookResult};

const CONTEXT_WARNING: &str =
    "WARNING: Context usage exceeds 80% threshold. Consider compacting the conversation or starting a new session to avoid token limits.";

const THRESHOLD_PCT: f64 = 80.0;

#[derive(Debug)]
pub struct ContextWindowMonitor;

#[async_trait]
impl Hook for ContextWindowMonitor {
    fn name(&self) -> &'static str {
        "context-window-monitor"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Params]
    }

    fn priority(&self) -> u8 {
        30
    }

    async fn on_params(&self, ctx: &mut HookContext, params: &mut ApiParams) -> HookResult {
        let usage_pct = ctx
            .get_metadata("context_usage_pct")
            .and_then(|v| v.parse::<f64>().ok());

        if let Some(pct) = usage_pct {
            if pct >= THRESHOLD_PCT {
                ctx.inject_message(format!("{CONTEXT_WARNING} Current usage: {pct:.1}%"));
            }
        }

        if let Some(ref effort) = params.reasoning_effort {
            if effort == "high" {
                ctx.set_metadata("reasoning_mode".into(), "extended".into());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_above_threshold() {
        let hook = ContextWindowMonitor;
        let mut ctx = HookContext::new();
        ctx.set_metadata("context_usage_pct".into(), "85.5".into());
        let mut params = ApiParams {
            model: "sonnet".into(),
            temperature: None,
            max_tokens: None,
            reasoning_effort: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_params(&mut ctx, &mut params)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
        assert!(ctx.injected_messages[0].contains("80%"));
    }

    #[test]
    fn no_warning_below_threshold() {
        let hook = ContextWindowMonitor;
        let mut ctx = HookContext::new();
        ctx.set_metadata("context_usage_pct".into(), "50.0".into());
        let mut params = ApiParams {
            model: "sonnet".into(),
            temperature: None,
            max_tokens: None,
            reasoning_effort: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_params(&mut ctx, &mut params)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }

    #[test]
    fn tracks_reasoning_effort() {
        let hook = ContextWindowMonitor;
        let mut ctx = HookContext::new();
        let mut params = ApiParams {
            model: "sonnet".into(),
            temperature: None,
            max_tokens: None,
            reasoning_effort: Some("high".into()),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_params(&mut ctx, &mut params)).unwrap();
        assert_eq!(ctx.get_metadata("reasoning_mode"), Some("extended"));
    }

    #[test]
    fn handles_missing_metadata_gracefully() {
        let hook = ContextWindowMonitor;
        let mut ctx = HookContext::new();
        let mut params = ApiParams {
            model: "sonnet".into(),
            temperature: None,
            max_tokens: None,
            reasoning_effort: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_params(&mut ctx, &mut params)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }
}
