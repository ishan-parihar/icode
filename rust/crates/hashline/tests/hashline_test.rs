use hashline::apply::{apply_edit, apply_edit_to_content, apply_edits};
use hashline::enhance::{compute_hash_code, enhance_with_hashlines, format_hashlines};
use hashline::types::{EditError, Hashline, HashlineEdit, HashlineEditOp};
use hashline::validate::{get_current_hashlines, validate_all, validate_edit};
use std::fs;

fn unique_temp_path() -> String {
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{}/hashline_test_{pid}_{ts}.txt",
        std::env::temp_dir().display()
    )
}

// --- compute_hash_code tests ---

#[test]
fn hash_code_determinism() {
    let a = compute_hash_code("hello");
    let b = compute_hash_code("hello");
    assert_eq!(a, b);
}

#[test]
fn hash_code_different_inputs() {
    let a = compute_hash_code("hello");
    let b = compute_hash_code("world");
    assert_ne!(a, b);
}

#[test]
fn hash_code_length() {
    let code = compute_hash_code("any content");
    assert_eq!(code.len(), 2);
}

#[test]
fn hash_code_uppercase_hex() {
    let code = compute_hash_code("test line");
    assert!(code
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
}

#[test]
fn hash_code_empty_string() {
    let code = compute_hash_code("");
    assert_eq!(code.len(), 2);
}

#[test]
fn hash_code_unicode() {
    let code = compute_hash_code("h\u{00e9}llo w\u{00f6}rld");
    assert_eq!(code.len(), 2);
}

#[test]
fn hash_code_very_long_line() {
    let long = "a".repeat(10000);
    let code = compute_hash_code(&long);
    assert_eq!(code.len(), 2);
}

// --- enhance_with_hashlines tests ---

#[test]
fn enhance_empty_content() {
    let result = enhance_with_hashlines("");
    assert!(result.is_empty());
}

#[test]
fn enhance_single_line() {
    let result = enhance_with_hashlines("hello");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].line_number, 1);
    assert_eq!(result[0].content, "hello");
    assert_eq!(result[0].hash_code, compute_hash_code("hello"));
}

#[test]
fn enhance_multi_line() {
    let content = "line one\nline two\nline three";
    let result = enhance_with_hashlines(content);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].line_number, 1);
    assert_eq!(result[1].line_number, 2);
    assert_eq!(result[2].line_number, 3);
}

#[test]
fn enhance_blank_lines() {
    let content = "first\n\nthird";
    let result = enhance_with_hashlines(content);
    assert_eq!(result.len(), 3);
    assert_eq!(result[1].content, "");
}

// --- format_hashlines tests ---

#[test]
fn format_empty() {
    assert!(format_hashlines("").is_empty());
}

#[test]
fn format_single_line() {
    let output = format_hashlines("hello");
    assert!(output.starts_with("1#"));
    assert!(output.contains("| hello"));
}

#[test]
fn format_multi_line() {
    let output = format_hashlines("foo\nbar");
    let lines: Vec<&str> = output.split('\n').collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("1#"));
    assert!(lines[1].starts_with("2#"));
}

// --- validate_edit tests ---

#[test]
fn validate_valid_edit() {
    let path = unique_temp_path();
    fs::write(&path, "hello\nworld").unwrap();
    let hash = compute_hash_code("hello");
    let edit = HashlineEditOp {
        line_number: 1,
        hash_code: hash,
        new_content: "HELLO".to_string(),
    };
    assert!(validate_edit(&path, &edit).is_ok());
    let _ = fs::remove_file(&path);
}

#[test]
fn validate_wrong_hash() {
    let path = unique_temp_path();
    fs::write(&path, "hello").unwrap();
    let edit = HashlineEditOp {
        line_number: 1,
        hash_code: "XX".to_string(),
        new_content: "nope".to_string(),
    };
    assert!(validate_edit(&path, &edit).is_err());
    let _ = fs::remove_file(&path);
}

#[test]
fn validate_missing_line() {
    let path = unique_temp_path();
    fs::write(&path, "hello").unwrap();
    let hash = compute_hash_code("hello");
    let edit = HashlineEditOp {
        line_number: 5,
        hash_code: hash,
        new_content: "nope".to_string(),
    };
    assert!(matches!(
        validate_edit(&path, &edit),
        Err(EditError::LineNotFound { .. })
    ));
    let _ = fs::remove_file(&path);
}

#[test]
fn validate_missing_file() {
    let edit = HashlineEditOp {
        line_number: 1,
        hash_code: "XX".to_string(),
        new_content: "nope".to_string(),
    };
    assert!(matches!(
        validate_edit("/nonexistent/path.txt", &edit),
        Err(EditError::FileNotFound(_))
    ));
}

// --- validate_all tests ---

#[test]
fn validate_all_valid() {
    let path = unique_temp_path();
    fs::write(&path, "alpha\nbeta").unwrap();
    let h1 = compute_hash_code("alpha");
    let h2 = compute_hash_code("beta");
    let edits = vec![
        HashlineEditOp {
            line_number: 1,
            hash_code: h1,
            new_content: "ALPHA".to_string(),
        },
        HashlineEditOp {
            line_number: 2,
            hash_code: h2,
            new_content: "BETA".to_string(),
        },
    ];
    assert!(validate_all(&path, &edits).is_ok());
    let _ = fs::remove_file(&path);
}

#[test]
fn validate_all_mixed() {
    let path = unique_temp_path();
    fs::write(&path, "alpha\nbeta").unwrap();
    let h1 = compute_hash_code("alpha");
    let edits = vec![
        HashlineEditOp {
            line_number: 1,
            hash_code: h1,
            new_content: "ALPHA".to_string(),
        },
        HashlineEditOp {
            line_number: 2,
            hash_code: "ZZ".to_string(),
            new_content: "BETA".to_string(),
        },
    ];
    assert!(validate_all(&path, &edits).is_err());
    let _ = fs::remove_file(&path);
}

// --- get_current_hashlines tests ---

#[test]
fn get_hashlines_roundtrip() {
    let path = unique_temp_path();
    let content = "foo\nbar\nbaz";
    fs::write(&path, content).unwrap();
    let hashlines = get_current_hashlines(&path).unwrap();
    assert_eq!(hashlines.len(), 3);
    assert_eq!(hashlines[0].content, "foo");
    assert_eq!(hashlines[1].content, "bar");
    assert_eq!(hashlines[2].content, "baz");
    let _ = fs::remove_file(&path);
}

#[test]
fn get_hashlines_missing_file() {
    assert!(matches!(
        get_current_hashlines("/no/such/file"),
        Err(EditError::FileNotFound(_))
    ));
}

// --- apply_edit_to_content tests ---

#[test]
fn apply_content_single_edit() {
    let hash = compute_hash_code("hello");
    let edit = HashlineEditOp {
        line_number: 1,
        hash_code: hash,
        new_content: "HELLO".to_string(),
    };
    let result = apply_edit_to_content("hello", &[edit]).unwrap();
    assert_eq!(result, "HELLO");
}

#[test]
fn apply_content_multiple_edits() {
    let h1 = compute_hash_code("a");
    let h2 = compute_hash_code("b");
    let edits = vec![
        HashlineEditOp {
            line_number: 1,
            hash_code: h1,
            new_content: "A".to_string(),
        },
        HashlineEditOp {
            line_number: 2,
            hash_code: h2,
            new_content: "B".to_string(),
        },
    ];
    let result = apply_edit_to_content("a\nb", &edits).unwrap();
    assert_eq!(result, "A\nB");
}

#[test]
fn apply_content_line_out_of_range() {
    let edit = HashlineEditOp {
        line_number: 10,
        hash_code: "XX".to_string(),
        new_content: "nope".to_string(),
    };
    assert!(matches!(
        apply_edit_to_content("short", &[edit]),
        Err(EditError::LineNotFound { .. })
    ));
}

// --- apply_edits filesystem roundtrip ---

#[test]
fn apply_edit_filesystem_roundtrip() {
    let path = unique_temp_path();
    fs::write(&path, "original").unwrap();
    let hash = compute_hash_code("original");
    let edit = HashlineEditOp {
        line_number: 1,
        hash_code: hash,
        new_content: "modified".to_string(),
    };
    let result = apply_edit(&path, &edit).unwrap();
    assert_eq!(result, "modified");
    let _ = fs::remove_file(&path);
}

#[test]
fn apply_edits_filesystem_multiple() {
    let path = unique_temp_path();
    fs::write(&path, "one\ntwo").unwrap();
    let h1 = compute_hash_code("one");
    let h2 = compute_hash_code("two");
    let edits = vec![
        HashlineEditOp {
            line_number: 1,
            hash_code: h1,
            new_content: "ONE".to_string(),
        },
        HashlineEditOp {
            line_number: 2,
            hash_code: h2,
            new_content: "TWO".to_string(),
        },
    ];
    let result = apply_edits(&path, &edits).unwrap();
    assert_eq!(result, "ONE\nTWO");
    let _ = fs::remove_file(&path);
}

// --- EditError Display ---

#[test]
fn error_display_file_not_found() {
    let err = EditError::FileNotFound("/tmp/x.txt".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("/tmp/x.txt"));
}

#[test]
fn error_display_hash_mismatch() {
    let err = EditError::HashMismatch {
        line: 3,
        expected: "AB".to_string(),
        actual: "CD".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("3"));
    assert!(msg.contains("AB"));
    assert!(msg.contains("CD"));
}

#[test]
fn error_display_line_not_found() {
    let err = EditError::LineNotFound {
        line: 5,
        total_lines: 3,
    };
    let msg = format!("{err}");
    assert!(msg.contains("5"));
    assert!(msg.contains("3"));
}

#[test]
fn error_display_invalid_hash() {
    let err = EditError::InvalidHashCode("abc".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("abc"));
}

// --- EditError Debug ---

#[test]
fn error_debug() {
    let err = EditError::FileNotFound("x".to_string());
    let debug = format!("{err:?}");
    assert!(debug.contains("FileNotFound"));
}

// --- Serialization ---

#[test]
fn serialize_edit_op() {
    let op = HashlineEditOp {
        line_number: 1,
        hash_code: "AB".to_string(),
        new_content: "test".to_string(),
    };
    let json = serde_json::to_string(&op).unwrap();
    let parsed: HashlineEditOp = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.line_number, 1);
    assert_eq!(parsed.hash_code, "AB");
    assert_eq!(parsed.new_content, "test");
}

#[test]
fn serialize_edit() {
    let edit = HashlineEdit {
        file_path: "/tmp/test.rs".to_string(),
        edits: vec![HashlineEditOp {
            line_number: 1,
            hash_code: "AB".to_string(),
            new_content: "hello".to_string(),
        }],
    };
    let json = serde_json::to_string(&edit).unwrap();
    let parsed: HashlineEdit = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.file_path, "/tmp/test.rs");
    assert_eq!(parsed.edits.len(), 1);
}

// --- Hashline equality ---

#[test]
fn hashline_equality() {
    let a = Hashline {
        line_number: 1,
        hash_code: "AB".to_string(),
        content: "test".to_string(),
    };
    let b = a.clone();
    assert_eq!(a, b);
}
