use std::collections::HashMap;
use std::sync::Mutex;

pub struct ConcurrencyLimiter {
    limits: HashMap<String, usize>,
    active: Mutex<HashMap<String, usize>>,
    default_limit: usize,
}

impl ConcurrencyLimiter {
    #[must_use]
    pub fn new(default_limit: usize) -> Self {
        Self {
            limits: HashMap::new(),
            active: Mutex::new(HashMap::new()),
            default_limit,
        }
    }

    pub fn set_limit(&mut self, model: String, limit: usize) {
        self.limits.insert(model, limit);
    }

    pub fn try_acquire(&self, model: &str) -> bool {
        let limit = self
            .limits
            .get(model)
            .copied()
            .unwrap_or(self.default_limit);
        let mut active = self.active.lock().expect("mutex poisoned");
        let count = active.entry(model.to_string()).or_insert(0);
        if *count < limit {
            *count += 1;
            true
        } else {
            false
        }
    }

    pub fn release(&self, model: &str) {
        let mut active = self.active.lock().expect("mutex poisoned");
        if let Some(count) = active.get_mut(model) {
            *count = count.saturating_sub(1);
        }
    }

    pub fn active_count(&self, model: &str) -> usize {
        self.active
            .lock()
            .expect("mutex poisoned")
            .get(model)
            .copied()
            .unwrap_or(0)
    }
}
