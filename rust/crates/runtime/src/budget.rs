use crate::usage::{pricing_for_model, ModelPricing};

/// Configuration for session budget enforcement.
#[derive(Debug, Clone, Copy, Default)]
pub struct BudgetConfig {
    /// Maximum USD to spend in a session. None = unlimited.
    pub max_cost_usd: Option<f64>,
}

/// Tracks running session cost against a budget cap.
#[derive(Debug, Clone)]
pub struct BudgetState {
    config: BudgetConfig,
    cumulative_cost_usd: f64,
}

impl BudgetState {
    /// Create a new budget state with the given configuration.
    #[must_use]
    pub fn new(config: BudgetConfig) -> Self {
        Self {
            config,
            cumulative_cost_usd: 0.0,
        }
    }

    /// Record a cost increment in USD.
    pub fn record_cost(&mut self, cost_usd: f64) {
        self.cumulative_cost_usd += cost_usd;
    }

    /// Returns true if a budget cap is set and has been exceeded.
    #[must_use]
    pub fn is_over_budget(&self) -> bool {
        match self.config.max_cost_usd {
            Some(max) => self.cumulative_cost_usd > max,
            None => false,
        }
    }

    /// Returns remaining budget, or 0.0 if over budget or unlimited.
    #[must_use]
    pub fn remaining_budget(&self) -> f64 {
        match self.config.max_cost_usd {
            Some(max) if self.cumulative_cost_usd >= max => 0.0,
            Some(max) => max - self.cumulative_cost_usd,
            None => 0.0,
        }
    }

    /// Returns a reference to the budget configuration.
    #[must_use]
    pub const fn config(&self) -> &BudgetConfig {
        &self.config
    }

    /// Returns the cumulative cost recorded so far.
    #[must_use]
    pub const fn cumulative_cost_usd(&self) -> f64 {
        self.cumulative_cost_usd
    }
}

/// Compute cost in USD from token counts and model name.
///
/// Falls back to sonnet pricing when the model is not recognized.
#[must_use]
pub fn compute_cost_from_tokens(
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_tokens: u32,
    cache_read_tokens: u32,
    model_name: &str,
) -> f64 {
    let pricing = pricing_for_model(model_name).unwrap_or_else(ModelPricing::default_sonnet_tier);
    let input_cost = f64::from(input_tokens) / 1_000_000.0 * pricing.input_cost_per_million;
    let output_cost = f64::from(output_tokens) / 1_000_000.0 * pricing.output_cost_per_million;
    let cache_create_cost =
        f64::from(cache_creation_tokens) / 1_000_000.0 * pricing.cache_creation_cost_per_million;
    let cache_read_cost =
        f64::from(cache_read_tokens) / 1_000_000.0 * pricing.cache_read_cost_per_million;
    input_cost + output_cost + cache_create_cost + cache_read_cost
}

/// Parse `BudgetConfig` from the `ICODE_SESSION_BUDGET_USD` environment variable.
///
/// If the variable is set and parses to a positive f64, returns a capped config.
/// Otherwise returns the default (unlimited).
#[must_use]
pub fn budget_config_from_env() -> BudgetConfig {
    match std::env::var("ICODE_SESSION_BUDGET_USD") {
        Ok(val) => match val.parse::<f64>() {
            Ok(v) if v > 0.0 => BudgetConfig {
                max_cost_usd: Some(v),
            },
            _ => BudgetConfig::default(),
        },
        Err(_) => BudgetConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::{budget_config_from_env, compute_cost_from_tokens, BudgetConfig, BudgetState};
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env_var(key: &str, value: Option<&str>, f: impl FnOnce()) {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var(key).ok();
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        f();
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn budget_state_tracks_cost() {
        let config = BudgetConfig {
            max_cost_usd: Some(1.0),
        };
        let mut state = BudgetState::new(config);
        state.record_cost(0.05);
        state.record_cost(0.10);

        assert!(!state.is_over_budget());
        assert!((state.remaining_budget() - 0.85).abs() < 1e-10);
        assert!((state.cumulative_cost_usd() - 0.15).abs() < 1e-10);
    }

    #[test]
    fn budget_state_detects_over_budget() {
        let config = BudgetConfig {
            max_cost_usd: Some(1.0),
        };
        let mut state = BudgetState::new(config);
        state.record_cost(0.60);
        state.record_cost(0.50);

        assert!(state.is_over_budget());
    }

    #[test]
    fn unlimited_budget_never_over_budget() {
        let config = BudgetConfig { max_cost_usd: None };
        let mut state = BudgetState::new(config);
        state.record_cost(999_999.0);
        assert!(!state.is_over_budget());
    }

    #[test]
    fn remaining_budget_zero_when_over() {
        let config = BudgetConfig {
            max_cost_usd: Some(1.0),
        };
        let mut state = BudgetState::new(config);
        state.record_cost(2.0);

        assert_eq!(state.remaining_budget(), 0.0);
    }

    #[test]
    fn compute_cost_sonnet() {
        let cost = compute_cost_from_tokens(1000, 500, 200, 100, "claude-sonnet-4-6");
        // input: 1000/1M * 15 = 0.015
        // output: 500/1M * 75 = 0.0375
        // cache_create: 200/1M * 18.75 = 0.00375
        // cache_read: 100/1M * 1.50 = 0.00015
        // total = 0.0564
        assert!(
            (cost - 0.0564).abs() < 1e-10,
            "expected ~0.0564, got {cost}"
        );
    }

    #[test]
    fn compute_cost_unknown_model_falls_back_to_sonnet() {
        let cost_unknown = compute_cost_from_tokens(1000, 500, 200, 100, "foo");
        let cost_sonnet = compute_cost_from_tokens(1000, 500, 200, 100, "claude-sonnet-4-6");
        assert!(
            (cost_unknown - cost_sonnet).abs() < 1e-10,
            "unknown model should fall back to sonnet pricing"
        );
    }

    #[test]
    fn env_var_budget_parsing() {
        with_env_var("ICODE_SESSION_BUDGET_USD", Some("5.00"), || {
            let config = budget_config_from_env();
            assert!(matches!(config.max_cost_usd, Some(v) if (v - 5.0).abs() < 1e-10));
        });

        with_env_var("ICODE_SESSION_BUDGET_USD", Some("abc"), || {
            let config = budget_config_from_env();
            assert!(config.max_cost_usd.is_none());
        });
    }

    #[test]
    fn env_var_zero_or_negative_ignored() {
        with_env_var("ICODE_SESSION_BUDGET_USD", Some("-1"), || {
            let config = budget_config_from_env();
            assert!(config.max_cost_usd.is_none());
        });

        with_env_var("ICODE_SESSION_BUDGET_USD", Some("0"), || {
            let config = budget_config_from_env();
            assert!(config.max_cost_usd.is_none());
        });
    }
}
