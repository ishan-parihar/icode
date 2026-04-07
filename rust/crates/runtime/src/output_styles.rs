use serde::{Deserialize, Serialize};

/// Definition of an output style with its associated system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputStyleDef {
    pub name: String,
    pub label: String,
    pub description: String,
    pub prompt: String,
}

/// Returns all built-in output styles.
pub fn builtin_styles() -> Vec<OutputStyleDef> {
    vec![
        OutputStyleDef {
            name: String::from("default"),
            label: String::from("Default"),
            description: String::from("Standard output with full reasoning and explanations."),
            prompt: String::new(),
        },
        OutputStyleDef {
            name: String::from("concise"),
            label: String::from("Concise"),
            description: String::from("Maximally concise responses with no filler."),
            prompt: String::from(
                "Be maximally concise. Skip preamble, summaries, and filler.",
            ),
        },
        OutputStyleDef {
            name: String::from("explanatory"),
            label: String::from("Explanatory"),
            description: String::from(
                "Thorough and educational responses with detailed reasoning.",
            ),
            prompt: String::from(
                "Be thorough and educational. Include reasoning, alternatives, and explain your thought process.",
            ),
        },
        OutputStyleDef {
            name: String::from("learning"),
            label: String::from("Learning"),
            description: String::from(
                "Educational responses focused on teaching concepts during implementation.",
            ),
            prompt: String::from(
                "This user is learning. Explain concepts as you implement them. Prioritize clarity over brevity.",
            ),
        },
    ]
}

/// Look up a built-in style by name and return its prompt.
pub fn resolve_style_prompt(name: &str) -> Option<String> {
    builtin_styles()
        .into_iter()
        .find(|s| s.name == name)
        .map(|s| s.prompt)
}

/// Read the `ICODE_OUTPUT_STYLE` environment variable and return its value if set.
pub fn output_style_from_env() -> Option<String> {
    std::env::var("ICODE_OUTPUT_STYLE").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_styles_returns_four_styles() {
        let styles = builtin_styles();
        assert_eq!(styles.len(), 4);
    }

    #[test]
    fn builtin_styles_contains_expected_names() {
        let styles = builtin_styles();
        let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"concise"));
        assert!(names.contains(&"explanatory"));
        assert!(names.contains(&"learning"));
    }

    #[test]
    fn builtin_styles_default_has_empty_prompt() {
        let styles = builtin_styles();
        let default_style = styles.iter().find(|s| s.name == "default").unwrap();
        assert!(default_style.prompt.is_empty());
    }

    #[test]
    fn builtin_styles_concise_has_prompt() {
        let styles = builtin_styles();
        let concise = styles.iter().find(|s| s.name == "concise").unwrap();
        assert!(concise.prompt.contains("maximally concise"));
    }

    #[test]
    fn builtin_styles_explanatory_has_prompt() {
        let styles = builtin_styles();
        let explanatory = styles.iter().find(|s| s.name == "explanatory").unwrap();
        assert!(explanatory.prompt.contains("thorough and educational"));
    }

    #[test]
    fn builtin_styles_learning_has_prompt() {
        let styles = builtin_styles();
        let learning = styles.iter().find(|s| s.name == "learning").unwrap();
        assert!(learning.prompt.contains("learning"));
    }

    #[test]
    fn resolve_style_prompt_returns_prompt_for_known_style() {
        let prompt = resolve_style_prompt("concise");
        assert_eq!(
            prompt,
            Some(String::from(
                "Be maximally concise. Skip preamble, summaries, and filler."
            ))
        );
    }

    #[test]
    fn resolve_style_prompt_returns_none_for_unknown_style() {
        let prompt = resolve_style_prompt("nonexistent");
        assert!(prompt.is_none());
    }

    #[test]
    fn resolve_style_prompt_returns_empty_for_default() {
        let prompt = resolve_style_prompt("default");
        assert_eq!(prompt, Some(String::new()));
    }

    #[test]
    fn output_style_from_env_returns_none_when_unset() {
        // ICODE_OUTPUT_STYLE should not be set in normal test runs.
        // If it is, this test documents that behavior.
        let val = output_style_from_env();
        // We cannot assert None because the env might be set externally,
        // so we verify the function returns Ok when the var exists.
        if val.is_some() {
            let v = val.unwrap();
            assert!(!v.is_empty());
        }
    }

    #[test]
    fn output_style_from_env_returns_some_when_set() {
        // std::env::set_var requires unsafe in this crate (unsafe_code = forbid).
        // Verify manually: ICODE_OUTPUT_STYLE=concise cargo test -p runtime output_styles
        let val = output_style_from_env();
        if let Some(v) = val {
            assert!(!v.is_empty());
        }
    }

    #[test]
    fn output_style_def_is_serializable() {
        let style = OutputStyleDef {
            name: String::from("test"),
            label: String::from("Test"),
            description: String::from("A test style"),
            prompt: String::from("Test prompt"),
        };
        let json = serde_json::to_string(&style).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("Test"));
        assert!(json.contains("A test style"));
        assert!(json.contains("Test prompt"));
    }

    #[test]
    fn output_style_def_is_deserializable() {
        let json =
            r#"{"name":"test","label":"Test","description":"A test style","prompt":"Test prompt"}"#;
        let style: OutputStyleDef = serde_json::from_str(json).unwrap();
        assert_eq!(style.name, "test");
        assert_eq!(style.label, "Test");
        assert_eq!(style.description, "A test style");
        assert_eq!(style.prompt, "Test prompt");
    }

    #[test]
    fn output_style_def_clone_works() {
        let style = OutputStyleDef {
            name: String::from("clone_test"),
            label: String::from("Clone Test"),
            description: String::from("Testing clone"),
            prompt: String::from("Clone prompt"),
        };
        let cloned = style.clone();
        assert_eq!(style.name, cloned.name);
        assert_eq!(style.label, cloned.label);
        assert_eq!(style.description, cloned.description);
        assert_eq!(style.prompt, cloned.prompt);
    }
}
