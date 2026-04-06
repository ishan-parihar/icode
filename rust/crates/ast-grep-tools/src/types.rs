use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstGrepSearchRequest {
    pub pattern: String,
    pub language: String,
    pub file_path: Option<String>,
    pub content: Option<String>,
    pub context_lines: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstGrepSearchResult {
    pub matches: Vec<MatchInfo>,
    pub total_matches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub matched_text: String,
    pub meta_variables: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstGrepReplaceRequest {
    pub pattern: String,
    pub rewrite: String,
    pub language: String,
    pub content: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstGrepReplaceResult {
    pub original: String,
    pub modified: String,
    pub replacements: usize,
    pub diff: String,
}
