use std::path::Path;

/// Files that should not be auto-edited by the agent.
const PROTECTED_FILES: &[&str] = &[
    ".gitconfig",
    ".gitignore",
    ".gitattributes",
    ".bashrc",
    ".zshrc",
    ".bash_profile",
    ".profile",
    ".mcp.json",
    ".env",
    ".env.local",
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    ".DS_Store",
    "Thumbs.db",
];

/// File patterns (glob-like) that are protected.
const PROTECTED_PATTERNS: &[&str] = &[".git/**", "**/.env*", "**/secrets.*"];

/// Check if a file path is protected.
pub fn is_protected(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Check exact matches
    if PROTECTED_FILES.contains(&file_name) {
        return true;
    }

    // Check patterns (simple prefix/glob matching)
    for pattern in PROTECTED_PATTERNS {
        if matches_pattern(path, pattern) {
            return true;
        }
    }

    false
}

/// Get a warning message for attempting to edit a protected file.
pub fn protected_file_warning(path: &Path) -> String {
    format!(
        "Attempting to modify protected file: {}. This file is guarded and should be edited manually.",
        path.display()
    )
}

/// Simple pattern matching for protected file patterns.
/// Supports:
/// - `.git/**` — any path under `.git/`
/// - `**/.env*` — any file starting with `.env` at any level
/// - `**/secrets.*` — any file named `secrets.*` at any level
fn matches_pattern(path: &Path, pattern: &str) -> bool {
    let path_str = path.to_string_lossy();
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if pattern.ends_with("/**") {
        // Directory prefix match: `.git/**`
        let prefix = pattern.strip_suffix("/**").unwrap_or(pattern);
        path_str.starts_with(prefix)
            || path_str.contains(&format!("/{prefix}/"))
            || path_str.contains(&format!("/{prefix}"))
    } else if pattern.starts_with("**/") {
        // Suffix match at any level: `**/.env*`, `**/secrets.*`
        let suffix_pattern = pattern.strip_prefix("**/").unwrap_or(pattern);
        if suffix_pattern.ends_with('*') {
            // Prefix match on filename: `.env*`
            let prefix = suffix_pattern.strip_suffix('*').unwrap_or(suffix_pattern);
            file_name.starts_with(prefix)
        } else if suffix_pattern.contains('*') {
            // Simple glob: `secrets.*`
            let parts: Vec<&str> = suffix_pattern.split('*').collect();
            if parts.len() == 2 {
                file_name.starts_with(parts[0]) && file_name.ends_with(parts[1])
            } else {
                false
            }
        } else {
            // Exact filename match
            file_name == suffix_pattern
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protected_exact_matches() {
        assert!(is_protected(Path::new(".gitconfig")));
        assert!(is_protected(Path::new(".gitignore")));
        assert!(is_protected(Path::new(".bashrc")));
        assert!(is_protected(Path::new(".zshrc")));
        assert!(is_protected(Path::new(".env")));
        assert!(is_protected(Path::new("Cargo.lock")));
        assert!(is_protected(Path::new("package-lock.json")));
        assert!(is_protected(Path::new("yarn.lock")));
        assert!(is_protected(Path::new(".DS_Store")));
        assert!(is_protected(Path::new("Thumbs.db")));
    }

    #[test]
    fn test_protected_env_patterns() {
        assert!(is_protected(Path::new(".env.local")));
        assert!(is_protected(Path::new(".env.production")));
        assert!(is_protected(Path::new("config/.env")));
        assert!(is_protected(Path::new("src/.env.test")));
    }

    #[test]
    fn test_protected_git_patterns() {
        assert!(is_protected(Path::new(".git/config")));
        assert!(is_protected(Path::new(".git/HEAD")));
        assert!(is_protected(Path::new("project/.git/config")));
    }

    #[test]
    fn test_protected_secrets_patterns() {
        assert!(is_protected(Path::new("secrets.yaml")));
        assert!(is_protected(Path::new("secrets.json")));
        assert!(is_protected(Path::new("config/secrets.yml")));
    }

    #[test]
    fn test_non_protected_files() {
        assert!(!is_protected(Path::new("src/main.rs")));
        assert!(!is_protected(Path::new("Cargo.toml")));
        assert!(!is_protected(Path::new("README.md")));
        assert!(!is_protected(Path::new("src/lib.rs")));
        assert!(!is_protected(Path::new("package.json")));
        assert!(!is_protected(Path::new("config.json")));
    }

    #[test]
    fn test_protected_warning_message() {
        let warning = protected_file_warning(Path::new(".gitconfig"));
        assert!(warning.contains(".gitconfig"));
        assert!(warning.contains("protected file"));
    }
}
