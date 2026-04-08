use glob::Pattern;
use regex::RegexBuilder;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GrepToolInput {
    /// Regex pattern to search for
    pub pattern: String,
    /// File glob to restrict search to (e.g., "*.rs")
    #[serde(default)]
    pub glob: Option<String>,
    /// Case insensitive search
    #[serde(default)]
    pub case_insensitive: bool,
}

pub fn execute_grep_tool(input: &GrepToolInput, cwd: &PathBuf) -> Result<String, String> {
    let re = if input.case_insensitive {
        RegexBuilder::new(&input.pattern)
            .case_insensitive(true)
            .build()
            .map_err(|e| format!("Invalid regex: {e}"))?
    } else {
        regex::Regex::new(&input.pattern).map_err(|e| format!("Invalid regex: {e}"))?
    };

    let glob_pat = input
        .glob
        .as_ref()
        .map(|g| Pattern::new(g))
        .transpose()
        .map_err(|e| format!("Invalid glob pattern: {e}"))?;

    let mut results: Vec<String> = Vec::new();
    for entry in WalkDir::new(cwd)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(ref gp) = glob_pat {
            let filename = path.file_name().unwrap_or_default().to_string_lossy();
            if !gp.matches(&filename) {
                continue;
            }
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for (line_num, line) in content.lines().enumerate() {
            if re.is_match(line) {
                let rel_path = path.strip_prefix(cwd).unwrap_or(path);
                results.push(format!("{}:{}:{}", rel_path.display(), line_num + 1, line));
            }
        }
    }

    let count = results.len();
    if count == 0 {
        return Ok(format!("No matches found for '{}'", input.pattern));
    }

    let output = results
        .iter()
        .take(100)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let mut text = format!(
        "Found {} matches for '{}':\n\n{}",
        count, input.pattern, output
    );
    if count > 100 {
        let _ = write!(text, "\n\n... and {} more matches", count - 100);
    }

    Ok(text)
}
