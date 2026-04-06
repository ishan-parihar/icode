use std::fs;
use std::path::Path;

use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{ApiParams, Hook, HookResult};

const RULES_DIR: &str = ".sisyphus/rules/";

#[derive(Debug)]
pub struct RulesInjector {
    base_path: String,
}

impl RulesInjector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            base_path: RULES_DIR.to_string(),
        }
    }

    #[must_use]
    pub fn with_base_path(base_path: String) -> Self {
        Self { base_path }
    }

    fn load_rules(&self) -> Vec<String> {
        let path = Path::new(&self.base_path);
        if !path.exists() || !path.is_dir() {
            return Vec::new();
        }

        fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .filter_map(|e| fs::read_to_string(e.path()).ok())
            .collect()
    }
}

impl Default for RulesInjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for RulesInjector {
    fn name(&self) -> &'static str {
        "rules-injector"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::Params]
    }

    fn priority(&self) -> u8 {
        15
    }

    async fn on_params(&self, ctx: &mut HookContext, _params: &mut ApiParams) -> HookResult {
        for rule_content in self.load_rules() {
            ctx.inject_message(format!("RULE: {rule_content}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_empty_when_no_rules_dir() {
        let hook = RulesInjector::with_base_path("/nonexistent/path/rules".into());
        let rules = hook.load_rules();
        assert!(rules.is_empty());
    }

    #[test]
    fn injects_no_rules_when_dir_missing() {
        let hook = RulesInjector::with_base_path("/no/such/dir".into());
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
