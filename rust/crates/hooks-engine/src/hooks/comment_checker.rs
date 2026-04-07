use async_trait::async_trait;

use crate::context::HookContext;
use crate::event_types::HookEvent;
use crate::hook_trait::{Hook, HookResult, ToolOutput};

const COMMENT_WARNING: &str =
    "WARNING: Excessive code comments detected. Prefer self-documenting code over explanatory comments. Remove BDD comments, directive comments (TODO, HACK, FIXME), and unnecessary docstrings.";

#[derive(Debug)]
pub struct CommentChecker;

fn has_excessive_comments(content: &str) -> bool {
    let patterns = [
        "// TODO", "// HACK", "// FIXME", "/// ", "/**", "# TODO", "/*", "BDD", "given", "when",
        "then",
    ];
    let count = patterns.iter().filter(|&&p| content.contains(p)).count();
    count >= 3
}

#[async_trait]
impl Hook for CommentChecker {
    fn name(&self) -> &'static str {
        "comment-checker"
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::PostToolUse]
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn on_post_tool_use(&self, ctx: &mut HookContext, output: &mut ToolOutput) -> HookResult {
        if has_excessive_comments(&output.result) {
            ctx.add_warning(COMMENT_WARNING.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_excessive_comments() {
        let hook = CommentChecker;
        let mut ctx = HookContext::new();
        let mut output = ToolOutput {
            tool_name: "write_file".into(),
            result: "// TODO: fix this\n// HACK: workaround\n/// docstring here\n// FIXME: broken"
                .into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_post_tool_use(&mut ctx, &mut output))
            .unwrap();
        assert_eq!(ctx.warnings.len(), 1);
    }

    #[test]
    fn no_warning_for_few_comments() {
        let hook = CommentChecker;
        let mut ctx = HookContext::new();
        let mut output = ToolOutput {
            tool_name: "write_file".into(),
            result: "fn main() { println!(\"hello\"); }".into(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(hook.on_post_tool_use(&mut ctx, &mut output))
            .unwrap();
        assert_eq!(ctx.warnings.len(), 0);
    }
}
