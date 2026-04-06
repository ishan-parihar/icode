use std::fmt::Write;

/// Builds the final prompt for a delegated task by injecting skill instructions.
pub struct PromptBuilder;

impl PromptBuilder {
    /// Build the final prompt with skill context prepended.
    ///
    /// If skills are provided, prepends a `[SKILL CONTEXT]` block with each skill
    /// formatted as `## Skill: {name}\n{prompt}\n---\n`, then appends the base prompt.
    /// If no skills, returns the base prompt unchanged.
    #[must_use]
    pub fn build(base_prompt: &str, skills: &[String], skill_prompts: &[String]) -> String {
        if skills.is_empty() || skill_prompts.is_empty() {
            return base_prompt.to_string();
        }

        debug_assert_eq!(
            skills.len(),
            skill_prompts.len(),
            "skills and skill_prompts must have the same length"
        );

        let mut result = String::from("[SKILL CONTEXT]\n");

        for (skill, prompt) in skills.iter().zip(skill_prompts.iter()) {
            let _ = write!(result, "## Skill: {skill}\n{prompt}\n---\n");
        }

        let _ = write!(result, "\n{base_prompt}");
        result
    }
}
