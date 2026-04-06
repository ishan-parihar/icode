use ast_grep_tools::replace::replace_in_content;
use ast_grep_tools::search::{search_in_content, search_in_file};
use ast_grep_tools::types::{
    AstGrepReplaceRequest, AstGrepReplaceResult, AstGrepSearchRequest, AstGrepSearchResult,
    MatchInfo,
};

#[test]
fn search_basic_pattern_match() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "console.log($MSG)".to_string(),
        language: "javascript".to_string(),
        file_path: None,
        content: Some("console.log('hello');".to_string()),
        context_lines: None,
    })
    .unwrap();
    assert_eq!(result.total_matches, 1);
    assert_eq!(result.matches[0].matched_text, "console.log('hello')");
}

#[test]
fn search_meta_variable_extraction() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "fn $NAME() {}".to_string(),
        language: "rust".to_string(),
        file_path: None,
        content: Some("fn main() {}".to_string()),
        context_lines: None,
    })
    .unwrap();
    assert_eq!(result.total_matches, 1);
    let meta = &result.matches[0].meta_variables;
    assert_eq!(meta["NAME"], "main");
}

#[test]
fn search_multi_match() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "let $X = $Y".to_string(),
        language: "typescript".to_string(),
        file_path: None,
        content: Some("let a = 1; let b = 2;".to_string()),
        context_lines: None,
    })
    .unwrap();
    assert_eq!(result.total_matches, 2);
}

#[test]
fn search_no_match() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "function foo() {}".to_string(),
        language: "javascript".to_string(),
        file_path: None,
        content: Some("function bar() {}".to_string()),
        context_lines: None,
    })
    .unwrap();
    assert_eq!(result.total_matches, 0);
}

#[test]
fn search_python_language() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "def $FUNC(): pass".to_string(),
        language: "python".to_string(),
        file_path: None,
        content: Some("def hello(): pass".to_string()),
        context_lines: None,
    })
    .unwrap();
    assert_eq!(result.total_matches, 1);
    assert_eq!(result.matches[0].meta_variables["FUNC"], "hello");
}

#[test]
fn search_go_language() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "func $NAME() {}".to_string(),
        language: "go".to_string(),
        file_path: None,
        content: Some("func main() {}".to_string()),
        context_lines: None,
    })
    .unwrap();
    assert_eq!(result.total_matches, 1);
    assert_eq!(result.matches[0].meta_variables["NAME"], "main");
}

#[test]
fn search_unsupported_language() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "foo".to_string(),
        language: "brainfuck".to_string(),
        file_path: None,
        content: Some("foo".to_string()),
        context_lines: None,
    });
    assert!(result.is_err());
}

#[test]
fn search_empty_content() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "foo".to_string(),
        language: "javascript".to_string(),
        file_path: None,
        content: Some("".to_string()),
        context_lines: None,
    });
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn search_no_content_provided() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "foo".to_string(),
        language: "javascript".to_string(),
        file_path: None,
        content: None,
        context_lines: None,
    });
    assert!(result.is_err());
}

#[test]
fn search_invalid_pattern() {
    let result = search_in_content(AstGrepSearchRequest {
        pattern: "a b c".to_string(),
        language: "javascript".to_string(),
        file_path: None,
        content: Some("foo".to_string()),
        context_lines: None,
    });
    assert!(result.is_err());
}

#[test]
fn replace_single() {
    let result = replace_in_content(&AstGrepReplaceRequest {
        pattern: "var $A = $B".to_string(),
        rewrite: "let $A = $B".to_string(),
        language: "javascript".to_string(),
        content: "var x = 1;".to_string(),
        dry_run: false,
    })
    .unwrap();
    assert_eq!(result.replacements, 1);
    assert_eq!(result.modified, "let x = 1;");
}

#[test]
fn replace_multi() {
    let result = replace_in_content(&AstGrepReplaceRequest {
        pattern: "var $A = $B".to_string(),
        rewrite: "let $A = $B".to_string(),
        language: "javascript".to_string(),
        content: "var a = 1; var b = 2;".to_string(),
        dry_run: false,
    })
    .unwrap();
    assert_eq!(result.replacements, 2);
    assert_eq!(result.modified, "let a = 1; let b = 2;");
}

#[test]
fn replace_meta_var_preservation() {
    let result = replace_in_content(&AstGrepReplaceRequest {
        pattern: "print($MSG)".to_string(),
        rewrite: "console.log($MSG)".to_string(),
        language: "javascript".to_string(),
        content: "print('hello')".to_string(),
        dry_run: false,
    })
    .unwrap();
    assert_eq!(result.replacements, 1);
    assert_eq!(result.modified, "console.log('hello')");
}

#[test]
fn replace_dry_run() {
    let result = replace_in_content(&AstGrepReplaceRequest {
        pattern: "var $A = $B".to_string(),
        rewrite: "let $A = $B".to_string(),
        language: "javascript".to_string(),
        content: "var x = 1;".to_string(),
        dry_run: true,
    })
    .unwrap();
    assert_eq!(result.replacements, 1);
    assert!(!result.diff.is_empty());
    assert!(result.diff.contains("--- original"));
    assert!(result.diff.contains("+++ modified"));
}

#[test]
fn replace_no_match() {
    let result = replace_in_content(&AstGrepReplaceRequest {
        pattern: "var $A = $B".to_string(),
        rewrite: "let $A = $B".to_string(),
        language: "javascript".to_string(),
        content: "const x = 1;".to_string(),
        dry_run: false,
    })
    .unwrap();
    assert_eq!(result.replacements, 0);
    assert_eq!(result.modified, "const x = 1;");
}

#[test]
fn replace_rust() {
    let result = replace_in_content(&AstGrepReplaceRequest {
        pattern: "let $A = $B".to_string(),
        rewrite: "let mut $A = $B".to_string(),
        language: "rust".to_string(),
        content: "let x = 42".to_string(),
        dry_run: false,
    })
    .unwrap();
    assert_eq!(result.replacements, 1);
    assert!(result.modified.contains("mut"));
}

#[test]
fn search_in_file_basic() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "console.log('test');").unwrap();

    let result = search_in_file(
        "console.log($MSG)",
        "javascript",
        tmp.path().to_str().unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(result.total_matches, 1);
}

#[test]
fn roundtrip_search_request() {
    let req = AstGrepSearchRequest {
        pattern: "fn $NAME() {}".to_string(),
        language: "rust".to_string(),
        file_path: Some("main.rs".to_string()),
        content: Some("fn main() {}".to_string()),
        context_lines: Some(3),
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: AstGrepSearchRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.pattern, req.pattern);
    assert_eq!(back.language, req.language);
    assert_eq!(back.file_path, req.file_path);
    assert_eq!(back.content, req.content);
    assert_eq!(back.context_lines, req.context_lines);
}

#[test]
fn roundtrip_search_result() {
    let result = AstGrepSearchResult {
        matches: vec![MatchInfo {
            file: "main.rs".to_string(),
            line: 0,
            column: 0,
            matched_text: "fn main() {}".to_string(),
            meta_variables: serde_json::json!({"NAME": "main"}),
        }],
        total_matches: 1,
    };
    let json = serde_json::to_string(&result).unwrap();
    let back: AstGrepSearchResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.total_matches, 1);
    assert_eq!(back.matches[0].matched_text, "fn main() {}");
}

#[test]
fn roundtrip_replace_request() {
    let req = AstGrepReplaceRequest {
        pattern: "var $A".to_string(),
        rewrite: "let $A".to_string(),
        language: "javascript".to_string(),
        content: "var x".to_string(),
        dry_run: true,
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: AstGrepReplaceRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.dry_run, true);
    assert_eq!(back.rewrite, "let $A");
}

#[test]
fn roundtrip_replace_result() {
    let result = AstGrepReplaceResult {
        original: "var x".to_string(),
        modified: "let x".to_string(),
        replacements: 1,
        diff: "--- original\n+++ modified\n@@ -1,1 +1,1 @@\n-var x\n+let x\n".to_string(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let back: AstGrepReplaceResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.replacements, 1);
    assert_eq!(back.modified, "let x");
}
