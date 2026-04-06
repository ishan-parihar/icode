/// Handle `/ulw-loop` command.
///
/// Returns an ultrawork mode activation prompt.
pub fn handle_ulw_loop() -> String {
    r#"Ultrawork Loop Activated

You are now in ULTRAWORK mode. This mode enables maximum effort execution
with no early termination.

## Rules
1. Continue working until the task is demonstrably complete
2. Do NOT stop at the first sign of success — verify thoroughly
3. Run all applicable checks: build, test, lint, clippy
4. Fix every issue found, not just the first one
5. Only stop when ALL verification passes with zero issues

## Mindset
- Exhaustive, not expedient
- Thorough, not surface-level
- Complete, not "good enough"

Begin ultrawork execution now."#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulw_loop_returns_activation_prompt() {
        let output = handle_ulw_loop();
        assert!(output.contains("Ultrawork"));
        assert!(output.contains("ULTRAWORK"));
        assert!(output.contains("verify"));
    }
}
