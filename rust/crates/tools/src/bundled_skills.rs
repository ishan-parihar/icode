/// Bundled skill definitions with prompt templates.
///
/// Each bundled skill represents a pre-defined instruction template that can be
/// looked up by name or alias and expanded with user-provided arguments.

/// A single bundled skill with metadata and a prompt template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledSkill {
    /// Canonical name of the skill.
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// Alternative names that resolve to this skill.
    pub aliases: &'static [&'static str],
    /// Prompt template. `$ARGUMENTS` is replaced with user input during expansion.
    pub prompt_template: &'static str,
    /// Whether this skill can be invoked directly by the user.
    pub user_invocable: bool,
}

const BUNDLED_SKILLS: &[BundledSkill] = &[
    BundledSkill {
        name: "simplify",
        description: "Simplify the code in the current file",
        aliases: &["clean", "refactor"],
        prompt_template: "Simplify the code in the current file. Focus on reducing complexity, removing duplication, and improving readability while preserving behavior.",
        user_invocable: true,
    },
    BundledSkill {
        name: "debug",
        description: "Debug a specific issue",
        aliases: &["fix", "troubleshoot"],
        prompt_template: "Debug a specific issue. $ARGUMENTS",
        user_invocable: true,
    },
    BundledSkill {
        name: "stuck",
        description: "I'm stuck — help me figure out what to do next",
        aliases: &["help", "unblock"],
        prompt_template: "I'm stuck. $ARGUMENTS",
        user_invocable: true,
    },
    BundledSkill {
        name: "verify",
        description: "Verify the current implementation",
        aliases: &["check", "validate"],
        prompt_template: "Verify the current implementation. Check for correctness, edge cases, and potential issues.",
        user_invocable: true,
    },
    BundledSkill {
        name: "explain",
        description: "Explain the code",
        aliases: &["describe", "walkthrough"],
        prompt_template: "Explain the code. $ARGUMENTS",
        user_invocable: true,
    },
];

/// Look up a bundled skill by name or alias (case-insensitive).
///
/// Returns `None` if no matching skill is found.
#[must_use]
pub fn find_bundled_skill(name: &str) -> Option<&'static BundledSkill> {
    let lower = name.trim().to_ascii_lowercase();
    BUNDLED_SKILLS.iter().find(|skill| {
        skill.name.to_ascii_lowercase() == lower
            || skill
                .aliases
                .iter()
                .any(|alias| alias.to_ascii_lowercase() == lower)
    })
}

/// Return all user-invocable skills as `(name, description)` pairs.
#[must_use]
pub fn user_invocable_skills() -> Vec<(String, String)> {
    BUNDLED_SKILLS
        .iter()
        .filter(|skill| skill.user_invocable)
        .map(|skill| (skill.name.to_string(), skill.description.to_string()))
        .collect()
}

/// Expand a skill's prompt template by replacing `$ARGUMENTS` with the provided args.
///
/// If `$ARGUMENTS` does not appear in the template, the args are appended to the
/// end of the template with a trailing space separator.
#[must_use]
pub fn expand_prompt(skill: &BundledSkill, args: &str) -> String {
    if skill.prompt_template.contains("$ARGUMENTS") {
        skill.prompt_template.replace("$ARGUMENTS", args)
    } else if args.is_empty() {
        skill.prompt_template.to_string()
    } else {
        format!("{} {}", skill.prompt_template, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_by_name() {
        let skill = find_bundled_skill("simplify").expect("should find simplify");
        assert_eq!(skill.name, "simplify");
        assert_eq!(skill.description, "Simplify the code in the current file");
    }

    #[test]
    fn test_find_by_alias() {
        let skill = find_bundled_skill("clean").expect("should find simplify via alias");
        assert_eq!(skill.name, "simplify");

        let skill = find_bundled_skill("fix").expect("should find debug via alias");
        assert_eq!(skill.name, "debug");

        let skill = find_bundled_skill("help").expect("should find stuck via alias");
        assert_eq!(skill.name, "stuck");
    }

    #[test]
    fn test_find_case_insensitive() {
        assert_eq!(find_bundled_skill("DEBUG").map(|s| s.name), Some("debug"));
        assert_eq!(find_bundled_skill("Verify").map(|s| s.name), Some("verify"));
        assert_eq!(
            find_bundled_skill("EXPLAIN").map(|s| s.name),
            Some("explain")
        );
    }

    #[test]
    fn test_find_unknown_returns_none() {
        assert!(find_bundled_skill("nonexistent").is_none());
        assert!(find_bundled_skill("").is_none());
    }

    #[test]
    fn test_expand_prompt_with_arguments() {
        let skill = find_bundled_skill("debug").expect("debug skill");
        let expanded = expand_prompt(skill, "the app crashes on startup");
        assert_eq!(
            expanded,
            "Debug a specific issue. the app crashes on startup"
        );
    }

    #[test]
    fn test_expand_prompt_empty_arguments() {
        let skill = find_bundled_skill("debug").expect("debug skill");
        let expanded = expand_prompt(skill, "");
        assert_eq!(expanded, "Debug a specific issue. ");
    }

    #[test]
    fn test_expand_prompt_without_placeholder() {
        let skill = find_bundled_skill("simplify").expect("simplify skill");
        let expanded = expand_prompt(skill, "make it shorter");
        assert_eq!(
            expanded,
            "Simplify the code in the current file. Focus on reducing complexity, removing duplication, and improving readability while preserving behavior. make it shorter"
        );
    }

    #[test]
    fn test_expand_prompt_without_placeholder_no_args() {
        let skill = find_bundled_skill("simplify").expect("simplify skill");
        let expanded = expand_prompt(skill, "");
        assert!(expanded.contains("Simplify the code"));
        assert!(!expanded.ends_with(' '));
    }

    #[test]
    fn test_user_invocable_skills_lists_all_five() {
        let skills = user_invocable_skills();
        assert_eq!(skills.len(), 5);

        let names: Vec<&str> = skills.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"simplify"));
        assert!(names.contains(&"debug"));
        assert!(names.contains(&"stuck"));
        assert!(names.contains(&"verify"));
        assert!(names.contains(&"explain"));
    }

    #[test]
    fn test_user_invocable_skills_have_descriptions() {
        let skills = user_invocable_skills();
        for (name, description) in &skills {
            assert!(
                !description.is_empty(),
                "skill '{name}' has an empty description"
            );
        }
    }

    #[test]
    fn test_all_skills_are_user_invocable() {
        for skill in BUNDLED_SKILLS {
            assert!(
                skill.user_invocable,
                "skill '{}' should be user invocable",
                skill.name
            );
        }
    }

    #[test]
    fn test_no_duplicate_names_or_aliases() {
        let mut seen = std::collections::HashSet::new();
        for skill in BUNDLED_SKILLS {
            assert!(
                seen.insert(skill.name.to_ascii_lowercase()),
                "duplicate skill name: {}",
                skill.name
            );
            for alias in skill.aliases {
                assert!(
                    seen.insert(alias.to_ascii_lowercase()),
                    "duplicate alias: {alias} (used by {})",
                    skill.name
                );
            }
        }
    }
}
