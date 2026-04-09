use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::hash::{Hash, Hasher};
use std::time::Instant;

/// A single entry in the doom loop detection history.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoomLoopEntry {
    pub tool_name: String,
    pub args_hash: u64,
    pub timestamp: Instant,
}

/// Detection result returned when a doom loop is identified.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoomLoopDetection {
    pub tool_name: String,
    pub call_count: u32,
    pub args_hash: u64,
}

/// Detects when the same tool+arguments are called repeatedly in succession,
/// indicating a potential infinite loop in the agentic conversation.
///
/// Maintains a bounded history of the last 10 tool calls and checks whether
/// the most recent calls are all identical (same tool name and argument hash).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DoomLoopDetector {
    history: VecDeque<DoomLoopEntry>,
    threshold: u32,
}

#[allow(dead_code)]
impl DoomLoopDetector {
    const MAX_HISTORY: usize = 10;
    const DEFAULT_THRESHOLD: u32 = 3;

    /// Creates a new detector with the default threshold of 3.
    #[must_use]
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(Self::MAX_HISTORY),
            threshold: Self::DEFAULT_THRESHOLD,
        }
    }

    /// Creates a new detector with a custom threshold.
    #[must_use]
    pub fn with_threshold(threshold: u32) -> Self {
        Self {
            history: VecDeque::with_capacity(Self::MAX_HISTORY),
            threshold,
        }
    }

    /// Records a tool call in the detection history.
    ///
    /// Hashes the arguments using `DefaultHasher` and appends an entry
    /// to the bounded history (max 10 entries, oldest removed first).
    pub fn record_tool_call(&mut self, tool_name: &str, args: &str) {
        let args_hash = Self::hash_args(args);
        self.history.push_back(DoomLoopEntry {
            tool_name: tool_name.to_string(),
            args_hash,
            timestamp: Instant::now(),
        });

        // Enforce bounded history size
        if self.history.len() > Self::MAX_HISTORY {
            self.history.pop_front();
        }
    }

    /// Checks whether the most recent tool calls form a doom loop.
    ///
    /// Returns `Some(DoomLoopDetection)` if the same tool+arguments have been
    /// called `threshold` or more times consecutively (most recent first).
    #[must_use]
    pub fn check(&self) -> Option<DoomLoopDetection> {
        if self.history.is_empty() {
            return None;
        }

        let last = self.history.last()?;
        let mut count: u32 = 1;

        // Count consecutive identical calls from the end (exclusive of last)
        for entry in self.history.iter().rev().skip(1) {
            if entry.tool_name == last.tool_name && entry.args_hash == last.args_hash {
                count += 1;
            } else {
                break;
            }
        }

        if count >= self.threshold {
            Some(DoomLoopDetection {
                tool_name: last.tool_name.clone(),
                call_count: count,
                args_hash: last.args_hash,
            })
        } else {
            None
        }
    }

    fn hash_args(args: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        args.hash(&mut hasher);
        hasher.finish()
    }

    /// Formats a user-facing error message for a detected doom loop.
    #[must_use]
    pub fn format_error_message(detection: &DoomLoopDetection) -> String {
        format!(
            "Doom loop detected: {} was called {} times with identical arguments. \
             This suggests an infinite loop. Please try a different approach.",
            detection.tool_name, detection.call_count
        )
    }
}

impl Default for DoomLoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{DoomLoopDetection, DoomLoopDetector};

    #[test]
    fn three_identical_calls_trigger_detection() {
        let mut detector = DoomLoopDetector::new();
        detector.record_tool_call("bash", "ls -la");
        detector.record_tool_call("bash", "ls -la");
        detector.record_tool_call("bash", "ls -la");

        let detection = detector.check().expect("should detect doom loop");
        assert_eq!(detection.tool_name, "bash");
        assert_eq!(detection.call_count, 3);
    }

    fn different_calls_no_detection() {
        let mut detector = DoomLoopDetector::new();
        detector.record_tool_call("bash", "ls -la");
        detector.record_tool_call("bash", "ls -la");
        detector.record_tool_call("read_file", "src/main.rs");

        assert!(
            detector.check().is_none(),
            "different tool call should break the loop"
        );
    }

    #[test]
    fn two_identical_plus_one_different_no_detection() {
        different_calls_no_detection();
    }

    #[test]
    fn threshold_of_four_requires_four_calls() {
        let mut detector = DoomLoopDetector::with_threshold(4);

        detector.record_tool_call("grep", "fn main");
        detector.record_tool_call("grep", "fn main");
        detector.record_tool_call("grep", "fn main");

        assert!(
            detector.check().is_none(),
            "3 calls should not trigger threshold of 4"
        );

        detector.record_tool_call("grep", "fn main");

        let detection = detector.check().expect("should detect at threshold 4");
        assert_eq!(detection.call_count, 4);
    }

    #[test]
    fn history_is_bounded() {
        let mut detector = DoomLoopDetector::new();

        // Insert 15 entries — only 10 should remain
        for i in 0..15 {
            detector.record_tool_call("tool", &format!("args-{i}"));
        }

        assert_eq!(
            detector.history.len(),
            10,
            "history should be bounded to 10 entries"
        );
    }

    #[test]
    fn empty_history_returns_no_detection() {
        let detector = DoomLoopDetector::new();
        assert!(detector.check().is_none());
    }

    #[test]
    fn single_call_below_threshold() {
        let mut detector = DoomLoopDetector::new();
        detector.record_tool_call("bash", "echo hello");
        assert!(detector.check().is_none());
    }

    #[test]
    fn error_message_is_formatted_correctly() {
        let detection = DoomLoopDetection {
            tool_name: "edit_file".to_string(),
            call_count: 3,
            args_hash: 12_345,
        };

        let message = DoomLoopDetector::format_error_message(&detection);
        assert!(message.contains("edit_file"));
        assert!(message.contains("3 times"));
        assert!(message.contains("infinite loop"));
        assert!(message.contains("different approach"));
    }
}
