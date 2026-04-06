use super::CategoryConfig;

/// Return the 8 builtin category configurations from oh-my-openagent.
#[must_use]
pub fn builtin_categories() -> Vec<CategoryConfig> {
    vec![
        CategoryConfig {
            name: "visual-engineering".into(),
            description: "Frontend, UI/UX, design, styling, animation".into(),
            model: "google/gemini-3.1-pro".into(),
            ..Default::default()
        },
        CategoryConfig {
            name: "ultrabrain".into(),
            description: "Deep logical reasoning, complex architecture decisions".into(),
            model: "openai/gpt-5.4".into(),
            variant: Some("xhigh".into()),
            ..Default::default()
        },
        CategoryConfig {
            name: "deep".into(),
            description:
                "Goal-oriented autonomous problem-solving. Thorough research before action.".into(),
            model: "openai/gpt-5.4".into(),
            variant: Some("medium".into()),
            ..Default::default()
        },
        CategoryConfig {
            name: "artistry".into(),
            description: "Highly creative/artistic tasks, novel ideas".into(),
            model: "google/gemini-3.1-pro".into(),
            variant: Some("high".into()),
            ..Default::default()
        },
        CategoryConfig {
            name: "quick".into(),
            description: "Trivial tasks - single file changes, typo fixes".into(),
            model: "openai/gpt-5.4-mini".into(),
            ..Default::default()
        },
        CategoryConfig {
            name: "unspecified-low".into(),
            description: "Tasks that don't fit other categories, low effort".into(),
            model: "anthropic/claude-sonnet-4-6".into(),
            ..Default::default()
        },
        CategoryConfig {
            name: "unspecified-high".into(),
            description: "Tasks that don't fit other categories, high effort".into(),
            model: "anthropic/claude-opus-4-6".into(),
            variant: Some("max".into()),
            ..Default::default()
        },
        CategoryConfig {
            name: "writing".into(),
            description: "Documentation, prose, technical writing".into(),
            model: "google/gemini-3-flash".into(),
            ..Default::default()
        },
    ]
}
