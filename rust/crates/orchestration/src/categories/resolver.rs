use std::collections::HashMap;
use std::fmt::Write;

use crate::agent_config::AgentConfigBuilder;
use crate::types::AgentConfig;

use super::{builtin_categories, CategoryConfig};

/// Resolves category names to fully-configured `AgentConfig` instances.
pub struct CategoryResolver {
    categories: HashMap<String, CategoryConfig>,
}

impl CategoryResolver {
    /// Create a resolver populated with all builtin categories.
    #[must_use]
    pub fn new() -> Self {
        Self::with_overrides(Vec::new())
    }

    /// Create a resolver with user-provided overrides replacing builtins by name.
    #[must_use]
    pub fn with_overrides(overrides: Vec<CategoryConfig>) -> Self {
        let mut categories: HashMap<String, CategoryConfig> = builtin_categories()
            .into_iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        for override_config in overrides {
            categories.insert(override_config.name.clone(), override_config);
        }

        Self { categories }
    }

    /// Resolve a category name to an `AgentConfig`, optionally injecting skill prompts.
    #[must_use]
    pub fn resolve(
        &self,
        category_name: &str,
        skills: &[String],
        skill_prompts: &[String],
    ) -> Option<AgentConfig> {
        let cat = self.categories.get(category_name)?;

        let mut builder = AgentConfigBuilder::new()
            .name(&cat.name)
            .description(&cat.description)
            .model(&cat.model);

        if let Some(ref variant) = cat.variant {
            builder = builder.reasoning_effort(variant);
        }
        if let Some(effort) = &cat.reasoning_effort {
            builder = builder.reasoning_effort(effort);
        }
        if let Some(temp) = cat.temperature {
            builder = builder.temperature(temp);
        }
        if let Some(max) = cat.max_tokens {
            builder = builder.max_tokens(max);
        }
        if !cat.disabled_tools.is_empty() {
            builder = builder.disabled_tools(cat.disabled_tools.clone());
        }

        let mut base_prompt = format!("You are the {} agent. {}\n", cat.name, cat.description);

        let skill_section = build_skill_prompt(skills, skill_prompts);
        if !skill_section.is_empty() {
            base_prompt.push_str(&skill_section);
        }

        if let Some(ref append) = cat.prompt_append {
            base_prompt.push_str(append);
        }

        let agent = builder.prompt(&base_prompt).build();
        Some(agent)
    }

    /// Return all available category names.
    #[must_use]
    pub fn available_categories(&self) -> Vec<&str> {
        self.categories.keys().map(String::as_str).collect()
    }

    /// Get the description for a category, if it exists.
    #[must_use]
    pub fn description(&self, name: &str) -> Option<&str> {
        self.categories.get(name).map(|c| c.description.as_str())
    }
}

impl Default for CategoryResolver {
    fn default() -> Self {
        Self::new()
    }
}

fn build_skill_prompt(skills: &[String], skill_prompts: &[String]) -> String {
    if skills.is_empty() || skill_prompts.is_empty() {
        return String::new();
    }

    let mut section = String::from("\n## Active Skills\n");
    for (skill, prompt) in skills.iter().zip(skill_prompts.iter()) {
        let _ = write!(section, "\n### {skill}\n{prompt}\n");
    }
    section
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_skill_prompt_empty_when_no_skills() {
        let result = build_skill_prompt(&[], &["some prompt".into()]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_skill_prompt_empty_when_no_prompts() {
        let result = build_skill_prompt(&["skill1".into()], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_skill_prompt_includes_skills() {
        let result = build_skill_prompt(
            &["frontend-design".into()],
            &["Design beautiful UIs".into()],
        );
        assert!(result.contains("frontend-design"));
        assert!(result.contains("Design beautiful UIs"));
    }
}
