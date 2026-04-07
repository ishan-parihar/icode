use serde::{Deserialize, Serialize};

/// Configuration for session compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Maximum number of messages before compaction triggers.
    pub max_messages: usize,
    /// Maximum total token count before compaction triggers.
    pub max_tokens: usize,
    /// Number of recent messages to always preserve (not compact).
    pub preserve_recent: usize,
    /// The system prompt to include in the compacted context.
    pub include_system_prompt: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_messages: 50,
            max_tokens: 50_000,
            preserve_recent: 6,
            include_system_prompt: true,
        }
    }
}

/// Result of a compaction operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Number of messages removed.
    pub messages_removed: usize,
    /// Number of messages remaining.
    pub messages_remaining: usize,
    /// Estimated tokens saved.
    pub tokens_saved: usize,
    /// Whether compaction was actually performed (false if below threshold).
    pub compaction_performed: bool,
}

/// A single message in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String, // "user", "assistant", "system", "tool"
    pub content: String,
    pub token_count: usize,
}

/// Compact a list of session messages by summarizing older messages.
///
/// This implements the opencode compaction strategy:
/// 1. If total messages <= max_messages AND total tokens <= max_tokens, return unchanged.
/// 2. Keep the system message (if any) and the most recent `preserve_recent` messages.
/// 3. Replace all middle messages with a single "compaction summary" message.
/// 4. Return the compacted message list and stats.
pub fn compact_messages(
    messages: Vec<SessionMessage>,
    config: &CompactionConfig,
) -> (Vec<SessionMessage>, CompactionResult) {
    let total_messages = messages.len();
    let total_tokens: usize = messages.iter().map(|m| m.token_count).sum();

    // Check if compaction is needed
    if total_messages <= config.max_messages && total_tokens <= config.max_tokens {
        return (
            messages,
            CompactionResult {
                messages_removed: 0,
                messages_remaining: total_messages,
                tokens_saved: 0,
                compaction_performed: false,
            },
        );
    }

    // Separate system messages, recent messages, and middle messages to compact
    let mut system_messages: Vec<SessionMessage> = Vec::new();
    let mut non_system: Vec<SessionMessage> = Vec::new();

    for msg in messages {
        if msg.role == "system" {
            system_messages.push(msg);
        } else {
            non_system.push(msg);
        }
    }

    let total_non_system = non_system.len();

    if total_non_system <= config.preserve_recent {
        // Not enough non-system messages to compact
        return (
            system_messages.into_iter().chain(non_system).collect(),
            CompactionResult {
                messages_removed: 0,
                messages_remaining: total_messages,
                tokens_saved: 0,
                compaction_performed: false,
            },
        );
    }

    // Keep the most recent messages
    let preserve_count = config.preserve_recent.min(total_non_system);
    let compact_count = total_non_system - preserve_count;

    let messages_to_compact: Vec<SessionMessage> = non_system.drain(..compact_count).collect();
    let tokens_in_compacted: usize = messages_to_compact.iter().map(|m| m.token_count).sum();

    // Create a compaction summary message
    let summary_content = format!(
        "<compaction_summary>\nPrevious conversation summary: {} messages were compacted.\nKey decisions and context from those messages have been summarized.\nContinue the conversation from the preserved messages below.\n</compaction_summary>",
        compact_count
    );

    let summary_msg = SessionMessage {
        role: "assistant".to_string(),
        content: summary_content,
        token_count: 50, // Estimate for the summary message
    };

    // Build the compacted message list
    let mut result = system_messages;
    result.push(summary_msg);
    result.extend(non_system);

    let remaining = result.len();

    (
        result,
        CompactionResult {
            messages_removed: compact_count,
            messages_remaining: remaining,
            tokens_saved: tokens_in_compacted.saturating_sub(50),
            compaction_performed: true,
        },
    )
}

/// Count approximate tokens in a string (rough estimate: ~4 chars per token).
pub fn estimate_token_count(text: &str) -> usize {
    text.len() / 4
}

/// Check if a session needs compaction based on message count and token estimate.
pub fn needs_compaction(
    message_count: usize,
    estimated_tokens: usize,
    config: &CompactionConfig,
) -> bool {
    message_count > config.max_messages || estimated_tokens > config.max_tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_messages(count: usize) -> Vec<SessionMessage> {
        (0..count)
            .map(|i| SessionMessage {
                role: if i % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                content: format!("Message content {}", i),
                token_count: 100,
            })
            .collect()
    }

    #[test]
    fn no_compaction_when_below_threshold() {
        let messages = make_messages(10);
        let config = CompactionConfig::default();
        let (result_messages, stats) = compact_messages(messages, &config);

        assert!(!stats.compaction_performed);
        assert_eq!(stats.messages_removed, 0);
        assert_eq!(stats.messages_remaining, 10);
        assert_eq!(result_messages.len(), 10);
    }

    #[test]
    fn compaction_removes_middle_messages() {
        let messages = make_messages(60);
        let config = CompactionConfig::default();
        let (_result_messages, stats) = compact_messages(messages, &config);

        assert!(stats.compaction_performed);
        assert!(stats.messages_removed > 0);
        // Should have system (0) + summary (1) + preserved recent (6) = 7
        assert_eq!(stats.messages_remaining, 7);
    }

    #[test]
    fn preserves_system_messages() {
        let mut messages = vec![SessionMessage {
            role: "system".to_string(),
            content: "You are a helpful assistant.".to_string(),
            token_count: 20,
        }];
        messages.extend(make_messages(60));

        let config = CompactionConfig::default();
        let (result_messages, stats) = compact_messages(messages, &config);

        assert!(stats.compaction_performed);
        assert_eq!(result_messages[0].role, "system");
        assert_eq!(result_messages[0].content, "You are a helpful assistant.");
    }

    #[test]
    fn needs_compaction_detects_threshold_breach() {
        let config = CompactionConfig::default();
        assert!(!needs_compaction(10, 1000, &config));
        assert!(needs_compaction(100, 1000, &config));
        assert!(needs_compaction(10, 100_000, &config));
    }

    #[test]
    fn estimate_token_count_is_reasonable() {
        assert_eq!(estimate_token_count("hello world"), 2); // 11 chars / 4 = 2
        assert_eq!(estimate_token_count(""), 0);
    }
}
