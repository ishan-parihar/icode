/// Handle `/stop-continuation` command.
///
/// Returns a prompt that instructs the agent to stop all continuation
/// mechanisms including ralph loops, todo continuation, and boulder cycles.
pub fn handle_stop_continuation() -> String {
    "Stop Continuation

All continuation mechanisms have been disabled for this session.

## Stopped
- Ralph Loop: terminated
- Todo Continuation: disabled
- Boulder Loop: halted

The agent should NOT auto-continue or re-trigger any loop mechanisms.
Respond only to direct user instructions from this point forward."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_continuation_returns_clear_message() {
        let output = handle_stop_continuation();
        assert!(output.contains("Stop Continuation"));
        assert!(output.contains("Ralph Loop"));
        assert!(output.contains("Todo Continuation"));
        assert!(output.contains("Boulder Loop"));
    }
}
