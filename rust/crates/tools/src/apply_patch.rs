//! `ApplyPatch` tool — applies unified diff patches to workspace files.
//!
//! Parses unified diff format (`--- a/path`, `+++ b/path`, `@@ ... @@` hunks)
//! and applies changes to target files without shelling out to `patch`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Input for the `ApplyPatch` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ApplyPatchInput {
    /// The unified diff patch content.
    pub patch: String,
    /// Number of leading path components to strip from file paths.
    pub strip: Option<u32>,
    /// If true, report what would change without writing files.
    pub dry_run: Option<bool>,
    /// Working directory for resolving relative paths.
    pub cwd: Option<String>,
}

/// Output from the `ApplyPatch` tool.
#[derive(Debug, Clone, Serialize)]
pub struct ApplyPatchOutput {
    /// Whether all hunks were applied successfully.
    pub success: bool,
    /// Paths of files that were (or would be) modified.
    pub files_modified: Vec<String>,
    /// Number of hunks successfully applied.
    pub hunks_applied: usize,
    /// Number of hunks that failed to apply.
    pub hunks_failed: usize,
    /// Human-readable summary of the operation.
    pub output: String,
}

#[derive(Debug, Clone)]
struct Hunk {
    old_start: usize,
    #[allow(dead_code)]
    old_count: usize,
    _new_start: usize,
    #[allow(dead_code)]
    new_count: usize,
    lines: Vec<HunkLine>,
}

#[derive(Debug, Clone)]
enum HunkLine {
    Context(String),
    Add(String),
    Remove(String),
    NoNewline,
}

#[derive(Debug, Clone)]
struct FileTarget {
    old_path: String,
    new_path: String,
    hunks: Vec<Hunk>,
}

/// Execute the `ApplyPatch` tool.
///
/// Parses the unified diff, applies each hunk to the target file, and returns
/// a summary of applied changes.
#[allow(clippy::too_many_lines)]
pub fn execute_apply_patch(input: &ApplyPatchInput) -> Result<ApplyPatchOutput, String> {
    let dry_run = input.dry_run.unwrap_or(false);
    let strip = input.strip.unwrap_or(1);

    let file_targets = parse_patch(&input.patch)?;

    if file_targets.is_empty() {
        return Err("No file targets found in patch".to_string());
    }

    let mut files_modified: Vec<String> = Vec::new();
    let mut hunks_applied: usize = 0;
    let mut hunks_failed: usize = 0;
    let mut details: Vec<String> = Vec::new();
    let mut all_success = true;

    let base_dir = match &input.cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => std::env::current_dir()
            .map_err(|e| format!("Cannot determine current directory: {e}"))?,
    };

    for target in &file_targets {
        let resolved_path = resolve_path(&target.new_path, strip, &base_dir)?;

        if let Ok(canonical) = resolved_path.canonicalize() {
            if let Ok(workspace) = base_dir.canonicalize() {
                if !canonical.starts_with(&workspace) {
                    all_success = false;
                    details.push(format!(
                        "SKIP: path '{}' is outside workspace '{}'",
                        resolved_path.display(),
                        base_dir.display()
                    ));
                    continue;
                }
            }
        } else if let Some(parent) = resolved_path.parent() {
            if let (Ok(canonical_parent), Ok(workspace)) =
                (parent.canonicalize(), base_dir.canonicalize())
            {
                if !canonical_parent.starts_with(&workspace) {
                    all_success = false;
                    details.push(format!(
                        "SKIP: path '{}' is outside workspace '{}'",
                        resolved_path.display(),
                        base_dir.display()
                    ));
                    continue;
                }
            }
        }

        let is_new_file = target.old_path == "/dev/null";

        if !is_new_file && !resolved_path.exists() {
            all_success = false;
            details.push(format!("FAIL: file not found: {}", resolved_path.display()));
            for _hunk in &target.hunks {
                hunks_failed += 1;
            }
            continue;
        }

        let old_content = if is_new_file {
            String::new()
        } else {
            std::fs::read_to_string(&resolved_path)
                .map_err(|e| format!("Cannot read {}: {e}", resolved_path.display()))?
        };

        let (new_content, applied, failed) = apply_hunks_to_content(&old_content, &target.hunks);

        hunks_applied += applied;
        hunks_failed += failed;

        if failed > 0 {
            all_success = false;
        }

        if dry_run {
            if applied > 0 {
                details.push(format!(
                    "Would apply {applied} hunk(s) to {}",
                    resolved_path.display()
                ));
                files_modified.push(resolved_path.display().to_string());
            }
            if failed > 0 {
                details.push(format!(
                    "Would fail to apply {failed} hunk(s) to {}",
                    resolved_path.display()
                ));
            }
        } else if applied > 0 {
            if let Some(parent) = resolved_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        format!("Cannot create directory {}: {e}", parent.display())
                    })?;
                }
            }
            std::fs::write(&resolved_path, &new_content)
                .map_err(|e| format!("Cannot write {}: {e}", resolved_path.display()))?;
            details.push(format!(
                "Applied {applied} hunk(s) to {}",
                resolved_path.display()
            ));
            files_modified.push(resolved_path.display().to_string());
            if failed > 0 {
                details.push(format!(
                    "Failed to apply {failed} hunk(s) to {}",
                    resolved_path.display()
                ));
            }
        } else if failed > 0 {
            details.push(format!(
                "Failed to apply {failed} hunk(s) to {}",
                resolved_path.display()
            ));
        }
    }

    let summary = if details.is_empty() {
        "No changes to apply".to_string()
    } else {
        details.join("\n")
    };

    Ok(ApplyPatchOutput {
        success: all_success,
        files_modified,
        hunks_applied,
        hunks_failed,
        output: summary,
    })
}

/// Returns the tool spec for the `ApplyPatch` tool as a JSON value.
pub fn apply_patch_tool_spec() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(ApplyPatchInput)).unwrap()
}

fn parse_patch(patch: &str) -> Result<Vec<FileTarget>, String> {
    let mut targets: Vec<FileTarget> = Vec::new();
    let lines: Vec<&str> = patch.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if line.starts_with("--- ") {
            let old_path = line.strip_prefix("--- ").unwrap_or("").trim();
            i += 1;

            if i >= lines.len() || !lines[i].starts_with("+++ ") {
                return Err("Patch has '---' without matching '+++'".to_string());
            }

            let new_path = lines[i].strip_prefix("+++ ").unwrap_or("").trim();
            i += 1;

            if is_binary_patch_line(old_path) || is_binary_patch_line(new_path) {
                return Err("Binary patches are not supported".to_string());
            }

            let mut hunks = Vec::new();
            while i < lines.len() && lines[i].starts_with("@@") {
                let (hunk, next_i) = parse_hunk(&lines, i)?;
                hunks.push(hunk);
                i = next_i;
            }

            targets.push(FileTarget {
                old_path: old_path.to_string(),
                new_path: new_path.to_string(),
                hunks,
            });
        } else {
            i += 1;
        }
    }

    Ok(targets)
}

fn is_binary_patch_line(path: &str) -> bool {
    path.contains("Binary file") || path.contains("binary file")
}

fn parse_hunk(lines: &[&str], start: usize) -> Result<(Hunk, usize), String> {
    let header = lines[start];

    let (old_start, old_count, new_start, new_count) = parse_hunk_header(header)?;

    let mut hunk_lines = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = lines[i];

        if line.starts_with("@@")
            || line.starts_with("--- ")
            || line.starts_with("diff ")
            || line.starts_with("index ")
        {
            break;
        }

        if line.starts_with('\\') {
            hunk_lines.push(HunkLine::NoNewline);
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix(' ') {
            hunk_lines.push(HunkLine::Context(stripped.to_string()));
        } else if let Some(stripped) = line.strip_prefix('-') {
            hunk_lines.push(HunkLine::Remove(stripped.to_string()));
        } else if let Some(stripped) = line.strip_prefix('+') {
            hunk_lines.push(HunkLine::Add(stripped.to_string()));
        } else if line.is_empty() {
            hunk_lines.push(HunkLine::Context(String::new()));
        } else {
            break;
        }

        i += 1;
    }

    Ok((
        Hunk {
            old_start,
            old_count,
            _new_start: new_start,
            new_count,
            lines: hunk_lines,
        },
        i,
    ))
}

fn parse_hunk_header(header: &str) -> Result<(usize, usize, usize, usize), String> {
    let content = header
        .strip_prefix("@@")
        .ok_or_else(|| format!("Invalid hunk header: {header}"))?;

    let end_idx = content.rfind("@@").map_or(content.len(), |idx| {
        if idx > 0 && content.as_bytes().get(idx - 1) == Some(&b' ') {
            idx - 1
        } else {
            idx
        }
    });

    let range_spec = content[..end_idx].trim();

    let parts: Vec<&str> = range_spec.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(format!("Invalid hunk header: {header}"));
    }

    let old_range = parts[0]
        .strip_prefix('-')
        .ok_or_else(|| format!("Invalid old range in hunk header: {}", parts[0]))?;
    let new_range = parts[1]
        .strip_prefix('+')
        .ok_or_else(|| format!("Invalid new range in hunk header: {}", parts[1]))?;

    let (old_start, old_count) = parse_range(old_range)?;
    let (new_start, new_count) = parse_range(new_range)?;

    Ok((old_start, old_count, new_start, new_count))
}

fn parse_range(range: &str) -> Result<(usize, usize), String> {
    if let Some(comma_idx) = range.find(',') {
        let start: usize = range[..comma_idx]
            .parse()
            .map_err(|e| format!("Invalid range start '{range}': {e}"))?;
        let count: usize = range[comma_idx + 1..]
            .parse()
            .map_err(|e| format!("Invalid range count '{range}': {e}"))?;
        Ok((start, count))
    } else {
        let start: usize = range
            .parse()
            .map_err(|e| format!("Invalid range '{range}': {e}"))?;
        Ok((start, 1))
    }
}

fn apply_hunks_to_content(content: &str, hunks: &[Hunk]) -> (String, usize, usize) {
    let mut lines: Vec<String> = if content.is_empty() {
        Vec::new()
    } else {
        content.lines().map(str::to_string).collect()
    };

    let ends_with_newline = content.ends_with('\n') && !content.is_empty();

    let mut applied = 0;
    let mut failed = 0;
    let mut offset: isize = 0;

    for hunk in hunks {
        let adjusted_start = if hunk.old_start == 0 {
            0
        } else {
            ((hunk.old_start.cast_signed() - 1 + offset).max(0)).cast_unsigned()
        };

        match apply_single_hunk(&mut lines, hunk, adjusted_start) {
            Ok(new_line_delta) => {
                offset += new_line_delta;
                applied += 1;
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    let mut result = lines.join("\n");
    if ends_with_newline || !hunks.is_empty() {
        result.push('\n');
    }

    (result, applied, failed)
}

fn apply_single_hunk(lines: &mut Vec<String>, hunk: &Hunk, start: usize) -> Result<isize, String> {
    let mut old_lines_to_match: Vec<&str> = Vec::new();
    for hline in &hunk.lines {
        match hline {
            HunkLine::Context(s) | HunkLine::Remove(s) => {
                old_lines_to_match.push(s.as_str());
            }
            HunkLine::Add(_) | HunkLine::NoNewline => {}
        }
    }

    let max_start = if lines.len() >= old_lines_to_match.len() {
        lines.len() - old_lines_to_match.len()
    } else {
        0
    };

    let try_positions = if start <= max_start {
        let mut positions = vec![start];
        let mut delta: usize = 1;
        while positions.len() < 100 {
            if start >= delta {
                positions.push(start - delta);
            }
            if start + delta <= max_start {
                positions.push(start + delta);
            } else {
                break;
            }
            delta += 1;
        }
        positions
    } else {
        let mut positions = Vec::new();
        for p in (0..=max_start).rev() {
            positions.push(p);
        }
        positions
    };

    let mut best_pos: Option<usize> = None;
    let mut best_match_count = 0;

    for pos in try_positions {
        let mut match_count = 0;
        for (j, &expected) in old_lines_to_match.iter().enumerate() {
            if pos + j < lines.len() && lines[pos + j] == expected {
                match_count += 1;
            } else {
                break;
            }
        }
        if match_count > best_match_count {
            best_match_count = match_count;
            best_pos = Some(pos);
        }
        if match_count == old_lines_to_match.len() {
            break;
        }
    }

    let pos = match best_pos {
        Some(p) => p,
        None => {
            if old_lines_to_match.is_empty() {
                start.min(lines.len())
            } else {
                return Err("Could not find matching context for hunk".to_string());
            }
        }
    };

    let mut new_hunk_lines: Vec<String> = Vec::new();
    for hline in &hunk.lines {
        match hline {
            HunkLine::Context(s) | HunkLine::Add(s) => {
                new_hunk_lines.push(s.clone());
            }
            HunkLine::Remove(_) | HunkLine::NoNewline => {}
        }
    }

    let old_line_count = old_lines_to_match.len();
    let new_line_count = new_hunk_lines.len();

    let remove_end = (pos + old_line_count).min(lines.len());
    lines.splice(pos..remove_end, new_hunk_lines);

    Ok(new_line_count.cast_signed() - old_line_count.cast_signed())
}

fn resolve_path(raw_path: &str, strip: u32, base_dir: &Path) -> Result<PathBuf, String> {
    if raw_path == "/dev/null" {
        return Err("Path is /dev/null (no target file)".to_string());
    }

    let mut path: String = raw_path.to_string();

    if strip == 0 {
        if path.starts_with("a/") || path.starts_with("b/") {
            path = path[2..].to_string();
        }
    } else {
        let stripped_prefix = if path.starts_with("a/") || path.starts_with("b/") {
            path[2..].to_string()
        } else {
            path.clone()
        };

        let remaining_strip =
            if stripped_prefix.starts_with("a/") || stripped_prefix.starts_with("b/") {
                strip.saturating_sub(1)
            } else {
                strip
            };

        path = stripped_prefix;
        let components: Vec<&str> = path.split('/').collect();

        if remaining_strip as usize >= components.len() {
            path = components.last().unwrap_or(&"").to_string();
        } else {
            path = components[remaining_strip as usize..].join("/");
        }
    }

    if path.is_empty() {
        return Err("Resulting path is empty after stripping".to_string());
    }

    let p = PathBuf::from(path);
    if p.is_absolute() {
        Ok(p)
    } else {
        Ok(base_dir.join(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct CleanupGuard {
        path: Option<PathBuf>,
    }

    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            if let Some(ref p) = self.path {
                if let Some(parent) = p.parent() {
                    let _ = fs::remove_dir_all(parent);
                }
            }
        }
    }

    fn make_temp_file(content: &str) -> (PathBuf, CleanupGuard) {
        let dir = std::env::temp_dir().join(format!(
            "apply_patch_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir).expect("failed to create temp dir");
        let path = dir.join("test.txt");
        fs::write(&path, content).expect("failed to write temp file");
        (path, CleanupGuard { path: Some(dir) })
    }

    #[test]
    #[allow(clippy::used_underscore_binding)]
    fn parse_single_file_patch() {
        let patch = "\
--- a/hello.txt
+++ b/hello.txt
@@ -1,3 +1,3 @@
 Hello
-World
-Old line
+New world
+New line
 Goodbye
";

        let targets = parse_patch(patch).expect("parse failed");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].old_path, "a/hello.txt");
        assert_eq!(targets[0].new_path, "b/hello.txt");
        assert_eq!(targets[0].hunks.len(), 1);

        let hunk = &targets[0].hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 3);
        assert_eq!(hunk._new_start, 1);
        assert_eq!(hunk.new_count, 3);
        assert_eq!(hunk.lines.len(), 6);
    }

    #[test]
    fn parse_multi_file_patch() {
        let patch = "\
--- a/file1.txt
+++ b/file1.txt
@@ -1 +1 @@
-old1
+new1
--- a/file2.txt
+++ b/file2.txt
@@ -1,2 +1,2 @@
 context1
-old2
+new2
";

        let targets = parse_patch(patch).expect("parse failed");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].new_path, "b/file1.txt");
        assert_eq!(targets[1].new_path, "b/file2.txt");
        assert_eq!(targets[0].hunks.len(), 1);
        assert_eq!(targets[1].hunks.len(), 1);
    }

    #[test]
    fn parse_hunk_header_variants() {
        let (s, c, ns, nc) = parse_hunk_header("@@ -1,5 +1,6 @@").unwrap();
        assert_eq!((s, c, ns, nc), (1, 5, 1, 6));

        let (s, c, ns, nc) = parse_hunk_header("@@ -10 +20 @@").unwrap();
        assert_eq!((s, c, ns, nc), (10, 1, 20, 1));

        let (s, c, ns, nc) = parse_hunk_header("@@ -1,3 +1,4 @@ some function").unwrap();
        assert_eq!((s, c, ns, nc), (1, 3, 1, 4));
    }

    #[test]
    fn parse_patch_with_dev_null() {
        let patch = "\
--- /dev/null
+++ b/new_file.txt
@@ -0,0 +1,2 @@
+line1
+line2
";

        let targets = parse_patch(patch).expect("parse failed");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].old_path, "/dev/null");
        assert_eq!(targets[0].new_path, "b/new_file.txt");
        assert_eq!(targets[0].hunks.len(), 1);
    }

    #[test]
    fn parse_patch_rejects_binary() {
        let patch = "Binary file image.png differs\n";
        let targets = parse_patch(patch).expect("should not error");
        assert!(targets.is_empty());

        let patch_with_binary_path = "\
--- Binary file a/image.png
+++ b/image.png
@@ -0,0 +1 @@
+test
";
        let result = parse_patch(patch_with_binary_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Binary"));
    }

    #[test]
    fn apply_single_file_patch() {
        let (path, _guard) = make_temp_file("Hello\nWorld\nOld line\nGoodbye\n");

        let patch_content = "\
--- a/test.txt
+++ b/test.txt
@@ -1,4 +1,4 @@
 Hello
-World
-Old line
+New world
+New line
 Goodbye
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(false),
            cwd: Some(
                path.parent()
                    .expect("parent exists")
                    .to_string_lossy()
                    .to_string(),
            ),
        };

        let result = execute_apply_patch(&input).expect("apply failed");
        assert!(result.success);
        assert_eq!(result.hunks_applied, 1);
        assert_eq!(result.hunks_failed, 0);
        assert_eq!(result.files_modified.len(), 1);

        let new_content = fs::read_to_string(&path).expect("file should exist after apply");
        assert!(new_content.contains("New world"));
        assert!(new_content.contains("New line"));
        assert!(!new_content.contains("Old line"));
    }

    #[test]
    fn apply_dry_run_does_not_modify_file() {
        let (path, _guard) = make_temp_file("Hello\nWorld\n");
        let original_content = fs::read_to_string(&path).expect("should read original");

        let patch_content = "\
--- a/test.txt
+++ b/test.txt
@@ -1,2 +1,2 @@
 Hello
-World
+Changed
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(true),
            cwd: Some(
                path.parent()
                    .expect("parent exists")
                    .to_string_lossy()
                    .to_string(),
            ),
        };

        let result = execute_apply_patch(&input).expect("dry_run should not error");
        assert!(result.success);
        assert_eq!(result.hunks_applied, 1);
        assert_eq!(result.files_modified.len(), 1);

        let after_content = fs::read_to_string(&path).expect("should read after dry run");
        assert_eq!(after_content, original_content);
    }

    #[test]
    fn apply_multi_file_patch() {
        let dir = std::env::temp_dir().join(format!(
            "apply_patch_multi_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir).expect("create dir");
        let path1 = dir.join("file1.txt");
        let path2 = dir.join("file2.txt");
        fs::write(&path1, "old1\n").expect("write file1");
        fs::write(&path2, "context1\nold2\n").expect("write file2");
        let dir_str = dir.to_string_lossy().to_string();

        let patch_content = "\
--- a/file1.txt
+++ b/file1.txt
@@ -1 +1 @@
-old1
+new1
--- a/file2.txt
+++ b/file2.txt
@@ -1,2 +1,2 @@
 context1
-old2
+new2
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(false),
            cwd: Some(dir_str.clone()),
        };

        let result = execute_apply_patch(&input).expect("apply failed");
        assert!(result.success);
        assert_eq!(result.hunks_applied, 2);
        assert_eq!(result.hunks_failed, 0);
        assert_eq!(result.files_modified.len(), 2);

        assert_eq!(fs::read_to_string(&path1).unwrap(), "new1\n");
        assert_eq!(fs::read_to_string(&path2).unwrap(), "context1\nnew2\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_new_file_creation() {
        let dir = std::env::temp_dir().join(format!(
            "apply_patch_new_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir).expect("create dir");
        let dir_str = dir.to_string_lossy().to_string();

        let patch_content = "\
--- /dev/null
+++ b/created.txt
@@ -0,0 +1,3 @@
+line one
+line two
+line three
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(false),
            cwd: Some(dir_str.clone()),
        };

        let result = execute_apply_patch(&input).expect("apply failed");
        assert!(result.success);
        assert_eq!(result.hunks_applied, 1);

        let created = dir.join("created.txt");
        assert!(created.exists());
        let content = fs::read_to_string(&created).expect("read new file");
        assert!(content.contains("line one"));
        assert!(content.contains("line two"));
        assert!(content.contains("line three"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_fails_for_missing_file() {
        let dir = std::env::temp_dir().join(format!(
            "apply_patch_missing_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir).expect("create dir");
        let dir_str = dir.to_string_lossy().to_string();

        let patch_content = "\
--- a/nonexistent.txt
+++ b/nonexistent.txt
@@ -1 +1 @@
-old
+new
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(false),
            cwd: Some(dir_str),
        };

        let result = execute_apply_patch(&input).expect("should not error");
        assert!(!result.success);
        assert_eq!(result.hunks_failed, 1);
        assert!(result.output.contains("file not found"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn strip_path_components() {
        let patch_content = "\
--- a/src/utils/helper.rs
+++ b/src/utils/helper.rs
@@ -1 +1 @@
-old
+new
";
        let targets = parse_patch(patch_content).expect("parse failed");
        assert_eq!(targets.len(), 1);

        let base = PathBuf::from("/some/base");
        assert_eq!(
            resolve_path("a/src/utils/helper.rs", 1, &base).unwrap(),
            base.join("utils/helper.rs")
        );
        assert_eq!(
            resolve_path("a/src/utils/helper.rs", 2, &base).unwrap(),
            base.join("helper.rs")
        );
        assert_eq!(
            resolve_path("a/src/utils/helper.rs", 0, &base).unwrap(),
            base.join("src/utils/helper.rs")
        );
    }

    #[test]
    fn tool_spec_has_required_fields() {
        let spec = apply_patch_tool_spec();
        let props = spec.get("properties").expect("spec should have properties");
        assert!(props.get("patch").is_some());
        assert!(props.get("strip").is_some());
        assert!(props.get("dry_run").is_some());
        assert!(props.get("cwd").is_some());

        let required = spec.get("required").expect("spec should have required");
        assert!(required.is_array());
        let arr = required.as_array().unwrap();
        assert!(arr.contains(&serde_json::json!("patch")));
    }

    #[test]
    fn apply_patch_with_no_trailing_newline() {
        let (path, _guard) = make_temp_file("Hello\nWorld");

        let patch_content = "\
--- a/test.txt
+++ b/test.txt
@@ -1,2 +1,2 @@
 Hello
-World
+World!
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(false),
            cwd: Some(
                path.parent()
                    .expect("parent exists")
                    .to_string_lossy()
                    .to_string(),
            ),
        };

        let result = execute_apply_patch(&input).expect("apply failed");
        assert!(result.success);

        let content = fs::read_to_string(&path).expect("read file");
        assert!(content.contains("World!"));
        assert!(!content.contains("World\n"));
    }

    #[test]
    fn apply_hunk_with_context_only() {
        let (path, _guard) = make_temp_file("line1\nline2\nline3\n");

        let patch_content = "\
--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,2 @@
 line1
-line2
 line3
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(false),
            cwd: Some(
                path.parent()
                    .expect("parent exists")
                    .to_string_lossy()
                    .to_string(),
            ),
        };

        let result = execute_apply_patch(&input).expect("apply failed");
        assert!(result.success);
        assert_eq!(result.hunks_applied, 1);

        let content = fs::read_to_string(&path).expect("read file");
        assert!(content.contains("line1"));
        assert!(content.contains("line3"));
        assert!(!content.contains("line2"));
    }

    #[test]
    fn apply_hunk_with_addition_only() {
        let (path, _guard) = make_temp_file("line1\nline3\n");

        let patch_content = "\
--- a/test.txt
+++ b/test.txt
@@ -1,2 +1,3 @@
 line1
+line2
 line3
"
        .to_string();

        let input = ApplyPatchInput {
            patch: patch_content,
            strip: Some(1),
            dry_run: Some(false),
            cwd: Some(
                path.parent()
                    .expect("parent exists")
                    .to_string_lossy()
                    .to_string(),
            ),
        };

        let result = execute_apply_patch(&input).expect("apply failed");
        assert!(result.success);

        let content = fs::read_to_string(&path).expect("read file");
        assert_eq!(content, "line1\nline2\nline3\n");
    }
}
