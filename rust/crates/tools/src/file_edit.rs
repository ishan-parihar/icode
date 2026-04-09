use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::fs;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EditFileInput {
    pub path: String,
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: bool,
}

pub fn edit_file_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(EditFileInput)).unwrap()
}

pub fn execute_edit_file(input: &EditFileInput) -> Result<String, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?;
    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workspace root: {e}"))?;
    let path = cwd.join(&input.path);
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {e}"))?;
    if !canonical_path.starts_with(&canonical_cwd) {
        return Err(format!("Path '{}' escapes workspace boundary", input.path));
    }
    let content =
        fs::read_to_string(&canonical_path).map_err(|e| format!("Failed to read file: {e}"))?;
    if !content.contains(&input.old_string) {
        return Err(format!("String not found in {}", input.path));
    }
    let new_content = if input.replace_all {
        content.replace(&input.old_string, &input.new_string)
    } else {
        content.replacen(&input.old_string, &input.new_string, 1)
    };
    fs::write(&path, &new_content).map_err(|e| format!("Failed to write file: {e}"))?;
    let replacements = if input.replace_all { "all" } else { "first" };
    Ok(format!(
        "Edited {} (replaced {} occurrence of old_string)",
        input.path, replacements
    ))
}
