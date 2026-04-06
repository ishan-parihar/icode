use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use crate::types::Hashline;

/// Computes a deterministic 2-character uppercase hex code from line content.
pub fn compute_hash_code(line: &str) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(line.as_bytes());
    let hash = hasher.finish();
    let byte = (hash >> 56) as u8;
    let alphabet = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    format!(
        "{}{}",
        alphabet[(byte >> 4) as usize] as char,
        alphabet[(byte & 0x0F) as usize] as char
    )
}

/// Enhances file content with hashline metadata.
pub fn enhance_with_hashlines(content: &str) -> Vec<Hashline> {
    if content.is_empty() {
        return vec![];
    }

    content
        .lines()
        .enumerate()
        .map(|(idx, line)| {
            let line_number = idx + 1;
            let hash_code = compute_hash_code(line);
            Hashline {
                line_number,
                hash_code,
                content: line.to_string(),
            }
        })
        .collect()
}

/// Formats content with hashline prefixes like "11#VK| content".
pub fn format_hashlines(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }

    let hashlines = enhance_with_hashlines(content);
    hashlines
        .iter()
        .map(|hl| format!("{}#{}| {}", hl.line_number, hl.hash_code, hl.content))
        .collect::<Vec<_>>()
        .join("\n")
}
