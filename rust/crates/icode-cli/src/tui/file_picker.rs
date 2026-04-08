use std::fs;
use std::path::Path;

/// Scans a directory recursively for files suitable for referencing.
/// Returns a sorted Vec of relative paths (relative to `root`).
///
/// Exclusions:
/// - Directories: .git, node_modules, target, .venv
/// - Binary files (by extension)
/// - Depth > 10
/// - Max 500 files
pub fn scan_files(root: &str) -> Vec<String> {
    let mut files = Vec::new();
    let binary_extensions = [
        "png", "jpg", "jpeg", "gif", "bmp", "ico", "tiff", "webp", "svg", "exe", "dll", "so",
        "dylib", "wasm", "o", "a", "lib", "pyc", "pyo", "class", "jar", "war", "ear", "zip", "tar",
        "gz", "bz2", "xz", "7z", "rar", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "mp3",
        "mp4", "avi", "mov", "mkv", "flac", "ogg", "wav", "ttf", "otf", "woff", "woff2", "eot",
        "lock", "db", "sqlite",
    ];

    let excluded_dirs = [".git", "node_modules", "target", ".venv"];

    collect_files(
        Path::new(root),
        Path::new(root),
        &mut files,
        0,
        &binary_extensions,
        &excluded_dirs,
    );

    files.sort();
    files
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
    depth: usize,
    binary_extensions: &[&str],
    excluded_dirs: &[&str],
) {
    if depth > 10 || files.len() >= 500 {
        return;
    }

    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if files.len() >= 500 {
            return;
        }

        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if path.is_dir() {
            if excluded_dirs.contains(&file_name) {
                continue;
            }
            collect_files(
                root,
                &path,
                files,
                depth + 1,
                binary_extensions,
                excluded_dirs,
            );
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if binary_extensions.contains(&ext.as_str()) {
                continue;
            }

            if let Ok(rel) = path.strip_prefix(root) {
                let rel_str = rel.to_string_lossy().to_string();
                files.push(rel_str);
            }
        }
    }
}

/// Fuzzy-match files against a query using simple substring matching.
/// A file matches if every character in the query appears in order in the filename.
pub fn fuzzy_match(files: &[String], query: &str) -> Vec<String> {
    if query.is_empty() {
        return files.iter().take(50).cloned().collect();
    }

    let query_lower = query.to_lowercase();
    let mut results: Vec<&String> = files
        .iter()
        .filter(|f| substring_match(&f.to_lowercase(), &query_lower))
        .collect();

    // Sort by relevance: prefer shorter matches and earlier positions
    results.sort_by(|a, b| {
        let a_pos = first_match_pos(&a.to_lowercase(), &query_lower);
        let b_pos = first_match_pos(&b.to_lowercase(), &query_lower);
        a_pos.cmp(&b_pos).then(a.len().cmp(&b.len()))
    });

    results.into_iter().cloned().collect()
}

/// Simple substring match: all characters of query must appear in order in text.
fn substring_match(text: &str, query: &str) -> bool {
    let mut query_chars = query.chars();
    let mut current = match query_chars.next() {
        Some(c) => c,
        None => return true,
    };

    for ch in text.chars() {
        if ch == current {
            match query_chars.next() {
                Some(c) => current = c,
                None => return true,
            }
        }
    }
    false
}

/// Find the position of the first match of query in text.
fn first_match_pos(text: &str, query: &str) -> usize {
    if query.is_empty() {
        return 0;
    }
    let text_chars: Vec<char> = text.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();

    for start in 0..=text_chars.len().saturating_sub(query_chars.len()) {
        let mut matched = 0;
        for (i, &qc) in query_chars.iter().enumerate() {
            if text_chars.get(start + i) == Some(&qc) {
                matched += 1;
            } else {
                break;
            }
        }
        if matched == query_chars.len() {
            return start;
        }
    }
    // Fallback: character-by-character subsequence match
    let mut qi = 0;
    for (ti, &tc) in text_chars.iter().enumerate() {
        if tc == query_chars[qi] {
            qi += 1;
            if qi == query_chars.len() {
                return ti;
            }
        }
    }
    text.len()
}

/// Read the contents of a file, with a size limit of 50KB.
/// Returns an error message if the file is too large or cannot be read.
pub fn read_file_content(path: &str) -> String {
    const MAX_CONTENT_SIZE: usize = 50 * 1024; // 50KB

    match fs::read_to_string(path) {
        Ok(content) => {
            if content.len() > MAX_CONTENT_SIZE {
                format!(
                    "[File too large: {} bytes (max {})]\n{}\n... (truncated)",
                    content.len(),
                    MAX_CONTENT_SIZE,
                    &content[..MAX_CONTENT_SIZE]
                )
            } else {
                content
            }
        }
        Err(e) => format!("[Error reading file: {e}]"),
    }
}

/// Represents a file reference parsed from prompt text.
pub struct ParsedFileRef {
    pub path: String,
    pub content: String,
}

/// Parse `@path` references from prompt text.
/// Returns (clean_text_with_refs_stripped, Vec<ParsedFileRef>).
pub fn parse_file_references(text: &str, cwd: &str) -> (String, Vec<ParsedFileRef>) {
    let mut refs = Vec::new();
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '@' {
            let mut path = String::new();
            while let Some(&nc) = chars.peek() {
                if nc.is_whitespace() {
                    break;
                }
                path.push(chars.next().unwrap_or(' '));
            }
            if path.is_empty() {
                result.push('@');
            } else {
                let full_path = if std::path::Path::new(&path).exists() {
                    path.clone()
                } else {
                    format!("{cwd}/{path}")
                };
                let content = read_file_content(&full_path);
                refs.push(ParsedFileRef { path, content });
            }
        } else {
            result.push(ch);
        }
    }

    (result.trim().to_string(), refs)
}

/// File picker state for the TUI.
#[derive(Debug, Default)]
pub struct FilePickerState {
    /// Whether the file picker is currently visible.
    pub open: bool,
    /// All scanned files (relative paths).
    pub files: Vec<String>,
    /// Currently filtered/matched files.
    pub matches: Vec<String>,
    /// Current query text (text after @).
    pub query: String,
    /// Current selection index.
    pub idx: usize,
    /// Scroll offset for the picker list.
    pub scroll: usize,
    /// The character position in the input where @ was typed.
    pub at_pos: usize,
}

impl FilePickerState {
    pub fn open(&mut self, cwd: &str, at_pos: usize) {
        self.open = true;
        self.at_pos = at_pos;
        self.query.clear();
        self.idx = 0;
        self.scroll = 0;
        self.files = scan_files(cwd);
        self.matches = self.files.iter().take(50).cloned().collect();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.files.clear();
        self.matches.clear();
        self.query.clear();
    }

    pub fn update_query(&mut self) {
        self.matches = fuzzy_match(&self.files, &self.query);
        self.idx = 0;
        self.scroll = 0;
    }

    pub fn cursor_up(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        if self.idx == 0 {
            self.idx = self.matches.len() - 1;
        } else {
            self.idx -= 1;
        }
        self.ensure_visible();
    }

    pub fn cursor_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.idx = (self.idx + 1) % self.matches.len();
        self.ensure_visible();
    }

    pub fn selected(&self) -> Option<&str> {
        self.matches.get(self.idx).map(String::as_str)
    }

    /// Format the selected file as an @reference.
    pub fn selected_reference(&self) -> Option<String> {
        self.selected().map(|p| format!("@{p}"))
    }

    fn ensure_visible(&mut self) {
        const MAX_VISIBLE: usize = 10;
        if self.idx < self.scroll {
            self.scroll = self.idx;
        } else if self.idx >= self.scroll + MAX_VISIBLE {
            self.scroll = self.idx - MAX_VISIBLE + 1;
        }
    }
}
