/// Data-driven optimization suggestions for context window usage.
use std::fmt;

/// Severity level for a context optimization suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionSeverity {
    /// Informational, no immediate action needed.
    Info,
    /// Warning, action recommended soon.
    Warning,
    /// Critical, action should be taken immediately.
    Critical,
}

impl fmt::Display for SuggestionSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SuggestionSeverity::Info => write!(f, "INFO"),
            SuggestionSeverity::Warning => write!(f, "WARN"),
            SuggestionSeverity::Critical => write!(f, "CRIT"),
        }
    }
}

/// A single optimization suggestion with an optional action.
#[derive(Debug, Clone)]
pub struct ContextSuggestion {
    pub message: String,
    pub severity: SuggestionSeverity,
    pub action: Option<String>,
}

/// Aggregated data from the session for suggestion generation.
#[derive(Debug, Clone)]
pub struct ContextVizData {
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_create_tokens: u32,
    pub cache_read_tokens: u32,
    pub context_window: u32,
    pub turns: u32,
    pub message_count: usize,
    pub cumulative_cost: f64,
    pub budget_max: Option<f64>,
    pub budget_remaining: Option<f64>,
    pub compaction_count: u32,
    pub compaction_removed: u32,
}

impl ContextVizData {
    fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens + self.cache_create_tokens + self.cache_read_tokens
    }

    fn usage_pct(&self) -> f64 {
        if self.context_window > 0 {
            (self.total_tokens() as f64 / self.context_window as f64 * 100.0).min(100.0)
        } else {
            0.0
        }
    }

    fn cache_read_ratio(&self) -> f64 {
        let total_input = self.input_tokens + self.cache_create_tokens + self.cache_read_tokens;
        if total_input > 0 {
            self.cache_read_tokens as f64 / total_input as f64
        } else {
            1.0
        }
    }
}

/// Generate data-driven optimization suggestions based on session metrics.
#[must_use]
pub fn generate_suggestions(data: &ContextVizData) -> Vec<ContextSuggestion> {
    let mut suggestions = Vec::new();

    // Cache hit rate
    if data.cache_read_ratio() < 0.3 && data.input_tokens > 10_000 {
        suggestions.push(ContextSuggestion {
            message: "Low cache hit rate — most input is not served from cache".into(),
            severity: SuggestionSeverity::Info,
            action: Some("Consider keeping session alive between turns".into()),
        });
    }

    // Context usage thresholds
    let usage_pct = data.usage_pct();
    if usage_pct > 95.0 {
        suggestions.push(ContextSuggestion {
            message: format!("Critical context usage ({usage_pct:.0}%)").into(),
            severity: SuggestionSeverity::Critical,
            action: Some("Run /compact immediately".into()),
        });
    } else if usage_pct > 80.0 {
        suggestions.push(ContextSuggestion {
            message: format!("Near context limit ({usage_pct:.0}%)").into(),
            severity: SuggestionSeverity::Warning,
            action: Some("Run /compact soon".into()),
        });
    }

    // Compaction frequency
    if data.compaction_count > 3 {
        suggestions.push(ContextSuggestion {
            message: format!(
                "Compacted {} times — consider starting a fresh session",
                data.compaction_count
            ),
            severity: SuggestionSeverity::Warning,
            action: Some("Start new session".into()),
        });
    }

    // Cost thresholds
    if data.cumulative_cost > 5.0 {
        suggestions.push(ContextSuggestion {
            message: format!("High session cost (${:.2})", data.cumulative_cost),
            severity: SuggestionSeverity::Critical,
            action: Some("Switch to cheaper model".into()),
        });
    } else if data.cumulative_cost > 1.0 {
        suggestions.push(ContextSuggestion {
            message: format!("Session cost ${:.2}", data.cumulative_cost),
            severity: SuggestionSeverity::Warning,
            action: Some("Consider switching model".into()),
        });
    }

    // Budget running low
    if let (Some(max), Some(remaining)) = (data.budget_max, data.budget_remaining) {
        if max > 0.0 && remaining / max < 0.2 {
            suggestions.push(ContextSuggestion {
                message: format!("Budget running low (${:.2} / ${:.2})", remaining, max),
                severity: SuggestionSeverity::Critical,
                action: Some("Switch model or end session".into()),
            });
        }
    }

    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_data() -> ContextVizData {
        ContextVizData {
            model: "claude-sonnet-4-6".into(),
            input_tokens: 5_000,
            output_tokens: 2_000,
            cache_create_tokens: 3_000,
            cache_read_tokens: 2_000,
            context_window: 200_000,
            turns: 5,
            message_count: 10,
            cumulative_cost: 0.05,
            budget_max: None,
            budget_remaining: None,
            compaction_count: 0,
            compaction_removed: 0,
        }
    }

    #[test]
    fn test_suggestion_near_context_limit() {
        let data = ContextVizData {
            input_tokens: 85_000,
            output_tokens: 5_000,
            context_window: 100_000,
            ..healthy_data()
        };
        let suggestions = generate_suggestions(&data);
        assert!(suggestions
            .iter()
            .any(|s| matches!(s.severity, SuggestionSeverity::Warning)
                && s.message.contains("Near context limit")));
    }

    #[test]
    fn test_suggestion_critical_context_limit() {
        let data = ContextVizData {
            input_tokens: 96_000,
            output_tokens: 2_000,
            context_window: 100_000,
            ..healthy_data()
        };
        let suggestions = generate_suggestions(&data);
        assert!(suggestions
            .iter()
            .any(|s| matches!(s.severity, SuggestionSeverity::Critical)
                && s.message.contains("Critical context usage")));
    }

    #[test]
    fn test_suggestion_high_compaction() {
        let data = ContextVizData {
            compaction_count: 4,
            ..healthy_data()
        };
        let suggestions = generate_suggestions(&data);
        assert!(suggestions
            .iter()
            .any(|s| matches!(s.severity, SuggestionSeverity::Warning)
                && s.message.contains("Compacted")));
    }

    #[test]
    fn test_suggestion_high_cost() {
        let data = ContextVizData {
            cumulative_cost: 1.50,
            ..healthy_data()
        };
        let suggestions = generate_suggestions(&data);
        assert!(suggestions
            .iter()
            .any(|s| matches!(s.severity, SuggestionSeverity::Warning)
                && s.message.contains("Session cost")));
    }

    #[test]
    fn test_suggestion_budget_low() {
        let data = ContextVizData {
            cumulative_cost: 0.10,
            budget_max: Some(1.0),
            budget_remaining: Some(0.15),
            ..healthy_data()
        };
        let suggestions = generate_suggestions(&data);
        assert!(suggestions
            .iter()
            .any(|s| matches!(s.severity, SuggestionSeverity::Critical)
                && s.message.contains("Budget running low")));
    }

    #[test]
    fn test_suggestion_low_cache_ratio() {
        let data = ContextVizData {
            input_tokens: 15_000,
            cache_create_tokens: 1_000,
            cache_read_tokens: 200,
            ..healthy_data()
        };
        let suggestions = generate_suggestions(&data);
        assert!(suggestions
            .iter()
            .any(|s| matches!(s.severity, SuggestionSeverity::Info)
                && s.message.contains("Low cache hit rate")));
    }

    #[test]
    fn test_no_suggestions_for_healthy_session() {
        let data = healthy_data();
        let suggestions = generate_suggestions(&data);
        assert!(
            suggestions.is_empty(),
            "Expected no suggestions for healthy session, got: {suggestions:?}"
        );
    }
}
