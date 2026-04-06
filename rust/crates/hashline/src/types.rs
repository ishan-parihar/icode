use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hashline {
    pub line_number: usize,
    pub hash_code: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashlineEdit {
    pub file_path: String,
    pub edits: Vec<HashlineEditOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashlineEditOp {
    pub line_number: usize,
    pub hash_code: String,
    pub new_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    FileNotFound(String),
    HashMismatch {
        line: usize,
        expected: String,
        actual: String,
    },
    LineNotFound {
        line: usize,
        total_lines: usize,
    },
    InvalidHashCode(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::FileNotFound(path) => write!(f, "File not found: {path}"),
            EditError::HashMismatch {
                line,
                expected,
                actual,
            } => write!(
                f,
                "Hash mismatch at line {line}: expected {expected}, got {actual}"
            ),
            EditError::LineNotFound { line, total_lines } => {
                write!(f, "Line {line} not found (file has {total_lines} lines)")
            }
            EditError::InvalidHashCode(code) => write!(f, "Invalid hash code: {code}"),
        }
    }
}
