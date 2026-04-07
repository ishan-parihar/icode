#[allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

// Types in this module are public API; dead_code warnings suppressed as
// they are consumed by external crates via re-exports.

/// Policy governing how tool output truncation is applied.
///
/// Uses a head+tail strategy: when output exceeds limits, the beginning
/// (head) and end (tail) are preserved, with the middle replaced by a
/// truncation marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncationPolicy {
    /// Maximum number of characters allowed before truncation.
    pub max_chars: usize,
    /// Maximum number of lines allowed before truncation. `None` disables line limits.
    pub max_lines: Option<usize>,
    /// Fraction of `max_chars` allocated to the head (beginning) of output.
    /// Must satisfy `head_ratio + tail_ratio <= 1.0`.
    pub head_ratio: f64,
    /// Fraction of `max_chars` allocated to the tail (end) of output.
    /// Must satisfy `head_ratio + tail_ratio <= 1.0`.
    pub tail_ratio: f64,
}

/// Result of applying a [`TruncationPolicy`] to tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncatedOutput {
    /// The (possibly truncated) output text.
    pub text: String,
    /// Metadata about the truncation operation.
    pub metadata: TruncationMetadata,
}

/// Metadata produced by a truncation operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncationMetadata {
    /// Total characters in the original input.
    pub total_chars: usize,
    /// Total lines in the original input.
    pub total_lines: usize,
    /// Whether truncation was applied.
    pub truncated: bool,
    /// Number of characters removed from the original.
    pub chars_removed: usize,
    /// Number of lines removed from the original.
    pub lines_removed: usize,
}

impl Default for TruncationPolicy {
    fn default() -> Self {
        Self {
            max_chars: 30_000,
            max_lines: Some(500),
            head_ratio: 0.7,
            tail_ratio: 0.3,
        }
    }
}

impl TruncationPolicy {
    /// Creates a new policy, returning `None` if the ratios are invalid.
    ///
    /// The sum of `head_ratio` and `tail_ratio` must be at most `1.0`.
    #[must_use]
    pub fn new(
        max_chars: usize,
        max_lines: Option<usize>,
        head_ratio: f64,
        tail_ratio: f64,
    ) -> Option<Self> {
        if head_ratio + tail_ratio > 1.0 {
            return None;
        }
        Some(Self {
            max_chars,
            max_lines,
            head_ratio,
            tail_ratio,
        })
    }

    /// Reads `ICODE_TRUNCATION_MAX_CHARS` from the environment and returns a policy
    /// with that `max_chars` value, falling back to defaults for all other fields.
    ///
    /// Returns `None` if the environment variable is not set or cannot be parsed.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let max_chars = std::env::var("ICODE_TRUNCATION_MAX_CHARS")
            .ok()?
            .parse::<usize>()
            .ok()?;
        if max_chars == 0 {
            return None;
        }
        let defaults = Self::default();
        Some(Self {
            max_chars,
            ..defaults
        })
    }

    /// Applies the truncation policy to the given output string.
    ///
    /// If the output is within both character and line limits, returns it unchanged.
    /// Otherwise, applies head+tail character truncation, then optionally line truncation.
    #[must_use]
    pub fn apply(&self, output: &str) -> TruncatedOutput {
        let total_chars = output.chars().count();
        let total_lines = output.lines().count();

        let over_chars = total_chars > self.max_chars;
        let over_lines = self.max_lines.is_some_and(|limit| total_lines > limit);

        if !over_chars && !over_lines {
            return TruncatedOutput {
                text: output.to_string(),
                metadata: TruncationMetadata {
                    total_chars,
                    total_lines,
                    truncated: false,
                    chars_removed: 0,
                    lines_removed: 0,
                },
            };
        }

        let (char_truncated_text, chars_after_phase1) = if over_chars {
            self.truncate_chars(output)
        } else {
            (output.to_string(), total_chars)
        };

        let (final_text, chars_after_phase2, lines_after_phase2) =
            if let Some(line_limit) = self.max_lines {
                let lines_in_char_truncated = char_truncated_text.lines().count();
                if lines_in_char_truncated > line_limit {
                    self.truncate_lines(&char_truncated_text, line_limit)
                } else {
                    (
                        char_truncated_text,
                        chars_after_phase1,
                        lines_in_char_truncated,
                    )
                }
            } else {
                let line_count = char_truncated_text.lines().count();
                (char_truncated_text, chars_after_phase1, line_count)
            };

        let chars_removed = total_chars.saturating_sub(chars_after_phase2);
        let lines_removed = total_lines.saturating_sub(lines_after_phase2);

        TruncatedOutput {
            text: final_text,
            metadata: TruncationMetadata {
                total_chars,
                total_lines,
                truncated: true,
                chars_removed,
                lines_removed,
            },
        }
    }

    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    fn truncate_chars(&self, output: &str) -> (String, usize) {
        let head_chars = (self.max_chars as f64 * self.head_ratio).floor() as usize;
        let tail_chars = (self.max_chars as f64 * self.tail_ratio).floor() as usize;

        let total_chars = output.chars().count();
        let chars: Vec<char> = output.chars().collect();

        let head_end = head_chars.min(total_chars);
        let tail_start = total_chars.saturating_sub(tail_chars);

        if head_end >= tail_start {
            let truncated: String = chars.iter().take(self.max_chars).collect();
            let kept = truncated.chars().count();
            return (truncated, kept.min(total_chars));
        }

        let head: String = chars[..head_end].iter().collect();
        let tail: String = chars[tail_start..].iter().collect();

        let chars_kept = head_end + tail_chars.min(total_chars.saturating_sub(head_end));
        let middle_chars = total_chars.saturating_sub(chars_kept);

        let lines_in_output = output.lines().count();
        let lines_in_head = head.lines().count();
        let lines_in_tail = tail.lines().count();
        let middle_lines = lines_in_output
            .saturating_sub(lines_in_head)
            .saturating_sub(lines_in_tail);

        let placeholder =
            format!("\n\n[...{middle_chars} characters and {middle_lines} lines truncated...]\n\n");

        let result = format!("{head}{placeholder}{tail}");
        (result, chars_kept)
    }

    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    fn truncate_lines(&self, output: &str, line_limit: usize) -> (String, usize, usize) {
        let lines: Vec<&str> = output.lines().collect();
        let total_lines = lines.len();

        if total_lines <= line_limit {
            return (output.to_string(), output.chars().count(), total_lines);
        }

        let head_lines = (line_limit as f64 * self.head_ratio).floor() as usize;
        let tail_lines = line_limit.saturating_sub(head_lines);

        let head_end = head_lines.min(total_lines);
        let tail_start = total_lines.saturating_sub(tail_lines);

        let middle_lines_count = if head_end >= tail_start {
            total_lines.saturating_sub(line_limit)
        } else {
            total_lines
                .saturating_sub(head_end)
                .saturating_sub(tail_lines)
        };

        if head_end >= tail_start {
            let mut truncated = String::with_capacity(output.len());
            let mut chars_kept = 0;
            for line in lines.iter().take(line_limit) {
                if !truncated.is_empty() {
                    truncated.push('\n');
                    chars_kept += 1;
                }
                chars_kept += line.chars().count();
                truncated.push_str(line);
            }
            return (truncated, chars_kept, line_limit);
        }

        let mut chars_kept = 0;
        let mut result = String::with_capacity(output.len());
        for line in &lines[..head_end] {
            if !result.is_empty() {
                result.push('\n');
                chars_kept += 1;
            }
            chars_kept += line.chars().count();
            result.push_str(line);
        }

        let _ = write!(result, "\n[...{middle_lines_count} lines truncated...]\n");

        for (i, line) in lines[tail_start..].iter().enumerate() {
            chars_kept += line.chars().count();
            if i > 0 {
                result.push('\n');
                chars_kept += 1;
            }
            result.push_str(line);
        }

        let lines_kept =
            head_lines.min(total_lines) + tail_lines.min(total_lines.saturating_sub(head_lines));
        (result, chars_kept, lines_kept.min(total_lines))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repeat_char(c: char, n: usize) -> String {
        std::iter::repeat_n(c, n).collect()
    }

    fn make_lines(line_count: usize, chars_per_line: usize) -> String {
        (0..line_count)
            .map(|i| {
                format!(
                    "{}{c}",
                    repeat_char('x', chars_per_line - 1),
                    c = (b'a' + u8::try_from(i % 26).expect("i%26 fits in u8")) as char
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn under_budget_returns_unchanged() {
        let policy = TruncationPolicy::default();
        let small_output = "hello world";

        let result = policy.apply(small_output);

        assert!(!result.metadata.truncated);
        assert_eq!(result.text, small_output);
        assert_eq!(result.metadata.chars_removed, 0);
        assert_eq!(result.metadata.lines_removed, 0);
    }

    #[test]
    fn over_char_limit_truncates_head_tail() {
        let policy = TruncationPolicy {
            max_chars: 100,
            max_lines: None,
            head_ratio: 0.7,
            tail_ratio: 0.3,
        };
        let large_output = format!("{}{}", repeat_char('A', 60), repeat_char('B', 60));

        let result = policy.apply(&large_output);

        assert!(result.metadata.truncated);
        assert_eq!(result.metadata.total_chars, 120);
        assert!(result.metadata.chars_removed > 0);
        assert!(result.text.starts_with("AAAAA"));
        assert!(result.text.ends_with("BBBBB"));
        assert!(result.text.contains("characters"));
        assert!(result.text.contains("truncated"));
    }

    #[test]
    fn over_line_limit_truncates_lines() {
        let policy = TruncationPolicy {
            max_chars: 100_000,
            max_lines: Some(10),
            head_ratio: 0.7,
            tail_ratio: 0.3,
        };
        let many_lines = make_lines(20, 5);

        let result = policy.apply(&many_lines);

        assert!(result.metadata.truncated);
        assert!(result.metadata.lines_removed > 0);
        assert_eq!(result.text.lines().count(), 11);
    }

    #[test]
    fn both_char_and_line_limits_exceeded() {
        let policy = TruncationPolicy {
            max_chars: 200,
            max_lines: Some(10),
            head_ratio: 0.7,
            tail_ratio: 0.3,
        };
        let many_lines = make_lines(20, 20);

        let result = policy.apply(&many_lines);

        assert!(result.metadata.truncated);
        assert!(result.metadata.chars_removed > 0);
        assert!(result.metadata.lines_removed > 0);
    }

    #[test]
    fn invalid_ratios_rejected() {
        let result = TruncationPolicy::new(1000, None, 0.6, 0.5);
        assert!(result.is_none());

        let result = TruncationPolicy::new(1000, None, 0.5, 0.5);
        assert!(result.is_some());
    }

    #[test]
    fn empty_input_returns_empty() {
        let policy = TruncationPolicy::default();
        let result = policy.apply("");

        assert!(!result.metadata.truncated);
        assert_eq!(result.text, "");
        assert_eq!(result.metadata.total_chars, 0);
        assert_eq!(result.metadata.total_lines, 0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn from_env_parsing() {
        std::env::set_var("ICODE_TRUNCATION_MAX_CHARS", "50000");
        let policy = TruncationPolicy::from_env();
        std::env::remove_var("ICODE_TRUNCATION_MAX_CHARS");

        assert!(policy.is_some());
        let policy = policy.unwrap();
        assert_eq!(policy.max_chars, 50_000);
        assert_eq!(policy.head_ratio, 0.7);
        assert_eq!(policy.tail_ratio, 0.3);
    }

    #[test]
    fn from_env_returns_none_for_invalid() {
        std::env::set_var("ICODE_TRUNCATION_MAX_CHARS", "not_a_number");
        let policy = TruncationPolicy::from_env();
        std::env::remove_var("ICODE_TRUNCATION_MAX_CHARS");
        assert!(policy.is_none());
    }

    #[test]
    fn from_env_returns_none_for_unset() {
        std::env::remove_var("ICODE_TRUNCATION_MAX_CHARS");
        let policy = TruncationPolicy::from_env();
        assert!(policy.is_none());
    }

    #[test]
    fn metadata_is_accurate() {
        let policy = TruncationPolicy {
            max_chars: 100,
            max_lines: None,
            head_ratio: 0.5,
            tail_ratio: 0.5,
        };
        let output = repeat_char('X', 200);
        let total_chars = output.chars().count();

        let result = policy.apply(&output);

        assert_eq!(result.metadata.total_chars, total_chars);
        assert_eq!(result.metadata.total_lines, 1);
        assert!(result.metadata.truncated);
        assert_eq!(result.metadata.chars_removed, 100);
        assert_eq!(result.metadata.lines_removed, 0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn default_values() {
        let policy = TruncationPolicy::default();
        assert_eq!(policy.max_chars, 30_000);
        assert_eq!(policy.max_lines, Some(500));
        assert_eq!(policy.head_ratio, 0.7);
        assert_eq!(policy.tail_ratio, 0.3);
    }

    #[test]
    fn single_char_over_limit() {
        let policy = TruncationPolicy {
            max_chars: 1,
            max_lines: None,
            head_ratio: 0.5,
            tail_ratio: 0.5,
        };
        let output = "AB";

        let result = policy.apply(output);

        assert!(result.metadata.truncated);
        assert!(result.text.contains("truncated"));
    }

    #[test]
    fn unicode_chars_counted_correctly() {
        let policy = TruncationPolicy {
            max_chars: 10,
            max_lines: None,
            head_ratio: 0.5,
            tail_ratio: 0.5,
        };
        let output = "🔥🔥🔥🔥🔥🔥🔥🔥🔥🔥🔥🔥";

        let result = policy.apply(output);

        assert_eq!(result.metadata.total_chars, 12);
        assert!(result.metadata.truncated);
    }
}
