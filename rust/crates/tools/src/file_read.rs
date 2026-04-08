use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::fs;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadFileInput {
    pub path: String,
    #[serde(default)]
    pub max_lines: usize,
    #[serde(default = "default_start_line")]
    pub start_line: usize,
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
    let path = cwd.join(&input.path);
    if !path.exists() {
        return Err(format!("File not found: {}", input.path));
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();
    let start = input.start_line.saturating_sub(1);
    let end = if input.max_lines > 0 {
        (start + input.max_lines).min(lines.len())
    } else {
        lines.len()
    };
    let snippet = lines[start..end].join("\n");
    let total_lines = lines.len();
    Ok(format!(
        "Read {} (lines {}-{} of {}):\n\n{}",
        input.path, input.start_line, end, total_lines, snippet
    ))
}
