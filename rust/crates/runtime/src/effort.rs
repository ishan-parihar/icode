use serde::{Deserialize, Serialize};
use std::fmt;

/// Effort levels controlling token budget, temperature, and visual display.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EffortLevel {
    Low,
    #[default]
    Medium,
    High,
    Max,
}

impl fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.glyph())
    }
}

#[allow(dead_code)]
impl EffortLevel {
    /// Thinking budget tokens for this effort level.
    ///
    /// Returns `None` for `Low` (no extended thinking) and `Some(n)` for others.
    pub fn thinking_budget_tokens(self) -> Option<u32> {
        match self {
            EffortLevel::Low => None,
            EffortLevel::Medium => Some(5_000),
            EffortLevel::High => Some(10_000),
            EffortLevel::Max => Some(20_000),
        }
    }

    /// Temperature override for this effort level.
    ///
    /// Returns `Some(0.0)` for `Low` (deterministic) and `None` for others (model default).
    pub fn temperature(self) -> Option<f64> {
        match self {
            EffortLevel::Low => Some(0.0),
            _ => None,
        }
    }

    /// Display glyph for this effort level.
    pub fn glyph(self) -> &'static str {
        match self {
            EffortLevel::Low => "\u{25CB}",    // ○
            EffortLevel::Medium => "\u{25D0}", // ◐
            EffortLevel::High => "\u{25CF}",   // ●
            EffortLevel::Max => "\u{25C9}",    // ◉
        }
    }

    /// Cycle to the next effort level: Low → Medium → High → Max → Low.
    pub fn next(self) -> Self {
        match self {
            EffortLevel::Low => EffortLevel::Medium,
            EffortLevel::Medium => EffortLevel::High,
            EffortLevel::High => EffortLevel::Max,
            EffortLevel::Max => EffortLevel::Low,
        }
    }

    /// Cycle to the previous effort level: Low → Max → High → Medium → Low.
    pub fn prev(self) -> Self {
        match self {
            EffortLevel::Low => EffortLevel::Max,
            EffortLevel::Medium => EffortLevel::Low,
            EffortLevel::High => EffortLevel::Medium,
            EffortLevel::Max => EffortLevel::High,
        }
    }
}

#[allow(dead_code)]
fn parse_effort(s: &str) -> Option<EffortLevel> {
    match s {
        "low" => Some(EffortLevel::Low),
        "medium" => Some(EffortLevel::Medium),
        "high" => Some(EffortLevel::High),
        "max" => Some(EffortLevel::Max),
        _ => None,
    }
}

/// Read effort level from the `ICODE_EFFORT` environment variable.
///
/// Accepts case-insensitive values: `"low"`, `"medium"`, `"high"`, `"max"`.
/// Defaults to `Medium` when the variable is unset or contains an unrecognized value.
#[allow(dead_code)]
pub fn effort_config_from_env() -> EffortLevel {
    std::env::var("ICODE_EFFORT")
        .ok()
        .as_deref()
        .map(str::to_lowercase)
        .as_deref()
        .and_then(parse_effort)
        .unwrap_or(EffortLevel::Medium)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_medium() {
        assert_eq!(EffortLevel::default(), EffortLevel::Medium);
    }

    #[test]
    fn display_renders_glyph() {
        assert_eq!(format!("{}", EffortLevel::Low), "\u{25CB}");
        assert_eq!(format!("{}", EffortLevel::Medium), "\u{25D0}");
        assert_eq!(format!("{}", EffortLevel::High), "\u{25CF}");
        assert_eq!(format!("{}", EffortLevel::Max), "\u{25C9}");
    }

    #[test]
    fn glyph_returns_correct_unicode() {
        assert_eq!(EffortLevel::Low.glyph(), "\u{25CB}");
        assert_eq!(EffortLevel::Medium.glyph(), "\u{25D0}");
        assert_eq!(EffortLevel::High.glyph(), "\u{25CF}");
        assert_eq!(EffortLevel::Max.glyph(), "\u{25C9}");
    }

    #[test]
    fn thinking_budget_tokens_low_is_none() {
        assert_eq!(EffortLevel::Low.thinking_budget_tokens(), None);
    }

    #[test]
    fn thinking_budget_tokens_medium() {
        assert_eq!(EffortLevel::Medium.thinking_budget_tokens(), Some(5_000));
    }

    #[test]
    fn thinking_budget_tokens_high() {
        assert_eq!(EffortLevel::High.thinking_budget_tokens(), Some(10_000));
    }

    #[test]
    fn thinking_budget_tokens_max() {
        assert_eq!(EffortLevel::Max.thinking_budget_tokens(), Some(20_000));
    }

    #[test]
    fn temperature_low_is_zero() {
        assert_eq!(EffortLevel::Low.temperature(), Some(0.0));
    }

    #[test]
    fn temperature_others_are_none() {
        assert_eq!(EffortLevel::Medium.temperature(), None);
        assert_eq!(EffortLevel::High.temperature(), None);
        assert_eq!(EffortLevel::Max.temperature(), None);
    }

    #[test]
    fn next_cycles_forward() {
        assert_eq!(EffortLevel::Low.next(), EffortLevel::Medium);
        assert_eq!(EffortLevel::Medium.next(), EffortLevel::High);
        assert_eq!(EffortLevel::High.next(), EffortLevel::Max);
        assert_eq!(EffortLevel::Max.next(), EffortLevel::Low);
    }

    #[test]
    fn prev_cycles_backward() {
        assert_eq!(EffortLevel::Low.prev(), EffortLevel::Max);
        assert_eq!(EffortLevel::Medium.prev(), EffortLevel::Low);
        assert_eq!(EffortLevel::High.prev(), EffortLevel::Medium);
        assert_eq!(EffortLevel::Max.prev(), EffortLevel::High);
    }

    #[test]
    fn next_then_prev_is_identity() {
        for level in [
            EffortLevel::Low,
            EffortLevel::Medium,
            EffortLevel::High,
            EffortLevel::Max,
        ] {
            assert_eq!(level.next().prev(), level);
        }
    }

    #[test]
    fn parse_effort_lowercase() {
        assert_eq!(parse_effort("low"), Some(EffortLevel::Low));
        assert_eq!(parse_effort("medium"), Some(EffortLevel::Medium));
        assert_eq!(parse_effort("high"), Some(EffortLevel::High));
        assert_eq!(parse_effort("max"), Some(EffortLevel::Max));
    }

    #[test]
    fn parse_effort_case_insensitive() {
        assert_eq!(parse_effort("LOW"), None);
        assert_eq!(parse_effort("Medium"), None);
        assert_eq!(parse_effort("HIGH"), None);
        assert_eq!(parse_effort("MaX"), None);
    }

    #[test]
    fn parse_effort_unknown_returns_none() {
        assert_eq!(parse_effort("extreme"), None);
        assert_eq!(parse_effort(""), None);
    }

    #[test]
    fn serialize_and_deserialize_roundtrip() {
        for level in [
            EffortLevel::Low,
            EffortLevel::Medium,
            EffortLevel::High,
            EffortLevel::Max,
        ] {
            if let Ok(json) = serde_json::to_string(&level) {
                if let Ok(deserialized) = serde_json::from_str::<EffortLevel>(&json) {
                    assert_eq!(deserialized, level);
                } else {
                    panic!("deserialize failed for {level:?}");
                }
            } else {
                panic!("serialize failed for {level:?}");
            }
        }
    }
}
