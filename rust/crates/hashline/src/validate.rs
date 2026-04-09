use std::fs;

use crate::enhance::enhance_with_hashlines;
use crate::types::{EditError, Hashline, HashlineEditOp};

/// Validates a single edit operation against a file.
pub fn validate_edit(file_path: &str, edit: &HashlineEditOp) -> Result<(), EditError> {
    let content = fs::read_to_string(file_path)
        .map_err(|_| EditError::FileNotFound(file_path.to_string()))?;
    validate_edit_with_content(&content, edit)
}

/// Validates a single edit against in-memory content.
pub fn validate_edit_with_content(content: &str, edit: &HashlineEditOp) -> Result<(), EditError> {
    let hashlines = enhance_with_hashlines(content);

    if edit.line_number == 0 || edit.line_number > hashlines.len() {
        return Err(EditError::LineNotFound {
            line: edit.line_number,
            total_lines: hashlines.len(),
        });
    }

    let expected_hashline = &hashlines[edit.line_number - 1];

    if expected_hashline.hash_code != edit.hash_code {
        return Err(EditError::HashMismatch {
            line: edit.line_number,
            expected: edit.hash_code.clone(),
            actual: expected_hashline.hash_code.clone(),
        });
    }

    Ok(())
}

/// Validates all edit operations in sequence against a file.
pub fn validate_all(file_path: &str, edits: &[HashlineEditOp]) -> Result<(), EditError> {
    let content = fs::read_to_string(file_path)
        .map_err(|_| EditError::FileNotFound(file_path.to_string()))?;
    for edit in edits {
        validate_edit_with_content(&content, edit)?;
    }
    Ok(())
}

/// Validates all edit operations against in-memory content.
pub fn validate_all_with_content(content: &str, edits: &[HashlineEditOp]) -> Result<(), EditError> {
    for edit in edits {
        validate_edit_with_content(content, edit)?;
    }
    Ok(())
}

/// Reads a file and returns its hashlines.
pub fn get_current_hashlines(file_path: &str) -> Result<Vec<Hashline>, EditError> {
    let content = fs::read_to_string(file_path)
        .map_err(|_| EditError::FileNotFound(file_path.to_string()))?;

    Ok(enhance_with_hashlines(&content))
}

/// Validates that a hash code is a proper 2-char uppercase alphanumeric code.
pub fn validate_hash_code(code: &str) -> Result<(), EditError> {
    if code.len() != 2
        || !code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(EditError::InvalidHashCode(code.to_string()));
    }
    Ok(())
}
