use std::fmt::Write;

use ast_grep_core::matcher::Pattern;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::AstGrep;
use ast_grep_language::SupportLang;

use crate::types::{AstGrepReplaceRequest, AstGrepReplaceResult};

fn parse_language(lang: &str) -> Result<SupportLang, String> {
    lang.parse::<SupportLang>()
        .map_err(|_| format!("Unsupported language: {lang}"))
}

fn generate_unified_diff(original: &str, modified: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let mod_lines: Vec<&str> = modified.lines().collect();

    if orig_lines == mod_lines {
        return String::new();
    }

    let mut diff = String::from("--- original\n+++ modified\n");

    let mut i = 0;
    let mut j = 0;

    while i < orig_lines.len() || j < mod_lines.len() {
        if i < orig_lines.len() && j < mod_lines.len() && orig_lines[i] == mod_lines[j] {
            let _ = writeln!(diff, " {}", orig_lines[i]);
            i += 1;
            j += 1;
        } else {
            let mut orig_hunk = Vec::new();
            let mut mod_hunk = Vec::new();
            let orig_start = i;
            let mod_start = j;

            while i < orig_lines.len() && j < mod_lines.len() {
                if orig_lines[i] == mod_lines[j] {
                    break;
                }
                let mut found = false;
                for lookahead in 1..=3 {
                    if j + lookahead < mod_lines.len() && orig_lines[i] == mod_lines[j + lookahead]
                    {
                        for k in 0..lookahead {
                            mod_hunk.push(mod_lines[j + k]);
                        }
                        j += lookahead;
                        found = true;
                        break;
                    }
                    if i + lookahead < orig_lines.len() && orig_lines[i + lookahead] == mod_lines[j]
                    {
                        for k in 0..lookahead {
                            orig_hunk.push(orig_lines[i + k]);
                        }
                        i += lookahead;
                        found = true;
                        break;
                    }
                }
                if !found {
                    orig_hunk.push(orig_lines[i]);
                    mod_hunk.push(mod_lines[j]);
                    i += 1;
                    j += 1;
                }
            }
            while i < orig_lines.len() {
                orig_hunk.push(orig_lines[i]);
                i += 1;
            }
            while j < mod_lines.len() {
                mod_hunk.push(mod_lines[j]);
                j += 1;
            }

            if !orig_hunk.is_empty() || !mod_hunk.is_empty() {
                let _ = writeln!(
                    diff,
                    "@@ -{},{} +{},{} @@",
                    orig_start + 1,
                    orig_hunk.len(),
                    mod_start + 1,
                    mod_hunk.len()
                );
                for line in &orig_hunk {
                    let _ = writeln!(diff, "-{line}");
                }
                for line in &mod_hunk {
                    let _ = writeln!(diff, "+{line}");
                }
            }
        }
    }

    diff
}

pub fn replace_in_content(request: &AstGrepReplaceRequest) -> Result<AstGrepReplaceResult, String> {
    let lang = parse_language(&request.language)?;
    let pattern =
        Pattern::try_new(&request.pattern, lang).map_err(|e| format!("Invalid pattern: {e}"))?;

    let original = request.content.clone();
    let mut grep: AstGrep<StrDoc<SupportLang>> = AstGrep::new(&original, lang);

    let mut replacements = 0;
    loop {
        let did_replace = grep
            .replace(&pattern, request.rewrite.as_str())
            .map_err(|e| format!("Replace error: {e}"))?;
        if !did_replace {
            break;
        }
        replacements += 1;
        if replacements > 10000 {
            return Err("Too many replacements, possible infinite loop".into());
        }
    }

    let modified = grep.generate();

    let diff = if request.dry_run {
        generate_unified_diff(&original, &modified)
    } else {
        String::new()
    };

    Ok(AstGrepReplaceResult {
        original,
        modified,
        replacements,
        diff,
    })
}
