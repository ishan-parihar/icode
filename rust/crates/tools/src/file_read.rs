use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::fs;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadFileInput {
    pub path: String,
    /// Maximum number of lines to return.
    #[serde(default, alias = "limit")]
    pub max_lines: usize,
    /// 1-based start line.
    #[serde(default = "default_start_line")]
    pub start_line: usize,
    /// 0-based line offset (overrides start_line when provided; start_line = offset + 1).
    #[serde(default)]
    pub offset: Option<usize>,
}

fn default_start_line() -> usize {
    1
}

pub fn read_file_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(ReadFileInput)).unwrap()
}

pub fn execute_read_file(input: &ReadFileInput) -> Result<String, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?;
    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workspace root: {e}"))?;
    // Resolve path: absolute paths used as-is, relative paths anchored to CWD.
    let path = if std::path::Path::new(&input.path).is_absolute() {
        std::path::PathBuf::from(&input.path)
    } else {
        cwd.join(&input.path)
    };
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {e}"))?;
    // For relative paths, enforce workspace boundary. Absolute paths are allowed (reads are safe).
    if !std::path::Path::new(&input.path).is_absolute()
        && !canonical_path.starts_with(&canonical_cwd)
    {
        return Err(format!("Path '{}' escapes workspace boundary", input.path));
    }
    if !canonical_path.exists() {
        return Err(format!("File not found: {}", input.path));
    }
    let content =
        fs::read_to_string(&canonical_path).map_err(|e| format!("Failed to read file: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();
    // offset (0-based) takes precedence over start_line (1-based).
    let start = input.offset.unwrap_or(input.start_line.saturating_sub(1));
    let end = if input.max_lines > 0 {
        (start + input.max_lines).min(lines.len())
    } else {
        lines.len()
    };
    // clamp start to valid range
    let start = start.min(lines.len());
    let end = end.min(lines.len());
    let snippet = lines[start..end].join("\n");
    let start_line = start + 1;

    let response = serde_json::json!({
        "file": {
            "path": input.path,
            "content": snippet,
            "startLine": start_line,
            "endLine": end,
            "totalLines": lines.len(),
        }
    });
    serde_json::to_string(&response).map_err(|e| format!("Failed to serialize response: {e}"))
}
