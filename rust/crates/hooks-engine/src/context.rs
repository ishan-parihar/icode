use std::collections::HashMap;

/// Mutable context passed to hook implementations
pub struct HookContext {
    /// Messages to inject into the conversation
    pub injected_messages: Vec<String>,
    /// Whether to block the current operation
    pub blocked: bool,
    /// Block reason (if blocked)
    pub block_reason: Option<String>,
    /// Warnings to attach to output
    pub warnings: Vec<String>,
    /// Key-value metadata for cross-hook communication
    pub metadata: HashMap<String, String>,
}

impl HookContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            injected_messages: Vec::new(),
            blocked: false,
            block_reason: None,
            warnings: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn inject_message(&mut self, message: String) {
        self.injected_messages.push(message);
    }

    pub fn block(&mut self, reason: String) {
        self.blocked = true;
        self.block_reason = Some(reason);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }
}

impl Default for HookContext {
    fn default() -> Self {
        Self::new()
    }
}
