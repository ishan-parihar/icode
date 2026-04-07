use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{Hook, HookResult, ToolOutput};

const TRUNCATION_SUFFIX: &str = "... (truncated)";
const DEFAULT_THRESHOLD: usize = 10_000;

const TRUNCATABLE_TOOLS: &[&str] = &[
    "grep_search",
    "glob_search",
    "symbols",
    "references",
    "diagnostics",
];

#[derive(Debug)]
pub struct ToolOutputTruncator {
    threshold: usize,
}

impl ToolOutputTruncator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            threshold: DEFAULT_THRESHOLD,
        }
    }

    #[must_use]
    pub fn with_threshold(threshold: usize) -> Self {
        Self { threshold }
    }
}

impl Default for ToolOutputTruncator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for ToolOutputTruncator {
    fn name(&self) -> &'static str {
        "tool-output-truncator"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::PostToolUse]
    }

    fn priority(&self) -> u8 {
        40
    }

    async fn on_post_tool_use(&self, ctx: &mut HookContext, output: &mut ToolOutput) -> HookResult {
        if TRUNCATABLE_TOOLS.contains(&output.tool_name.as_str())
            && output.result.len() > self.threshold
        {
            let truncated = output
                .result
                .chars()
                .take(self.threshold)
                .collect::<String>();
            output.result = format!("{truncated}{TRUNCATION_SUFFIX}");
            ctx.add_warning(format!(
                "Output from '{}' truncated to {} characters",
                output.tool_name, self.threshold
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_oversized_output() {
        let hook = ToolOutputTruncator::with_threshold(100);
        let mut ctx = HookContext::new();
        let mut output = ToolOutput {
            tool_name: "grep_search".into(),
            result: "x".repeat(200),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_post_tool_use(&mut ctx, &mut output))
            .unwrap();
        assert!(output.result.contains(TRUNCATION_SUFFIX));
        assert_eq!(ctx.warnings.len(), 1);
    }

    #[test]
    fn does_not_truncate_small_output() {
        let hook = ToolOutputTruncator::new();
        let mut ctx = HookContext::new();
        let mut output = ToolOutput {
            tool_name: "grep_search".into(),
            result: "small result".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_post_tool_use(&mut ctx, &mut output))
            .unwrap();
        assert!(!output.result.contains(TRUNCATION_SUFFIX));
    }

    #[test]
    fn does_not_truncate_non_truncatable_tool() {
        let hook = ToolOutputTruncator::with_threshold(10);
        let mut ctx = HookContext::new();
        let mut output = ToolOutput {
            tool_name: "bash".into(),
            result: "x".repeat(200),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_post_tool_use(&mut ctx, &mut output))
            .unwrap();
        assert!(!output.result.contains(TRUNCATION_SUFFIX));
    }
}
