use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const MAX_MEMORY_FILES: usize = 200;
pub const FRONTMATTER_MAX_LINES: usize = 30;
pub const MEMORY_ENTRYPOINT: &str = "MEMORY.md";
pub const MAX_ENTRYPOINT_LINES: usize = 200;
pub const MAX_ENTRYPOINT_BYTES: usize = 25_000;

// ---------------------------------------------------------------------------
// MemoryType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryType::User => write!(f, "User"),
            MemoryType::Feedback => write!(f, "Feedback"),
            MemoryType::Project => write!(f, "Project"),
            MemoryType::Reference => write!(f, "Reference"),
        }
    }
}

impl FromStr for MemoryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "User" => Ok(MemoryType::User),
            "Feedback" => Ok(MemoryType::Feedback),
            "Project" => Ok(MemoryType::Project),
            "Reference" => Ok(MemoryType::Reference),
            other => Err(format!("unknown MemoryType: {other}")),
        }
    }
}

impl Default for MemoryType {
    fn default() -> Self {
        MemoryType::Project
    }
}

// ---------------------------------------------------------------------------
// MemoryFileMeta
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MemoryFileMeta {
    pub name: String,
    pub description: String,
    pub file_type: MemoryType,
    pub path: PathBuf,
    pub modified: SystemTime,
}

// ---------------------------------------------------------------------------
// Sanitization helper
// ---------------------------------------------------------------------------

fn sanitize_project_name(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Environment checks
// ---------------------------------------------------------------------------

pub fn is_auto_memory_enabled() -> bool {
    if let Ok(val) = std::env::var("ICODE_DISABLE_AUTO_MEMORY") {
        let lower = val.to_lowercase();
        if lower == "1" || lower == "true" || lower == "yes" {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

pub fn auto_memory_path() -> Option<PathBuf> {
    // 1. Override
    if let Ok(val) = std::env::var("ICODE_MEMORY_PATH_OVERRIDE") {
        if !val.is_empty() {
            return Some(PathBuf::from(val));
        }
    }

    // 2. CODEBASE_INTELLIGENCE_ROOT
    if let Ok(root) = std::env::var("CODEBASE_INTELLIGENCE_ROOT") {
        if !root.is_empty() {
            let sanitized = sanitize_project_name(&root);
            return Some(build_memory_path(&sanitized));
        }
    }

    // 3. PWD → git root
    if let Ok(pwd) = std::env::var("PWD") {
        if let Some(git_root) = find_git_root(Path::new(&pwd)) {
            let sanitized = sanitize_project_name(&git_root.to_string_lossy());
            return Some(build_memory_path(&sanitized));
        }
    }

    None
}

fn build_memory_path(sanitized_name: &str) -> PathBuf {
    let mut path = home_dir();
    path.push("icode");
    path.push("projects");
    path.push(sanitized_name);
    path.push("memory");
    path.push(MEMORY_ENTRYPOINT);
    path
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.canonicalize().ok()?;
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

pub fn parse_frontmatter_quick(content: &str) -> Option<(String, String, MemoryType)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }

    let mut end_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if i >= FRONTMATTER_MAX_LINES {
            break;
        }
        if line.trim() == "---" {
            end_idx = Some(i);
            break;
        }
    }

    let end = end_idx?;
    let yaml_lines = &lines[1..end];

    let mut name = String::new();
    let mut description = String::new();
    let mut mem_type: Option<MemoryType> = None;

    for line in yaml_lines {
        if let Some((key, value)) = line.split_once(':') {
            let k = key.trim();
            let v = value.trim();
            match k {
                "name" => name = v.to_string(),
                "description" => description = v.to_string(),
                "type" => {
                    if let Ok(t) = MemoryType::from_str(v) {
                        mem_type = Some(t);
                    }
                }
                _ => {}
            }
        }
    }

    if name.is_empty() && description.is_empty() && mem_type.is_none() {
        return None;
    }

    Some((name, description, mem_type.unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Directory scanning
// ---------------------------------------------------------------------------

pub fn scan_memory_dir(dir: &Path) -> Result<Vec<MemoryFileMeta>, String> {
    if !dir.is_dir() {
        return Err(format!("not a directory: {}", dir.display()));
    }

    let mut metas: Vec<MemoryFileMeta> = Vec::new();
    walk_md_files(dir, &mut metas).map_err(|e| format!("failed to scan directory: {e}"))?;

    // Sort by modification time, newest first
    metas.sort_by(|a, b| b.modified.cmp(&a.modified));

    // Cap at MAX_MEMORY_FILES
    if metas.len() > MAX_MEMORY_FILES {
        metas.truncate(MAX_MEMORY_FILES);
    }

    Ok(metas)
}

fn walk_md_files(dir: &Path, metas: &mut Vec<MemoryFileMeta>) -> std::io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk_md_files(&path, metas)?;
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" {
                    // Skip MEMORY.md
                    if let Some(file_name) = path.file_name() {
                        if file_name == MEMORY_ENTRYPOINT {
                            continue;
                        }
                    }

                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .unwrap_or(SystemTime::UNIX_EPOCH);

                    let content = fs::read_to_string(&path).unwrap_or_default();
                    let (name, description, file_type) = parse_frontmatter_quick(&content)
                        .unwrap_or_else(|| {
                            (
                                path.file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                                String::new(),
                                MemoryType::default(),
                            )
                        });

                    metas.push(MemoryFileMeta {
                        name,
                        description,
                        file_type,
                        path,
                        modified,
                    });
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Prompt building
// ---------------------------------------------------------------------------

pub fn build_memory_prompt_content(memory_dir: &Path) -> Result<String, String> {
    let entrypoint = memory_dir.join(MEMORY_ENTRYPOINT);

    if !entrypoint.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(&entrypoint)
        .map_err(|e| format!("failed to read {}: {e}", entrypoint.display()))?;

    let mut result = String::from("## Memory Index\n");

    let mut total_bytes = result.len();
    let mut line_count = 0;

    for line in content.lines() {
        if line_count >= MAX_ENTRYPOINT_LINES {
            break;
        }
        let line_with_newline = format!("{line}\n");
        if total_bytes + line_with_newline.len() > MAX_ENTRYPOINT_BYTES {
            break;
        }
        result.push_str(&line_with_newline);
        total_bytes += line_with_newline.len();
        line_count += 1;
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Age / freshness helpers
// ---------------------------------------------------------------------------

pub fn memory_age_days(modified: SystemTime) -> u64 {
    let now = SystemTime::now();
    let duration = now.duration_since(modified).unwrap_or(Duration::ZERO);
    duration.as_secs() / 86_400
}

pub fn memory_freshness_note(modified: SystemTime) -> String {
    let days = memory_age_days(modified);
    if days > 1 {
        format!(
            "<system-reminder>This memory is {days} days old. Memories are point-in-time observations, not live state \
             — claims about code behavior or file:line citations may be outdated. Verify against current code before \
             asserting as fact.</system-reminder>"
        )
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_frontmatter_valid() {
        let content = "---\nname: Test Memory\ndescription: A test memory file\ntype: User\n---\nSome content";
        let result = parse_frontmatter_quick(content);
        assert!(result.is_some());
        let (name, description, mem_type) = result.unwrap();
        assert_eq!(name, "Test Memory");
        assert_eq!(description, "A test memory file");
        assert_eq!(mem_type, MemoryType::User);
    }

    #[test]
    fn parse_frontmatter_missing_type() {
        let content = "---\nname: No Type\ndescription: Missing type field\n---\nContent";
        let result = parse_frontmatter_quick(content);
        assert!(result.is_some());
        let (name, description, mem_type) = result.unwrap();
        assert_eq!(name, "No Type");
        assert_eq!(description, "Missing type field");
        assert_eq!(mem_type, MemoryType::Project);
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let content = "Just plain text, no frontmatter here.";
        let result = parse_frontmatter_quick(content);
        assert!(result.is_none());
    }

    #[test]
    fn memory_age_days_calculation() {
        let three_days_ago = SystemTime::now() - Duration::from_secs(3 * 86_400);
        let age = memory_age_days(three_days_ago);
        assert_eq!(age, 3);
    }

    #[test]
    fn freshness_note_old() {
        let five_days_ago = SystemTime::now() - Duration::from_secs(5 * 86_400);
        let note = memory_freshness_note(five_days_ago);
        assert!(note.contains("<system-reminder>"));
        assert!(note.contains("5 days old"));
        assert!(note.contains("Verify against current code"));
    }

    #[test]
    fn freshness_note_fresh() {
        let now = SystemTime::now();
        let note = memory_freshness_note(now);
        assert!(note.is_empty());
    }

    #[test]
    fn sanitization_strips_special_chars() {
        let input = "my/project (v2)";
        let sanitized = sanitize_project_name(input);
        assert_eq!(sanitized, "my_project__v2_");
    }

    #[test]
    fn auto_memory_enabled_by_default() {
        // This test assumes no ICODE_DISABLE_AUTO_MEMORY is set in test env.
        // We can't easily unset env vars, but the default behavior should be true.
        // If the env var is set, we test the negation path separately.
        let enabled = is_auto_memory_enabled();
        // In a clean test environment this should be true.
        // If a developer has ICODE_DISABLE_AUTO_MEMORY set, this test documents
        // the current state rather than enforcing absolute behavior.
        assert!(
            enabled || std::env::var("ICODE_DISABLE_AUTO_MEMORY").is_ok(),
            "should be enabled unless env var is set"
        );
    }

    #[test]
    fn memory_prompt_content_missing_file_returns_empty() {
        let temp_dir = std::env::temp_dir().join("memdir_test_missing");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let result = build_memory_prompt_content(&temp_dir).unwrap();
        assert_eq!(result, "");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn build_memory_prompt_content_truncates_large_file() {
        let temp_dir = std::env::temp_dir().join("memdir_test_truncate");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create MEMORY.md with 300 lines
        let mut file = fs::File::create(temp_dir.join(MEMORY_ENTRYPOINT)).unwrap();
        for i in 0..300 {
            writeln!(file, "Line {i}: This is test content for line number {i}").unwrap();
        }
        drop(file);

        let result = build_memory_prompt_content(&temp_dir).unwrap();

        // Should start with "## Memory Index\n"
        assert!(result.starts_with("## Memory Index\n"));

        // Count lines after the header
        let content_lines = result.lines().count();
        // Header is line 1, then up to MAX_ENTRYPOINT_LINES of content
        assert!(
            content_lines <= MAX_ENTRYPOINT_LINES + 1,
            "expected <= {} lines, got {}",
            MAX_ENTRYPOINT_LINES + 1,
            content_lines
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
