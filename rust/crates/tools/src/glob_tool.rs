use glob::Pattern;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;
use walkdir::WalkDir;

use runtime::PermissionMode;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GlobToolInput {
    /// Glob pattern to match (e.g., "**/*.rs", "src/**/*.ts")
    pub pattern: String,
    /// Directory to search in (default: cwd)
    #[serde(default)]
    pub path: Option<String>,
}

pub fn create_glob_tool() -> (String, &'static str, serde_json::Value, PermissionMode) {
    (
        "GlobTool".to_string(),
        "Find files matching a glob pattern. Supports ** for recursive matching.",
        serde_json::to_value(schemars::schema_for!(GlobToolInput)).unwrap(),
        PermissionMode::ReadOnly,
    )
}

pub fn execute_glob_tool(input: GlobToolInput, cwd: &PathBuf) -> Result<String, String> {
    let search_dir = input
        .path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.clone());
    if !search_dir.exists() {
        return Err(format!(
            "Search directory not found: {}",
            search_dir.display()
        ));
    }

    let pattern = Pattern::new(&input.pattern).map_err(|e| format!("Invalid glob pattern: {e}"))?;

    let mut matches: Vec<String> = Vec::new();
    for entry in WalkDir::new(&search_dir).into_iter().filter_map(|e| e.ok()) {
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
        output.push_str(&format!("\n\n... and {} more files", count - 100));
    }

    Ok(output)
}
