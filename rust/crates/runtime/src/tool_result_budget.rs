use crate::session::{ContentBlock, ConversationMessage};

/// Default character budget for cumulative tool result output.
pub const DEFAULT_TOOL_RESULT_BUDGET: usize = 50_000;

const TRUNCATION_PLACEHOLDER: &str = "[tool result truncated to save context]";

/// Sum the character lengths of all `ToolResult` content blocks across messages.
#[must_use]
pub fn total_tool_result_chars(messages: &[ConversationMessage]) -> usize {
    messages
        .iter()
        .flat_map(|msg| &msg.blocks)
        .filter_map(|block| match block {
            ContentBlock::ToolResult { output, .. } => Some(output.chars().count()),
            _ => None,
        })
        .sum()
}

/// Enforce a character budget on tool result content.
///
/// Scans all messages, sums the char lengths of every `ToolResult` block,
/// and if the total exceeds `budget`, replaces the **oldest** tool results
/// with a truncation placeholder until the total fits under budget.
///
/// Returns the number of tool results that were truncated.
#[must_use]
pub fn apply_tool_result_budget(messages: &mut [ConversationMessage], budget: usize) -> usize {
    let entries: Vec<(usize, usize, usize)> = messages
        .iter()
        .enumerate()
        .flat_map(|(mi, msg)| {
            msg.blocks
                .iter()
                .enumerate()
                .filter_map(move |(bi, block)| match block {
                    ContentBlock::ToolResult { output, .. } => {
                        Some((mi, bi, output.chars().count()))
                    }
                    _ => None,
                })
        })
        .collect();

    let total: usize = entries.iter().map(|(_, _, c)| *c).sum();
    if total <= budget {
        return 0;
    }

    let mut truncated = 0;
    let mut freed = 0;
    let needed = total - budget;

    for (mi, bi, char_count) in &entries {
        if freed >= needed {
            break;
        }
        if let Some(ContentBlock::ToolResult { output, .. }) =
            messages.get_mut(*mi).and_then(|m| m.blocks.get_mut(*bi))
        {
            *output = TRUNCATION_PLACEHOLDER.to_string();
        }
        freed += char_count;
        truncated += 1;
    }

    truncated
}

/// Read the `ICODE_TOOL_RESULT_BUDGET` environment variable.
///
/// Returns `Some(value)` when the variable is set to a positive integer,
/// otherwise `None`.
#[must_use]
pub fn tool_result_budget_from_env() -> Option<usize> {
    let raw = std::env::var("ICODE_TOOL_RESULT_BUDGET").ok()?;
    let value = raw.parse::<usize>().ok()?;
    if value > 0 {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_result_message(output: &str) -> ConversationMessage {
        ConversationMessage::tool_result("id", "test_tool", output, false)
    }

    #[test]
    fn no_truncation_when_under_budget() {
        let mut messages = vec![tool_result_message(&"x".repeat(10_000))];
        let budget = 50_000;

        let truncated = apply_tool_result_budget(&mut messages, budget);

        assert_eq!(truncated, 0);
        assert_eq!(total_tool_result_chars(&messages), 10_000);
    }

    #[test]
    fn truncates_oldest_first_when_over_budget() {
        // 3 tool results: 20K + 20K + 20K = 60K, budget 50K
        let mut messages = vec![
            tool_result_message(&"a".repeat(20_000)),
            tool_result_message(&"b".repeat(20_000)),
            tool_result_message(&"c".repeat(20_000)),
        ];
        let budget = 50_000;

        let truncated = apply_tool_result_budget(&mut messages, budget);

        assert_eq!(truncated, 1);
        // Oldest (first) should be truncated
        if let ContentBlock::ToolResult { output, .. } = &messages[0].blocks[0] {
            assert_eq!(output, TRUNCATION_PLACEHOLDER);
        } else {
            panic!("first message should be ToolResult");
        }
        // Remaining two should be intact
        if let ContentBlock::ToolResult { output, .. } = &messages[1].blocks[0] {
            assert_eq!(output.chars().count(), 20_000);
        }
        if let ContentBlock::ToolResult { output, .. } = &messages[2].blocks[0] {
            assert_eq!(output.chars().count(), 20_000);
        }
    }

    #[test]
    fn empty_messages_zero_chars() {
        let messages: Vec<ConversationMessage> = vec![];
        assert_eq!(total_tool_result_chars(&messages), 0);
    }

    #[test]
    fn env_var_parsing() {
        // We cannot easily set env vars in tests without a lock,
        // so test the parsing logic directly via a helper approach.
        // Instead, verify the function returns None when unset.
        // (ICODE_TOOL_RESULT_BUDGET is unlikely set in test env)
        std::env::remove_var("ICODE_TOOL_RESULT_BUDGET");
        assert!(tool_result_budget_from_env().is_none());
    }

    #[test]
    fn multiple_tool_results_same_message() {
        let msg = ConversationMessage {
            role: crate::session::MessageRole::Tool,
            blocks: vec![
                ContentBlock::ToolResult {
                    tool_use_id: "id1".to_string(),
                    tool_name: "tool_a".to_string(),
                    output: "x".repeat(30_000),
                    is_error: false,
                },
                ContentBlock::ToolResult {
                    tool_use_id: "id2".to_string(),
                    tool_name: "tool_b".to_string(),
                    output: "y".repeat(30_000),
                    is_error: false,
                },
            ],
            usage: None,
        };
        let mut messages = vec![msg];
        let budget = 50_000;
        let total_before = total_tool_result_chars(&messages);
        assert_eq!(total_before, 60_000);

        let truncated = apply_tool_result_budget(&mut messages, budget);
        assert_eq!(truncated, 1);

        // First ToolResult in the message should be truncated (oldest)
        if let ContentBlock::ToolResult { output, .. } = &messages[0].blocks[0] {
            assert_eq!(output, TRUNCATION_PLACEHOLDER);
        } else {
            panic!("first block should be ToolResult");
        }
        // Second should remain intact
        if let ContentBlock::ToolResult { output, .. } = &messages[0].blocks[1] {
            assert_eq!(output.chars().count(), 30_000);
        }
    }
}
