/// Handle `/ralph-loop` command.
///
/// Returns a prompt that instructs the agent to iterate in a self-referential
/// development loop until the task is complete.
pub fn handle_ralph_loop(max_iterations: usize) -> String {
    format!(
        r#"Ralph Loop Activated

You are now in a self-referential development loop. Your goal is to iterate
until the task is fully complete. Do NOT stop until all acceptance criteria
are met.

## Rules
1. Implement the task step by step
2. After each change, verify correctness (build, lint, test)
3. Fix any issues found during verification
4. Repeat until everything passes cleanly
5. Maximum iterations: {max_iterations}

## Exit Conditions
- All builds pass with zero warnings
- All tests pass
- All clippy checks pass
- You have reached {max_iterations} iterations (stop even if incomplete)

Begin working on the current task now."#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ralph_loop_contains_max_iterations() {
        let output = handle_ralph_loop(5);
        assert!(output.contains("5"));
        assert!(output.contains("Ralph Loop"));
    }

    #[test]
    fn ralph_loop_default_iterations() {
        let output = handle_ralph_loop(10);
        assert!(output.contains("10"));
    }
}
