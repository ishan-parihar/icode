use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::fs;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WriteFileInput {
    pub path: String,
    pub content: String,
}

pub fn write_file_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(WriteFileInput)).unwrap()
}

pub fn execute_write_file(input: &WriteFileInput) -> Result<String, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?;
    let path = cwd.join(&input.path);
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
        }
    }
    fs::write(&path, &input.content).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(format!(
        "Wrote {} ({} bytes)",
        input.path,
        input.content.len()
    ))
}
