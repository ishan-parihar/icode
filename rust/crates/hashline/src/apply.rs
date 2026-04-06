use std::fs;

use crate::enhance::enhance_with_hashlines;
use crate::types::{EditError, HashlineEditOp};
use crate::validate::{validate_all, validate_edit};

/// Validates and applies a single edit to a file, returning new content.
pub fn apply_edit(file_path: &str, edit: &HashlineEditOp) -> Result<String, EditError> {
    validate_edit(file_path, edit)?;

    let content = fs::read_to_string(file_path)
        .map_err(|_| EditError::FileNotFound(file_path.to_string()))?;

    apply_edit_to_content(&content, std::slice::from_ref(edit))
}

/// Validates and applies multiple edits to a file, returning new content.
pub fn apply_edits(file_path: &str, edits: &[HashlineEditOp]) -> Result<String, EditError> {
    validate_all(file_path, edits)?;

    let content = fs::read_to_string(file_path)
        .map_err(|_| EditError::FileNotFound(file_path.to_string()))?;

    apply_edit_to_content(&content, edits)
}

/// Applies edits to in-memory content without filesystem access.
pub fn apply_edit_to_content(content: &str, edits: &[HashlineEditOp]) -> Result<String, EditError> {
    let hashlines = enhance_with_hashlines(content);
    let total_lines = hashlines.len();

    let mut lines: Vec<&str> = content.lines().collect();

    for edit in edits {
        if edit.line_number == 0 || edit.line_number > total_lines {
            return Err(EditError::LineNotFound {
                line: edit.line_number,
                total_lines,
            });
        }

        lines[edit.line_number - 1] = &edit.new_content;
    }

    Ok(lines.join("\n"))
}

/// Writes new content to a file after applying edits.
pub fn apply_edit_and_write(file_path: &str, edit: &HashlineEditOp) -> Result<String, EditError> {
    let new_content = apply_edit(file_path, edit)?;
    fs::write(file_path, &new_content)
        .map_err(|_| EditError::FileNotFound(file_path.to_string()))?;
    Ok(new_content)
}

/// Writes new content to a file after applying multiple edits.
pub fn apply_edits_and_write(
    file_path: &str,
    edits: &[HashlineEditOp],
) -> Result<String, EditError> {
    let new_content = apply_edits(file_path, edits)?;
    fs::write(file_path, &new_content)
        .map_err(|_| EditError::FileNotFound(file_path.to_string()))?;
    Ok(new_content)
}
