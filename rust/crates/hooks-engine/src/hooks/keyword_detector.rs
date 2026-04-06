use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{Hook, HookResult, Message};

const ULTRAWORK_PROMPT: &str =
    "SYSTEM: Ultrawork mode activated. Use systematic-debugging, dispatching-parallel-agents, and subagent-driven-development skills. Break work into independent tasks and dispatch agents for parallel throughput.";

const SEARCH_PROMPT: &str =
    "SYSTEM: Search mode activated. Use grep, glob, and web search tools thoroughly before acting. Explore the codebase extensively to build a complete mental model.";

const ANALYZE_PROMPT: &str =
    "SYSTEM: Analysis mode activated. Perform deep investigation before proposing solutions. Read extensively, trace dependencies, and understand the full context.";

#[derive(Debug, Clone, Copy)]
pub struct KeywordDetector;

impl KeywordDetector {
    fn matches_any(lower_content: &str, patterns: &[&str]) -> bool {
        patterns
            .iter()
            .any(|p| Self::contains_word_boundary(lower_content, p))
    }

    fn contains_word_boundary(haystack: &str, word: &str) -> bool {
        let haystack = haystack.to_lowercase();
        let word = word.to_lowercase();
        if haystack == word {
            return true;
        }
        haystack.contains(&format!(" {word} "))
            || haystack.starts_with(&format!("{word} "))
            || haystack.ends_with(&format!(" {word}"))
            || haystack.contains(&format!(" {word}."))
            || haystack.contains(&format!(" {word},"))
    }
}

#[async_trait]
impl Hook for KeywordDetector {
    fn name(&self) -> &'static str {
        "keyword-detector"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Message, HookEvent::Params]
    }

    fn priority(&self) -> u8 {
        10
    }

    async fn on_message(&self, ctx: &mut HookContext, message: &mut Message) -> HookResult {
        let lower = message.content.to_lowercase();

        if Self::matches_any(&lower, &["ultrawork", "ulw"]) {
            ctx.inject_message(ULTRAWORK_PROMPT.to_string());
        }
        if Self::matches_any(&lower, &["search", "find"]) {
            ctx.inject_message(SEARCH_PROMPT.to_string());
        }
        if Self::matches_any(
            &lower,
            &["analyze", "investigate", "deep dive", "deep-dive"],
        ) {
            ctx.inject_message(ANALYZE_PROMPT.to_string());
        }

        Ok(())
    }

    async fn on_params(&self, _ctx: &mut HookContext, _params: &mut crate::hook_trait::ApiParams) -> HookResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ultrawork_keyword() {
        let hook = KeywordDetector;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Let's do some ultrawork on this".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
        assert!(ctx.injected_messages[0].contains("Ultrawork"));
    }

    #[test]
    fn detects_search_keyword() {
        let hook = KeywordDetector;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Search for the bug".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn detects_analyze_keyword() {
        let hook = KeywordDetector;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Analyze this code".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 1);
    }

    #[test]
    fn no_match_for_unrelated_content() {
        let hook = KeywordDetector;
        let mut ctx = HookContext::new();
        let mut msg = Message {
            role: "user".into(),
            content: "Hello world".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_message(&mut ctx, &mut msg)).unwrap();
        assert_eq!(ctx.injected_messages.len(), 0);
    }
}
