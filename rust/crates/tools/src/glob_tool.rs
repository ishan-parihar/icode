use glob::Pattern;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GlobToolInput {
    /// Glob pattern to match (e.g., "**/*.rs", "src/**/*.ts")
    pub pattern: String,
    /// Directory to search in (default: cwd)
    #[serde(default)]
    pub path: Option<String>,
}

pub fn execute_glob_tool(input: &GlobToolInput, cwd: &Path) -> Result<String, String> {
    let search_dir = input
        .path
        .as_ref()
        .map_or_else(|| cwd.to_path_buf(), PathBuf::from);
    if !search_dir.exists() {
        return Err(format!(
            "Search directory not found: {}",
            search_dir.display()
        ));
    }

    let pattern = Pattern::new(&input.pattern).map_err(|e| format!("Invalid glob pattern: {e}"))?;

    let mut matches: Vec<String> = Vec::new();
    for entry in WalkDir::new(&search_dir)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if let Ok(relative) = path.strip_prefix(&search_dir) {
            if pattern.matches_path(relative) {
                matches.push(relative.to_string_lossy().into_owned());
            }
        }
    }

    matches.sort();

    let count = matches.len();
    if count == 0 {
        return Ok(format!("No files found matching '{}'", input.pattern));
    }

    let listing = matches
        .iter()
        .take(100)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = format!(
        "Found {} files matching '{}':\n\n{}",
        count, input.pattern, listing
    );
    if count > 100 {
        let _ = write!(output, "\n\n... and {} more files", count - 100);
    }

    Ok(output)
}
