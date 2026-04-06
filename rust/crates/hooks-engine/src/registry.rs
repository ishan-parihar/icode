use std::sync::Arc;

use crate::dispatcher::HookDispatcher;
use crate::hook_trait::Hook;

pub struct HookRegistry {
    hooks: Vec<Arc<dyn Hook>>,
}

impl HookRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: Arc<dyn Hook>) {
        self.hooks.push(hook);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Hook>> {
        self.hooks.iter().find(|h| h.name() == name).cloned()
    }

    #[must_use]
    pub fn list(&self) -> Vec<&Arc<dyn Hook>> {
        self.hooks.iter().collect()
    }

    #[must_use]
    pub fn into_dispatcher(self) -> HookDispatcher {
        let mut dispatcher = HookDispatcher::new();
        for hook in self.hooks {
            dispatcher.register(hook);
        }
        dispatcher
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
