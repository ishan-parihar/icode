use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: Vec<SkillEntry>,
}

impl SkillRegistry {
    #[must_use]
    pub fn discover(roots: &[PathBuf]) -> Self {
        let mut skills: Vec<SkillEntry> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for root in roots {
            if !root.is_dir() {
                continue;
            }

            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let skill_md = path.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }

                let content = match std::fs::read_to_string(&skill_md) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!(
                            "[skill_registry] failed to read {}: {e}",
                            skill_md.display()
                        );
                        continue;
                    }
                };

                let name = entry.file_name().to_string_lossy().to_string();

                if !seen.insert(name.clone()) {
                    eprintln!(
                        "[skill_registry] duplicate skill name '{name}' at {}, keeping existing",
                        skill_md.display()
                    );
                    continue;
                }

                let description = parse_skill_description(&content);

                skills.push(SkillEntry {
                    name,
                    description,
                    path: skill_md,
                    content,
                });
            }
        }

        skills.sort_by(|a, b| a.name.cmp(&b.name));

        Self { skills }
    }

    #[must_use]
    pub fn all(&self) -> &[SkillEntry] {
        &self.skills
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SkillEntry> {
        self.skills
            .binary_search_by(|s| s.name.as_str().cmp(name))
            .ok()
            .map(|idx| &self.skills[idx])
    }

    #[must_use]
    pub fn format_for_system_prompt(&self) -> String {
        if self.skills.is_empty() {
            return String::from("<available_skills>\n</available_skills>");
        }

        let mut xml = String::from("<available_skills>\n");
        for skill in &self.skills {
            xml.push_str("<skill>");
            xml.push_str("<name>");
            xml.push_str(&escape_xml(&skill.name));
            xml.push_str("</name>");
            xml.push_str("<description>");
            xml.push_str(&escape_xml(&skill.description));
            xml.push_str("</description>");
            xml.push_str("</skill>\n");
        }
        xml.push_str("</available_skills>");
        xml
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

fn parse_skill_description(content: &str) -> String {
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("description:") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[must_use]
pub fn default_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        for rel in [
            ".opencode/skills",
            ".opencode/skill",
            ".claude/skills",
            ".agents/skills",
        ] {
            let p = cwd.join(rel);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        for rel in [
            ".claude/skills",
            ".agents/skills",
            ".config/opencode/skills",
            ".codex/skills",
        ] {
            let p = home.join(rel);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }

    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_skill_dir(base: &std::path::Path, name: &str, description: &str, content: &str) {
        let skill_dir = base.join(name);
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        let skill_md = skill_dir.join("SKILL.md");

        let full_content = if description.is_empty() {
            content.to_string()
        } else {
            format!("---\ndescription: {description}\n---\n\n{content}")
        };

        fs::write(&skill_md, full_content).expect("failed to write SKILL.md");
    }

    #[test]
    fn test_discover_skills_from_single_root() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        create_skill_dir(
            temp_dir.path(),
            "frontend-design",
            "Create distinctive UI",
            "# Frontend Design\n\nSkill content here.",
        );
        create_skill_dir(
            temp_dir.path(),
            "audit",
            "Run quality checks",
            "# Audit\n\nAudit skill content.",
        );

        let registry = SkillRegistry::discover(&[temp_dir.path().to_path_buf()]);

        assert_eq!(registry.len(), 2);
        assert!(registry.get("frontend-design").is_some());
        assert!(registry.get("audit").is_some());
    }

    #[test]
    fn test_discover_skills_from_multiple_roots() {
        let temp_a = tempfile::tempdir().expect("failed to create temp dir a");
        let temp_b = tempfile::tempdir().expect("failed to create temp dir b");

        create_skill_dir(temp_a.path(), "skill-a", "Skill from root A", "Content A");
        create_skill_dir(temp_b.path(), "skill-b", "Skill from root B", "Content B");

        let registry =
            SkillRegistry::discover(&[temp_a.path().to_path_buf(), temp_b.path().to_path_buf()]);

        assert_eq!(registry.len(), 2);
        assert!(registry.get("skill-a").is_some());
        assert!(registry.get("skill-b").is_some());
    }

    #[test]
    fn test_duplicate_skill_first_wins() {
        let temp_a = tempfile::tempdir().expect("failed to create temp dir a");
        let temp_b = tempfile::tempdir().expect("failed to create temp dir b");

        create_skill_dir(
            temp_a.path(),
            "duplicate",
            "First description",
            "First content",
        );
        create_skill_dir(
            temp_b.path(),
            "duplicate",
            "Second description",
            "Second content",
        );

        let registry =
            SkillRegistry::discover(&[temp_a.path().to_path_buf(), temp_b.path().to_path_buf()]);

        assert_eq!(registry.len(), 1);
        let skill = registry.get("duplicate").unwrap();
        assert_eq!(skill.description, "First description");
    }

    #[test]
    fn test_discover_skips_non_skill_dirs() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

        fs::create_dir_all(temp_dir.path().join("no-skill")).expect("failed to create dir");
        create_skill_dir(temp_dir.path(), "real-skill", "A real skill", "Content");

        let registry = SkillRegistry::discover(&[temp_dir.path().to_path_buf()]);
        assert_eq!(registry.len(), 1);
        assert!(registry.get("real-skill").is_some());
        assert!(registry.get("no-skill").is_none());
    }

    #[test]
    fn test_discover_handles_missing_root() {
        let non_existent = PathBuf::from("/tmp/nonexistent_skill_dir_12345");
        let registry = SkillRegistry::discover(&[non_existent]);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_skill_entry_content_preserved() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let full_content = "# My Skill\n\nThis is the full markdown content.";
        create_skill_dir(temp_dir.path(), "my-skill", "My description", full_content);

        let registry = SkillRegistry::discover(&[temp_dir.path().to_path_buf()]);
        let skill = registry.get("my-skill").unwrap();

        assert!(skill.content.contains("My Skill"));
        assert!(skill.content.contains("full markdown content"));
        assert!(skill.path.ends_with("my-skill/SKILL.md"));
    }

    #[test]
    fn test_format_for_system_prompt() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        create_skill_dir(temp_dir.path(), "alpha", "Alpha skill", "Content A");
        create_skill_dir(temp_dir.path(), "beta", "Beta skill", "Content B");

        let registry = SkillRegistry::discover(&[temp_dir.path().to_path_buf()]);
        let xml = registry.format_for_system_prompt();

        assert!(xml.starts_with("<available_skills>\n"));
        assert!(xml.ends_with("</available_skills>"));
        assert!(xml.contains("<name>alpha</name>"));
        assert!(xml.contains("<description>Alpha skill</description>"));
        assert!(xml.contains("<name>beta</name>"));
        assert!(xml.contains("<description>Beta skill</description>"));
    }

    #[test]
    fn test_format_for_system_prompt_empty() {
        let registry = SkillRegistry::discover(&[]);
        let xml = registry.format_for_system_prompt();
        assert_eq!(xml, "<available_skills>\n</available_skills>");
    }

    #[test]
    fn test_format_for_system_prompt_escapes_xml() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        create_skill_dir(
            temp_dir.path(),
            "risky",
            "Has <special> & 'chars'",
            "Content with <tags>",
        );

        let registry = SkillRegistry::discover(&[temp_dir.path().to_path_buf()]);
        let xml = registry.format_for_system_prompt();

        assert!(!xml.contains("<special>"));
        assert!(xml.contains("&lt;special&gt;"));
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&apos;"));
    }

    #[test]
    fn test_all_returns_sorted_skills() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        create_skill_dir(temp_dir.path(), "zebra", "Z", "Z");
        create_skill_dir(temp_dir.path(), "alpha", "A", "A");
        create_skill_dir(temp_dir.path(), "middle", "M", "M");

        let registry = SkillRegistry::discover(&[temp_dir.path().to_path_buf()]);
        let all = registry.all();

        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "alpha");
        assert_eq!(all[1].name, "middle");
        assert_eq!(all[2].name, "zebra");
    }

    #[test]
    fn test_parse_skill_description_with_frontmatter() {
        let content = "---\ntitle: My Skill\ndescription: A great skill for testing\nversion: 1.0\n---\n\n# My Skill\n\nContent here.";
        assert_eq!(
            parse_skill_description(content),
            "A great skill for testing"
        );
    }

    #[test]
    fn test_parse_skill_description_no_frontmatter() {
        let content = "# My Skill\n\nThis skill does things.\n\ndescription: fallback line";
        assert_eq!(parse_skill_description(content), "fallback line");
    }

    #[test]
    fn test_parse_skill_description_empty_value() {
        let content = "---\ndescription: \n---\n\nContent";
        assert_eq!(parse_skill_description(content), "");
    }

    #[test]
    fn test_parse_skill_description_not_found() {
        let content = "# No Description\n\nJust content, no description field.";
        assert_eq!(parse_skill_description(content), "");
    }

    #[test]
    fn test_get_returns_none_for_missing_skill() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        create_skill_dir(temp_dir.path(), "exists", "desc", "content");

        let registry = SkillRegistry::discover(&[temp_dir.path().to_path_buf()]);
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_all_returns_empty_slice_for_empty_registry() {
        let registry = SkillRegistry::discover(&[]);
        let all = registry.all();
        assert!(all.is_empty());
    }
}
