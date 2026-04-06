use crate::types::ThinkingConfig;
use serde::{Deserialize, Serialize};

pub mod builtin;
pub mod resolver;

/// Category-level model configuration that maps task types to model settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategoryConfig {
    pub name: String,
    pub description: String,
    pub model: String,
    pub variant: Option<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub prompt_append: Option<String>,
    pub thinking: Option<ThinkingConfig>,
    pub reasoning_effort: Option<String>,
    pub text_verbosity: Option<String>,
    pub max_tokens: Option<u32>,
    pub disabled_tools: Vec<String>,
    pub is_unstable_agent: bool,
}

pub use builtin::builtin_categories;
pub use resolver::CategoryResolver;
