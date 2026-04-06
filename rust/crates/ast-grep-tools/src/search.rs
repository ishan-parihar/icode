use ast_grep_core::matcher::{NodeMatch, Pattern};
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{meta_var::MetaVariable, AstGrep};
use ast_grep_language::SupportLang;

use crate::types::{AstGrepSearchRequest, AstGrepSearchResult, MatchInfo};

fn parse_language(lang: &str) -> Result<SupportLang, String> {
    lang.parse::<SupportLang>()
        .map_err(|_| format!("Unsupported language: {lang}"))
}

fn extract_meta_vars(nm: &NodeMatch<'_, StrDoc<SupportLang>>) -> serde_json::Value {
    let env = nm.get_env();
    let mut map = serde_json::Map::new();
    for var in env.get_matched_variables() {
        let name = match &var {
            MetaVariable::Capture(n, _) | MetaVariable::MultiCapture(n) => n.clone(),
            _ => continue,
        };
        if let Some(node) = env.get_match(&name) {
            map.insert(name, serde_json::Value::String(node.text().to_string()));
        }
    }
    serde_json::Value::Object(map)
}

pub fn search_in_content(request: AstGrepSearchRequest) -> Result<AstGrepSearchResult, String> {
    let src = request.content.ok_or("No content provided")?;
    let lang = parse_language(&request.language)?;
    let pattern =
        Pattern::try_new(&request.pattern, lang).map_err(|e| format!("Invalid pattern: {e}"))?;

    let grep: AstGrep<StrDoc<SupportLang>> = AstGrep::new(&src, lang);
    let root = grep.root();

    let matches: Vec<MatchInfo> = root
        .find_all(pattern)
        .map(|nm| {
            let start = nm.start_pos();
            MatchInfo {
                file: request
                    .file_path
                    .clone()
                    .unwrap_or_else(|| "<stdin>".into()),
                line: start.line(),
                column: start.column(&nm),
                matched_text: nm.text().to_string(),
                meta_variables: extract_meta_vars(&nm),
            }
        })
        .collect();

    let total_matches = matches.len();
    Ok(AstGrepSearchResult {
        matches,
        total_matches,
    })
}

pub fn search_in_file(
    pattern: &str,
    lang: &str,
    file_path: &str,
    context: Option<usize>,
) -> Result<AstGrepSearchResult, String> {
    let src = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Cannot read file {file_path}: {e}"))?;

    search_in_content(AstGrepSearchRequest {
        pattern: pattern.to_string(),
        language: lang.to_string(),
        file_path: Some(file_path.to_string()),
        content: Some(src),
        context_lines: context,
    })
}
