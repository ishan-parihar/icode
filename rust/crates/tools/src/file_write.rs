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
    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workspace root: {e}"))?;
    let path = cwd.join(&input.path);

    // Always write to the canonical path to prevent symlink-swap attacks.
    let canonical_path = if path.exists() {
        path.canonicalize()
            .map_err(|e| format!("Failed to resolve path: {e}"))?
    } else if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| format!("Failed to resolve parent: {e}"))?;
        if !canonical_parent.starts_with(&canonical_cwd) {
            return Err(format!("Path '{}' escapes workspace boundary", input.path));
        }
        canonical_parent.join(
            path.file_name()
                .ok_or_else(|| format!("Path has no filename: {}", input.path))?,
        )
    } else {
        return Err(format!("Path has no filename: {}", input.path));
    };

    if !canonical_path.starts_with(&canonical_cwd) {
        return Err(format!("Path '{}' escapes workspace boundary", input.path));
    }

    // Read existing content before overwrite (to return in JSON response).
    let (write_type, original_file) = if canonical_path.exists() {
        let original = fs::read_to_string(&canonical_path).unwrap_or_default();
        ("update", original)
    } else {
        ("create", String::new())
    };

    fs::write(&canonical_path, &input.content).map_err(|e| format!("Failed to write file: {e}"))?;

    let response = serde_json::json!({
        "type": write_type,
        "path": input.path,
        "bytes": input.content.len(),
        "originalFile": original_file,
    });
    serde_json::to_string(&response).map_err(|e| format!("Failed to serialize response: {e}"))
}
