/// Automatic fallback to a cheaper model when the primary model hits rate limits
/// or is overloaded, after retries are exhausted.

/// Configuration for model fallback behavior.
#[derive(Debug, Clone, Default)]
pub struct FallbackConfig {
    /// Model to fall back to when primary is unavailable. None = no fallback.
    pub fallback_model: Option<String>,
    /// Whether to auto-select a small/cheap model as fallback when no explicit fallback is set.
    pub auto_fallback: bool,
}

/// Tracks whether fallback has been used and the current active model.
#[derive(Debug, Clone)]
pub struct FallbackState {
    config: FallbackConfig,
    used_fallback: bool,
    current_model: String,
}

impl Default for FallbackState {
    fn default() -> Self {
        Self {
            config: FallbackConfig::default(),
            used_fallback: false,
            current_model: String::new(),
        }
    }
}

impl FallbackState {
    /// Create a new fallback state with the given configuration and primary model.
    pub fn new(config: FallbackConfig, primary_model: String) -> Self {
        Self {
            config,
            used_fallback: false,
            current_model: primary_model,
        }
    }

    /// Returns true if the error indicates a rate limit / overload condition,
    /// a fallback model is available, and fallback hasn't already been used.
    pub fn should_fallback(&self, error: &str) -> bool {
        if self.used_fallback {
            return false;
        }
        if !is_rate_limit_error(error) {
            return false;
        }
        // Explicit fallback takes priority
        if self.config.fallback_model.is_some() {
            return true;
        }
        // Auto-fallback heuristic
        if self.config.auto_fallback {
            return select_auto_fallback_model(&self.current_model).is_some();
        }
        false
    }

    /// Activate fallback: switches current_model to the configured fallback
    /// and marks that fallback has been used. No-op if already used.
    pub fn activate_fallback(&mut self) {
        if self.used_fallback {
            return;
        }
        if let Some(ref model) = self.config.fallback_model {
            self.current_model = model.clone();
        } else if self.config.auto_fallback {
            if let Some(auto) = select_auto_fallback_model(&self.current_model) {
                self.current_model = auto;
            }
        }
        self.used_fallback = true;
    }

    /// Whether fallback has already been activated.
    pub fn is_used_fallback(&self) -> bool {
        self.used_fallback
    }

    /// Returns the currently active model name.
    pub fn current_model(&self) -> &str {
        &self.current_model
    }

    /// Returns the currently active model name (alias for current_model).
    pub fn model_name(&self) -> &str {
        &self.current_model
    }
}

/// Given a primary model name, returns a cheaper alternative model.
///
/// Heuristics:
/// - opus -> sonnet
/// - sonnet (any) -> haiku
/// - haiku -> None (already cheapest)
/// - unknown -> None
pub fn select_auto_fallback_model(primary_model: &str) -> Option<String> {
    let lower = primary_model.to_lowercase();
    if lower.contains("opus") {
        Some("claude-sonnet-4-6".to_string())
    } else if lower.contains("sonnet") {
        Some("claude-haiku-4-5".to_string())
    } else if lower.contains("haiku") {
        None
    } else {
        None
    }
}

/// Read fallback configuration from environment variables.
///
/// - `ICODE_FALLBACK_MODEL`: explicit fallback model name
/// - `ICODE_AUTO_FALLBACK`: "1" or "true" enables auto-fallback heuristic
pub fn fallback_config_from_env() -> FallbackConfig {
    let fallback_model = std::env::var("ICODE_FALLBACK_MODEL").ok();
    let auto_fallback = std::env::var("ICODE_AUTO_FALLBACK")
        .map(|v| {
            let v = v.to_lowercase();
            v == "1" || v == "true"
        })
        .unwrap_or(false);
    FallbackConfig {
        fallback_model,
        auto_fallback,
    }
}

/// Returns true if the error string indicates a rate-limit or overload condition.
pub fn is_rate_limit_error(error_str: &str) -> bool {
    let lower = error_str.to_lowercase();
    lower.contains("overloaded")
        || lower.contains("rate_limit")
        || lower.contains("429")
        || lower.contains("529")
        || lower.contains("rate limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rate_limit_error() {
        assert!(is_rate_limit_error("overloaded"));
        assert!(is_rate_limit_error("rate_limit_exceeded"));
        assert!(is_rate_limit_error("429 Too Many Requests"));
        assert!(is_rate_limit_error("529 service unavailable"));
        assert!(is_rate_limit_error("Rate Limit Hit"));
    }

    #[test]
    fn does_not_detect_normal_error() {
        assert!(!is_rate_limit_error("connection refused"));
    }

    #[test]
    fn fallback_config_from_env_explicit() {
        let orig_model = std::env::var("ICODE_FALLBACK_MODEL").ok();
        let orig_auto = std::env::var("ICODE_AUTO_FALLBACK").ok();

        std::env::set_var("ICODE_FALLBACK_MODEL", "claude-haiku-4-5");
        std::env::set_var("ICODE_AUTO_FALLBACK", "0");
        let cfg = fallback_config_from_env();
        assert_eq!(cfg.fallback_model.as_deref(), Some("claude-haiku-4-5"));
        assert!(!cfg.auto_fallback);

        std::env::set_var("ICODE_AUTO_FALLBACK", "1");
        let cfg = fallback_config_from_env();
        assert!(cfg.auto_fallback);

        std::env::remove_var("ICODE_FALLBACK_MODEL");
        std::env::set_var("ICODE_AUTO_FALLBACK", "true");
        let cfg = fallback_config_from_env();
        assert!(cfg.auto_fallback);
        assert!(cfg.fallback_model.is_none());

        match orig_model {
            Some(v) => std::env::set_var("ICODE_FALLBACK_MODEL", v),
            None => std::env::remove_var("ICODE_FALLBACK_MODEL"),
        }
        match orig_auto {
            Some(v) => std::env::set_var("ICODE_AUTO_FALLBACK", v),
            None => std::env::remove_var("ICODE_AUTO_FALLBACK"),
        }
    }

    #[test]
    fn fallback_config_from_env_auto() {
        let cfg = FallbackConfig {
            fallback_model: None,
            auto_fallback: true,
        };
        assert!(cfg.auto_fallback);
        assert!(cfg.fallback_model.is_none());
    }

    #[test]
    fn auto_fallback_opus_to_sonnet() {
        assert_eq!(
            select_auto_fallback_model("claude-opus-4-6"),
            Some("claude-sonnet-4-6".to_string())
        );
    }

    #[test]
    fn auto_fallback_sonnet_to_haiku() {
        assert_eq!(
            select_auto_fallback_model("claude-sonnet-4-6"),
            Some("claude-haiku-4-5".to_string())
        );
    }

    #[test]
    fn auto_fallback_haiku_no_fallback() {
        assert_eq!(select_auto_fallback_model("claude-haiku-4-5"), None);
    }

    #[test]
    fn auto_fallback_unknown_model() {
        assert_eq!(select_auto_fallback_model("foo-bar"), None);
    }

    #[test]
    fn fallback_state_activates_once() {
        let cfg = FallbackConfig {
            fallback_model: Some("claude-haiku-4-5".to_string()),
            auto_fallback: false,
        };
        let mut state = FallbackState::new(cfg, "claude-opus-4-6".to_string());
        state.activate_fallback();
        assert_eq!(state.model_name(), "claude-haiku-4-5");
        assert!(state.is_used_fallback());

        // Second activation is a no-op
        state.activate_fallback();
        assert_eq!(state.model_name(), "claude-haiku-4-5");
    }

    #[test]
    fn fallback_state_should_fallback_checks_used_flag() {
        let cfg = FallbackConfig {
            fallback_model: Some("claude-haiku-4-5".to_string()),
            auto_fallback: false,
        };
        let mut state = FallbackState::new(cfg, "claude-opus-4-6".to_string());

        // Before activation, should_fallback is true for rate limit errors
        assert!(state.should_fallback("rate_limit exceeded"));

        // After activation, should_fallback is false even for rate limit errors
        state.activate_fallback();
        assert!(!state.should_fallback("rate_limit exceeded"));
    }
}
