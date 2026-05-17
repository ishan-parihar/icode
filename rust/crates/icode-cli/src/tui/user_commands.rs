use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct UserCommand {
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub template: String,
}

#[derive(Debug, Clone)]
pub struct UserCommandRegistry {
    commands: HashMap<String, UserCommand>,
}

impl UserCommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&UserCommand> {
        self.commands.get(name)
    }

    pub fn all(&self) -> Vec<&UserCommand> {
        let mut cmds: Vec<&UserCommand> = self.commands.values().collect();
        cmds.sort_by(|a, b| a.name.cmp(&b.name));
        cmds
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }

    pub fn resolve_template(&self, name: &str, args: &[&str]) -> Option<String> {
        let cmd = self.commands.get(name)?;
        Some(substitute_template(&cmd.template, args))
    }
}

pub fn load_user_commands() -> UserCommandRegistry {
    let commands_dir = commands_dir();
    let mut registry = UserCommandRegistry::new();

    if !commands_dir.is_dir() {
        return registry;
    }

    let entries = match fs::read_dir(&commands_dir) {
        Ok(e) => e,
        Err(_) => return registry,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Some(cmd) = parse_command_file(&path) {
            registry.commands.insert(cmd.name.clone(), cmd);
        }
    }

    registry
}

fn commands_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        let p = PathBuf::from(&path).join("commands");
        if p.is_dir() {
            return p;
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".icode").join("commands")
}

fn parse_command_file(path: &Path) -> Option<UserCommand> {
    let content = fs::read_to_string(path).ok()?;
    let content = content.trim();

    if !content.starts_with("---") {
        return None;
    }

    let rest = content.trim_start_matches("---").trim_start();
    let end = rest.find("\n---")?;
    let frontmatter_str = &rest[..end];
    let body = rest[end + 4..].trim();

    let frontmatter = parse_frontmatter(frontmatter_str);

    let name = path.file_stem()?.to_str()?.to_string();
    let description = frontmatter
        .get("description")
        .cloned()
        .unwrap_or_default();
    let model = frontmatter.get("model").cloned();
    let template = body.to_string();

    if template.is_empty() {
        return None;
    }

    Some(UserCommand {
        name,
        description,
        model,
        template,
    })
}

fn parse_frontmatter(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(eq_pos) = line.find(':') {
            let key = line[..eq_pos].trim().to_string();
            let value = line[eq_pos + 1..].trim().trim_matches('"').to_string();
            if !key.is_empty() {
                map.insert(key, value);
            }
        }
    }
    map
}

fn substitute_template(template: &str, args: &[&str]) -> String {
    let mut result = template.to_string();

    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("${}", i + 1);
        result = result.replace(&placeholder, arg);
    }

    for i in args.len() + 1..=9 {
        let placeholder = format!("${i}");
        result = result.replace(&placeholder, "");
    }

    let all_args = args.join(" ");
    result = result.replace("$ARGUMENTS", &all_args);

    result
}

pub fn create_example_command() -> UserCommand {
    UserCommand {
        name: "explain".to_string(),
        description: "Explain a code pattern or concept in detail".to_string(),
        model: None,
        template: "Explain $ARGUMENTS in detail with examples. "
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_temp_command_file(dir: &Path, name: &str, description: &str, model: Option<&str>, body: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{name}.md"));
        let mut file = fs::File::create(&path).unwrap();
        if let Some(m) = model {
            writeln!(file, "---\ndescription: \"{description}\"\nmodel: \"{m}\"\n---\n{body}").unwrap();
        } else {
            writeln!(file, "---\ndescription: \"{description}\"\n---\n{body}").unwrap();
        }
        path
    }

    #[test]
    fn test_parse_frontmatter_basic() {
        let input = "description: \"Test command\"\nmodel: \"claude\"";
        let map = parse_frontmatter(input);
        assert_eq!(map.get("description").unwrap(), "Test command");
        assert_eq!(map.get("model").unwrap(), "claude");
    }

    #[test]
    fn test_parse_frontmatter_empty() {
        let map = parse_frontmatter("");
        assert!(map.is_empty());
    }

    #[test]
    fn test_parse_frontmatter_no_value() {
        let input = "description:";
        let map = parse_frontmatter(input);
        assert_eq!(map.get("description").unwrap(), "");
    }

    #[test]
    fn test_substitute_arguments() {
        let template = "Explain $1 with $2 and $ARGUMENTS";
        let args = &["rust", "examples", "rust concurrency patterns"];
        let result = substitute_template(template, args);
        assert_eq!(result, "Explain rust with examples and rust examples rust concurrency patterns");
    }

    #[test]
    fn test_substitute_no_args() {
        let template = "Hello $1 $ARGUMENTS";
        let args: &[&str] = &[];
        let result = substitute_template(template, args);
        assert_eq!(result, "Hello  ");
    }

    #[test]
    fn test_substitute_repeated_placeholder() {
        let template = "Explain $1. $1 is important because...";
        let args = &["Rust"];
        let result = substitute_template(template, args);
        assert_eq!(result, "Explain Rust. Rust is important because...");
    }

    #[test]
    fn test_parse_command_file() {
        let dir = PathBuf::from("/tmp/icode-test-commands-parse");
        let _ = fs::remove_dir_all(&dir);
        let path = create_temp_command_file(
            &dir,
            "analyze",
            "Analyze code quality",
            Some("claude"),
            "Analyze the following code for quality issues: $ARGUMENTS",
        );
        let cmd = parse_command_file(&path).unwrap();
        assert_eq!(cmd.name, "analyze");
        assert_eq!(cmd.description, "Analyze code quality");
        assert_eq!(cmd.model.unwrap(), "claude");
        assert_eq!(cmd.template, "Analyze the following code for quality issues: $ARGUMENTS");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_command_file_no_model() {
        let dir = PathBuf::from("/tmp/icode-test-commands-no-model");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = create_temp_command_file(
            &dir,
            "review",
            "Review changes",
            None,
            "Review the following changes: $ARGUMENTS",
        );
        let cmd = parse_command_file(&path).unwrap();
        assert_eq!(cmd.name, "review");
        assert_eq!(cmd.description, "Review changes");
        assert!(cmd.model.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_command_file_no_frontmatter() {
        let dir = PathBuf::from("/tmp/icode-test-commands-no-fm");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.md");
        fs::write(&path, "Just a regular markdown file with no frontmatter").unwrap();
        assert!(parse_command_file(&path).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_command_file_empty_body() {
        let dir = PathBuf::from("/tmp/icode-test-commands-empty-body");
        let _ = fs::remove_dir_all(&dir);
        let path = create_temp_command_file(
            &dir,
            "empty",
            "Empty command",
            None,
            "",
        );
        assert!(parse_command_file(&path).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_user_commands_from_dir() {
        let dir = PathBuf::from("/tmp/icode-test-load-commands");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        create_temp_command_file(&dir, "fmt", "Format code", None, "Format the code: $ARGUMENTS");
        create_temp_command_file(&dir, "lint", "Lint code", Some("claude"), "Lint the code: $ARGUMENTS");
        create_temp_command_file(&dir, "readme", "Generate readme", None, "Generate a README for $1");

        let fmt_path = dir.join("fmt.md");
        let lint_path = dir.join("lint.md");
        let readme_path = dir.join("readme.md");

        let fmt_cmd = parse_command_file(&fmt_path).unwrap();
        assert_eq!(fmt_cmd.name, "fmt");
        let lint_cmd = parse_command_file(&lint_path).unwrap();
        assert_eq!(lint_cmd.name, "lint");
        let readme_cmd = parse_command_file(&readme_path).unwrap();
        assert_eq!(readme_cmd.name, "readme");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_registry_basic() {
        let mut registry = UserCommandRegistry::new();
        assert!(registry.is_empty());
        registry.commands.insert(
            "test".to_string(),
            UserCommand {
                name: "test".to_string(),
                description: "A test command".to_string(),
                model: None,
                template: "Test $1".to_string(),
            },
        );
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("test"));
        assert!(!registry.contains("nonexistent"));

        let resolved = registry.resolve_template("test", &["hello"]).unwrap();
        assert_eq!(resolved, "Test hello");
    }

    #[test]
    fn test_registry_resolve_nonexistent() {
        let registry = UserCommandRegistry::new();
        assert!(registry.resolve_template("nonexistent", &[]).is_none());
    }
}
