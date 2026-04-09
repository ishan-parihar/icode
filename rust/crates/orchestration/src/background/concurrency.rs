use std::collections::HashMap;
use std::sync::RwLock;

pub struct ConcurrencyLimiter {
    limits: RwLock<HashMap<String, usize>>,
    active: RwLock<HashMap<String, usize>>,
    default_limit: usize,
}

impl ConcurrencyLimiter {
    #[must_use]
    pub fn new(default_limit: usize) -> Self {
        Self {
            limits: RwLock::new(HashMap::new()),
            active: RwLock::new(HashMap::new()),
            default_limit,
        }
    }

    pub fn set_limit(&self, model: String, limit: usize) {
        self.limits
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(model, limit);
    }

    pub fn try_acquire(&self, model: &str) -> bool {
        let limits = self
            .limits
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let limit = limits.get(model).copied().unwrap_or(self.default_limit);
        let mut active = self
            .active
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = active.entry(model.to_string()).or_insert(0);
        if *count < limit {
            *count += 1;
            true
        } else {
            false
        }
    }

    pub fn release(&self, model: &str) {
        let mut active = self
            .active
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = active.get_mut(model) {
            *count = count.saturating_sub(1);
        }
    }

    pub fn active_count(&self, model: &str) -> usize {
        self.active
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(model)
            .copied()
            .unwrap_or(0)
    }
}
