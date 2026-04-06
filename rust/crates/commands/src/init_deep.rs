use std::fs;
use std::path::Path;

/// Handle `/init-deep` command.
///
/// Generates hierarchical AGENTS.md content by scanning the project directory
/// for key configuration files and summarizing the project structure.
pub fn handle_init_deep(directory: &str) -> Result<String, String> {
    let dir_path = Path::new(directory);
    if !dir_path.is_dir() {
        return Err(format!("'{directory}' is not a valid directory."));
    }

    let mut lines = vec![
        "# Project Context".to_string(),
        String::new(),
        format!("## Directory\n`{directory}`"),
        String::new(),
    ];

    // Detect key files
    let key_files = [
        ("Cargo.toml", "Rust workspace configuration"),
        ("package.json", "Node.js package configuration"),
        ("pyproject.toml", "Python project configuration"),
        ("go.mod", "Go module configuration"),
        ("README.md", "Project documentation"),
        (".gitignore", "Git ignore rules"),
        ("Makefile", "Build automation"),
        ("Dockerfile", "Container configuration"),
        ("docker-compose.yml", "Multi-container configuration"),
    ];

    let mut found_files: Vec<(&str, &str)> = Vec::new();
    for (filename, description) in &key_files {
        if dir_path.join(filename).is_file() {
            found_files.push((filename, description));
        }
    }

    if !found_files.is_empty() {
        lines.push("## Key Files".to_string());
        for (filename, description) in &found_files {
            lines.push(format!("- `{filename}` — {description}"));
        }
        lines.push(String::new());
    }

    // Detect languages and frameworks
    let mut languages: Vec<&str> = Vec::new();
    let mut frameworks: Vec<&str> = Vec::new();

    if dir_path.join("Cargo.toml").is_file() {
        languages.push("Rust");
    }
    if dir_path.join("package.json").is_file() {
        languages.push("TypeScript/JavaScript");
        let pkg = fs::read_to_string(dir_path.join("package.json")).ok();
        if let Some(ref content) = pkg {
            if content.contains("\"next\"") || content.contains("next.js") {
                frameworks.push("Next.js");
            }
            if content.contains("\"react\"") {
                frameworks.push("React");
            }
            if content.contains("\"tailwind\"") {
                frameworks.push("Tailwind CSS");
            }
        }
    }
    if dir_path.join("pyproject.toml").is_file() || dir_path.join("requirements.txt").is_file() {
        languages.push("Python");
    }
    if dir_path.join("go.mod").is_file() {
        languages.push("Go");
    }
    if dir_path.join("pom.xml").is_file() {
        languages.push("Java");
    }

    if !languages.is_empty() {
        lines.push(format!("## Languages\n{}", languages.join(", ")));
        lines.push(String::new());
    }

    if !frameworks.is_empty() {
        lines.push(format!("## Frameworks\n{}", frameworks.join(", ")));
        lines.push(String::new());
    }

    // Subdirectory summary
    if let Ok(entries) = fs::read_dir(directory) {
        let mut dirs: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if !name_str.starts_with('.') {
                    dirs.push(name_str.to_string());
                }
            }
        }
        if !dirs.is_empty() {
            dirs.sort();
            lines.push("## Structure".to_string());
            for d in &dirs {
                lines.push(format!("- `{d}/`"));
            }
            lines.push(String::new());
        }
    }

    lines.push(
        "## Agent Guidelines\n- Read this file before starting any task.\n- Update sections as the project evolves.".to_string(),
    );

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("init-deep-{label}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn init_deep_detects_rust_project() {
        let dir = temp_dir("rust");
        fs::write(format!("{dir}/Cargo.toml"), "[workspace]\n").unwrap();
        fs::create_dir_all(format!("{dir}/src")).unwrap();

        let result = handle_init_deep(&dir);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.contains("Rust"));
        assert!(msg.contains("Cargo.toml"));
        assert!(msg.contains("src/"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn init_deep_rejects_invalid_path() {
        let result = handle_init_deep("/nonexistent/path/12345");
        assert!(result.is_err());
    }
}
