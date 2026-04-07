//! BatchEdit tool — atomic multi-file edits in a single tool call.
//!
//! Validates all edits upfront, then applies them in-memory before writing
//! back. If any write fails, reports partial success/failure per file.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Input for the batch_edit tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchEditInput {
    pub edits: Vec<FileEdit>,
    pub validate_read_first: Option<bool>,
}

/// A single file edit within a batch.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileEdit {
    pub path: String,
    pub old_string: String,
    pub new_string: String,
    pub replace_all: Option<bool>,
}

/// Output from the batch_edit tool.
#[derive(Debug, Serialize)]
pub struct BatchEditOutput {
    pub success: bool,
    pub edits_applied: usize,
    pub edits_failed: usize,
    pub results: Vec<EditResult>,
}

/// Per-file result of an edit attempt.
#[derive(Debug, Serialize)]
pub struct EditResult {
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
    pub lines_changed: Option<isize>,
}

/// Validate all edits without applying them.
///
/// Returns `Vec<String>` of validation errors (one per failed edit),
/// or an empty `Vec` if all edits are valid.
fn validate_edits(edits: &[FileEdit], validate_read_first: bool) -> Vec<(usize, String)> {
    let mut errors = Vec::new();

    for (i, edit) in edits.iter().enumerate() {
        if edit.old_string.is_empty() {
            errors.push((i, String::from("old_string must not be empty")));
            continue;
        }

        let content = match std::fs::read_to_string(&edit.path) {
            Ok(c) => c,
            Err(e) => {
                if validate_read_first {
                    errors.push((i, format!("cannot read file '{}': {e}", edit.path)));
                }
                continue;
            }
        };

        let occurrences = content.matches(&edit.old_string).count();
        if occurrences == 0 {
            errors.push((i, format!("old_string not found in '{}'", edit.path)));
        }
    }

    errors
}

/// Count the difference in newline count between two strings.
fn count_line_diff(old_content: &str, new_content: &str) -> isize {
    let old_lines = old_content.matches('\n').count() as isize;
    let new_lines = new_content.matches('\n').count() as isize;
    new_lines - old_lines
}

/// Execute a batch of file edits atomically.
///
/// Validation phase checks all edits first. If `validate_read_first` is true
/// (default) and any validation fails, the entire batch is rejected without
/// modifying any files.
///
/// Application phase reads each file, applies the edit in memory, then writes
/// it back. If a write fails mid-batch, previously written files remain changed
/// and the output reports which succeeded vs failed.
pub fn execute_batch_edit(input: BatchEditInput) -> Result<BatchEditOutput, String> {
    let validate_read_first = input.validate_read_first.unwrap_or(true);

    if input.edits.is_empty() {
        return Err(String::from("edits must not be empty"));
    }

    // Phase 1: Validate all edits
    let validation_errors = validate_edits(&input.edits, validate_read_first);

    if validate_read_first && !validation_errors.is_empty() {
        let error_details: Vec<String> = validation_errors
            .into_iter()
            .map(|(idx, msg)| format!("edit {idx}: {msg}"))
            .collect();
        return Err(format!(
            "batch rejected due to validation errors:\n- {}",
            error_details.join("\n- ")
        ));
    }

    // Phase 2: Apply edits and collect results
    let mut results = Vec::with_capacity(input.edits.len());
    let mut edits_applied = 0usize;
    let mut edits_failed = 0usize;

    for edit in &input.edits {
        let result = apply_single_edit(edit);
        if result.success {
            edits_applied += 1;
        } else {
            edits_failed += 1;
        }
        results.push(result);
    }

    Ok(BatchEditOutput {
        success: edits_failed == 0,
        edits_applied,
        edits_failed,
        results,
    })
}

/// Apply a single file edit. Returns an `EditResult` with status.
fn apply_single_edit(edit: &FileEdit) -> EditResult {
    let path = edit.path.clone();
    let replace_all = edit.replace_all.unwrap_or(false);

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return EditResult {
                path,
                success: false,
                error: Some(format!("cannot read file: {e}")),
                lines_changed: None,
            };
        }
    };

    let occurrences = content.matches(&edit.old_string).count();
    if occurrences == 0 {
        return EditResult {
            path,
            success: false,
            error: Some(String::from("old_string not found in file")),
            lines_changed: None,
        };
    }

    let new_content = if replace_all {
        content.replace(&edit.old_string, &edit.new_string)
    } else {
        content.replacen(&edit.old_string, &edit.new_string, 1)
    };

    let lines_changed = count_line_diff(&content, &new_content);

    if let Err(e) = std::fs::write(&path, &new_content) {
        return EditResult {
            path,
            success: false,
            error: Some(format!("cannot write file: {e}")),
            lines_changed: None,
        };
    }

    EditResult {
        path,
        success: true,
        error: None,
        lines_changed: Some(lines_changed),
    }
}

/// Return the JSON Schema tool spec for the batch_edit tool.
#[must_use]
pub fn batch_edit_tool_spec() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(BatchEditInput)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn make_tmp_dir(test_name: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("batch_edit_{test_name}_{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir.to_string_lossy().to_string()
    }

    /// Clean up the temporary directory.
    fn cleanup_tmp_dir(dir: &str) {
        let _ = fs::remove_dir_all(dir);
    }

    /// Create a test file with the given content and return its full path.
    fn create_file(dir: &str, name: &str, content: &str) -> String {
        let path = Path::new(dir).join(name);
        fs::write(&path, content).expect("failed to write test file");
        path.to_string_lossy().to_string()
    }

    #[test]
    fn successful_multi_edit() {
        let dir = make_tmp_dir("multi_edit");
        let file_a = create_file(&dir, "a.txt", "hello world\nfoo bar\n");
        let file_b = create_file(&dir, "b.txt", "hello world\nbaz qux\n");

        let input = BatchEditInput {
            edits: vec![
                FileEdit {
                    path: file_a.clone(),
                    old_string: String::from("hello world"),
                    new_string: String::from("HELLO WORLD"),
                    replace_all: None,
                },
                FileEdit {
                    path: file_b.clone(),
                    old_string: String::from("hello world"),
                    new_string: String::from("HELLO WORLD"),
                    replace_all: None,
                },
            ],
            validate_read_first: Some(true),
        };

        let output = execute_batch_edit(input).expect("batch edit should succeed");

        assert!(output.success);
        assert_eq!(output.edits_applied, 2);
        assert_eq!(output.edits_failed, 0);
        assert_eq!(output.results.len(), 2);

        for result in &output.results {
            assert!(result.success);
            assert!(result.error.is_none());
            assert!(result.lines_changed.is_some());
        }

        // Verify file contents
        assert_eq!(
            fs::read_to_string(&file_a).expect("read file_a"),
            "HELLO WORLD\nfoo bar\n"
        );
        assert_eq!(
            fs::read_to_string(&file_b).expect("read file_b"),
            "HELLO WORLD\nbaz qux\n"
        );

        cleanup_tmp_dir(&dir);
    }

    #[test]
    fn atomic_rejection_on_missing_old_string() {
        let dir = make_tmp_dir("atomic_rejection");
        let file = create_file(&dir, "test.txt", "hello world\n");

        let input = BatchEditInput {
            edits: vec![
                FileEdit {
                    path: file.clone(),
                    old_string: String::from("hello world"),
                    new_string: String::from("HELLO WORLD"),
                    replace_all: None,
                },
                FileEdit {
                    path: file.clone(),
                    old_string: String::from("this does not exist"),
                    new_string: String::from("replacement"),
                    replace_all: None,
                },
            ],
            validate_read_first: Some(true),
        };

        let result = execute_batch_edit(input);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.contains("batch rejected"));
        assert!(err.contains("old_string not found"));

        // File should remain unchanged
        assert_eq!(
            fs::read_to_string(&file).expect("read file"),
            "hello world\n"
        );

        cleanup_tmp_dir(&dir);
    }

    #[test]
    fn missing_file_handling() {
        let dir = make_tmp_dir("missing_file");

        let input = BatchEditInput {
            edits: vec![FileEdit {
                path: format!("{dir}/nonexistent.txt"),
                old_string: String::from("something"),
                new_string: String::from("else"),
                replace_all: None,
            }],
            validate_read_first: Some(true),
        };

        let result = execute_batch_edit(input);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.contains("cannot read file"));
        assert!(err.contains("nonexistent.txt"));

        cleanup_tmp_dir(&dir);
    }

    #[test]
    fn replace_all_edits_all_occurrences() {
        let dir = make_tmp_dir("replace_all");
        let file = create_file(&dir, "multi.txt", "foo\nfoo\nfoo\n");

        let input = BatchEditInput {
            edits: vec![FileEdit {
                path: file.clone(),
                old_string: String::from("foo"),
                new_string: String::from("bar"),
                replace_all: Some(true),
            }],
            validate_read_first: Some(true),
        };

        let output = execute_batch_edit(input).expect("batch edit should succeed");

        assert!(output.success);
        assert_eq!(output.results[0].lines_changed, Some(0));

        assert_eq!(
            fs::read_to_string(&file).expect("read file"),
            "bar\nbar\nbar\n"
        );

        cleanup_tmp_dir(&dir);
    }

    #[test]
    fn empty_edits_list_returns_error() {
        let input = BatchEditInput {
            edits: vec![],
            validate_read_first: None,
        };

        let result = execute_batch_edit(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "edits must not be empty");
    }

    #[test]
    fn validate_read_first_false_skips_validation() {
        let dir = make_tmp_dir("skip_validation");
        let file = create_file(&dir, "skip.txt", "hello world\n");

        // Even with a missing old_string, validation is skipped
        let input = BatchEditInput {
            edits: vec![FileEdit {
                path: file.clone(),
                old_string: String::from("this does not exist"),
                new_string: String::from("replacement"),
                replace_all: None,
            }],
            validate_read_first: Some(false),
        };

        let output = execute_batch_edit(input).expect("should not reject during validation");
        // The edit itself fails during application, but the batch is not rejected upfront
        assert!(!output.success);
        assert_eq!(output.edits_failed, 1);
        assert!(output.results[0].error.is_some());

        cleanup_tmp_dir(&dir);
    }

    #[test]
    fn lines_changed_reflects_newline_difference() {
        let dir = make_tmp_dir("lines_changed");
        // Single line -> multi-line replacement
        let file = create_file(&dir, "lines.txt", "one line\n");

        let input = BatchEditInput {
            edits: vec![FileEdit {
                path: file.clone(),
                old_string: String::from("one line"),
                new_string: String::from("line one\nline two\nline three"),
                replace_all: None,
            }],
            validate_read_first: Some(true),
        };

        let output = execute_batch_edit(input).expect("batch edit should succeed");

        // old: 1 newline, new: 3 newlines => +2
        assert_eq!(output.results[0].lines_changed, Some(2));

        cleanup_tmp_dir(&dir);
    }

    #[test]
    fn tool_spec_has_correct_structure() {
        let spec = batch_edit_tool_spec();

        assert_eq!(spec["type"], "object");
        assert!(spec["properties"]["edits"].is_object());
        assert!(spec["properties"]["validate_read_first"].is_object());

        let required = &spec["required"];
        assert!(required.is_array());
        assert!(required
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("edits"))));
    }
}
