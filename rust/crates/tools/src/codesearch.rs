use std::fmt;
use std::fs;
use std::path::Path;

use ignore::WalkBuilder;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSearchResult {
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub symbol: String,
    pub symbol_type: String,
    pub context: String,
    pub language: String,
}

#[derive(Debug)]
pub enum CodeSearchError {
    TreeSitterError(String),
    IoError(std::io::Error),
    UnsupportedLanguage(String),
    InvalidPattern(String),
}

impl fmt::Display for CodeSearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TreeSitterError(msg) => write!(f, "tree-sitter error: {msg}"),
            Self::IoError(err) => write!(f, "IO error: {err}"),
            Self::UnsupportedLanguage(lang) => write!(f, "unsupported language: {lang}"),
            Self::InvalidPattern(pattern) => write!(f, "invalid pattern: {pattern}"),
        }
    }
}

impl From<std::io::Error> for CodeSearchError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}

struct LanguageConfig {
    name: &'static str,
    language: Language,
    extensions: &'static [&'static str],
}

fn language_configs() -> Vec<LanguageConfig> {
    vec![
        LanguageConfig {
            name: "rust",
            language: tree_sitter_rust::LANGUAGE.into(),
            extensions: &["rs"],
        },
        LanguageConfig {
            name: "typescript",
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            extensions: &["ts", "tsx", "js"],
        },
        LanguageConfig {
            name: "python",
            language: tree_sitter_python::LANGUAGE.into(),
            extensions: &["py"],
        },
        LanguageConfig {
            name: "go",
            language: tree_sitter_go::LANGUAGE.into(),
            extensions: &["go"],
        },
    ]
}

#[allow(clippy::too_many_lines)]
fn queries_for_pattern(pattern: &str, lang: &str) -> Option<Vec<(String, String)>> {
    let pattern_lower = pattern.to_lowercase();
    match pattern_lower.as_str() {
        "function" | "def" | "func" => match lang {
            "rust" => Some(vec![(
                String::from("(function_item name: (identifier) @name)"),
                String::from("function"),
            )]),
            "typescript" => Some(vec![
                (
                    String::from("(function_declaration name: (identifier) @name)"),
                    String::from("function"),
                ),
                (
                    String::from("(method_definition name: (property_identifier) @name)"),
                    String::from("method"),
                ),
            ]),
            "python" => Some(vec![(
                String::from("(function_definition name: (identifier) @name)"),
                String::from("function"),
            )]),
            "go" => Some(vec![
                (
                    String::from("(function_declaration name: (identifier) @name)"),
                    String::from("function"),
                ),
                (
                    String::from("(method_declaration name: (field_identifier) @name)"),
                    String::from("method"),
                ),
            ]),
            _ => None,
        },
        "class" | "struct" => match lang {
            "rust" => Some(vec![
                (
                    String::from("(struct_item name: (type_identifier) @name)"),
                    String::from("struct"),
                ),
                (
                    String::from("(enum_item name: (type_identifier) @name)"),
                    String::from("enum"),
                ),
            ]),
            "typescript" => Some(vec![
                (
                    String::from("(class_declaration name: (type_identifier) @name)"),
                    String::from("class"),
                ),
                (
                    String::from("(interface_declaration name: (type_identifier) @name)"),
                    String::from("interface"),
                ),
            ]),
            "python" => Some(vec![(
                String::from("(class_definition name: (identifier) @name)"),
                String::from("class"),
            )]),
            "go" => Some(vec![(
                String::from("(type_spec name: (type_identifier) @name type: (struct_type)) @decl"),
                String::from("struct"),
            )]),
            _ => None,
        },
        "import" => match lang {
            "rust" => Some(vec![(
                String::from("(use_declaration argument: _ @name)"),
                String::from("import"),
            )]),
            "typescript" => Some(vec![
                (
                    String::from("(import_statement) @name"),
                    String::from("import"),
                ),
            ]),
            "python" => Some(vec![
                (
                    String::from("(import_statement name: (dotted_name (identifier) @name))"),
                    String::from("import"),
                ),
                (
                    String::from(
                        "(import_from_statement name: (dotted_name (identifier) @name))",
                    ),
                    String::from("import"),
                ),
            ]),
            "go" => Some(vec![(
                String::from("(import_declaration (import_spec) @name)"),
                String::from("import"),
            )]),
            _ => None,
        },
        "test" => match lang {
            "rust" => Some(vec![(
                String::from(
                    "(attribute_item (attribute name: (identifier) @attr)) @parent",
                ),
                String::from("test"),
            )]),
            "typescript" => Some(vec![
                (
                    String::from(
                        "(call_expression function: (identifier) @fn (#match? @fn \"^(test|it|describe)$\"))",
                    ),
                    String::from("test"),
                ),
                (
                    String::from(
                        "(function_declaration name: (identifier) @name (#match? @name \"_?test\"))",
                    ),
                    String::from("test_function"),
                ),
            ]),
            "python" => Some(vec![(
                String::from(
                    "(function_definition name: (identifier) @name (#match? @name \"^test_\"))",
                ),
                String::from("test"),
            )]),
            "go" => Some(vec![(
                String::from(
                    "(function_declaration name: (identifier) @name (#match? @name \"^Test\"))",
                ),
                String::from("test"),
            )]),
            _ => None,
        },
        "interface" => match lang {
            "rust" => Some(vec![(
                String::from("(trait_item name: (type_identifier) @name)"),
                String::from("trait"),
            )]),
            "typescript" => Some(vec![
                (
                    String::from("(interface_declaration name: (type_identifier) @name)"),
                    String::from("interface"),
                ),
                (
                    String::from("(type_alias_declaration name: (type_identifier) @name)"),
                    String::from("type_alias"),
                ),
            ]),
            "go" => Some(vec![(
                String::from(
                    "(type_spec name: (type_identifier) @name type: (interface_type)) @decl",
                ),
                String::from("interface"),
            )]),
            _ => None,
        },
        _ => None,
    }
}

fn extract_context(source: &str, target_line: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let total = lines.len();
    if total == 0 {
        return String::new();
    }

    let start = target_line.saturating_sub(3);
    let end = std::cmp::min(start + 5, total);

    lines[start..end].join("\n")
}

#[allow(clippy::too_many_lines)]
pub fn codesearch(
    pattern: &str,
    root_dir: &Path,
    languages: &[&str],
) -> Result<Vec<CodeSearchResult>, CodeSearchError> {
    if pattern.trim().is_empty() {
        return Err(CodeSearchError::InvalidPattern(String::from(
            "pattern must not be empty",
        )));
    }

    let all_configs = language_configs();
    let selected: Vec<&LanguageConfig> = if languages.is_empty() {
        all_configs.iter().collect()
    } else {
        languages
            .iter()
            .map(|&lang| {
                all_configs
                    .iter()
                    .find(|c| c.name == lang)
                    .ok_or_else(|| CodeSearchError::UnsupportedLanguage(lang.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    let has_any_queries = selected
        .iter()
        .any(|cfg| queries_for_pattern(pattern, cfg.name).is_some());
    if !has_any_queries {
        return Err(CodeSearchError::InvalidPattern(pattern.to_string()));
    }

    let extensions: std::collections::HashSet<&str> = selected
        .iter()
        .flat_map(|cfg| cfg.extensions.iter().copied())
        .collect();

    let walk = WalkBuilder::new(root_dir)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .hidden(false)
        .build();

    let mut results: Vec<CodeSearchResult> = Vec::new();

    for entry in walk {
        let entry = entry.map_err(std::io::Error::other)?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);
        let Some(ext) = ext else {
            continue;
        };

        if !extensions.contains(ext.as_str()) {
            continue;
        }

        let Some(lang_config) = selected
            .iter()
            .find(|cfg| cfg.extensions.iter().any(|&e| e == ext))
        else {
            continue;
        };

        let source = fs::read_to_string(path)?;

        let mut parser = Parser::new();
        parser
            .set_language(&lang_config.language)
            .map_err(|e| CodeSearchError::TreeSitterError(format!("set_language failed: {e}")))?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| CodeSearchError::TreeSitterError(String::from("parse returned None")))?;

        let root_node = tree.root_node();

        let Some(queries) = queries_for_pattern(pattern, lang_config.name) else {
            continue;
        };

        let file_path_str = path.to_string_lossy().to_string();

        for (query_str, symbol_type) in &queries {
            let query = Query::new(&lang_config.language, query_str).map_err(|e| {
                CodeSearchError::TreeSitterError(format!("query compile error: {e}"))
            })?;

            let mut cursor = QueryCursor::new();
            let mut matches_iter = cursor.matches(&query, root_node, source.as_bytes());

            while let Some(m) = matches_iter.next() {
                for capture in m.captures {
                    let capture_names = query.capture_names();
                    let capture_name = capture_names
                        .get(capture.index as usize)
                        .copied()
                        .unwrap_or("");
                    let capture_node = capture.node;

                    if pattern.to_lowercase() == "test" && lang_config.name == "rust" {
                        if capture_name == "attr" {
                            let attr_text = capture_node
                                .utf8_text(source.as_bytes())
                                .unwrap_or_default();
                            if attr_text != "test" {
                                continue;
                            }
                            let parent = capture_node.parent();
                            if let Some(parent_node) = parent {
                                let mut tree_cursor = parent_node.walk();
                                tree_cursor.goto_first_child();
                                loop {
                                    let child: tree_sitter::Node = tree_cursor.node();
                                    if child.kind() == "function_item" {
                                        if let Some(name_node) = child.child_by_field_name("name") {
                                            let sym = name_node
                                                .utf8_text(source.as_bytes())
                                                .unwrap_or_default();
                                            let line = name_node.start_position().row + 1;
                                            let col = name_node.start_position().column;
                                            let context = extract_context(&source, line);
                                            results.push(CodeSearchResult {
                                                file_path: file_path_str.clone(),
                                                line,
                                                column: col,
                                                symbol: sym.to_string(),
                                                symbol_type: String::from("test_function"),
                                                context,
                                                language: String::from("rust"),
                                            });
                                        }
                                    }
                                    if !tree_cursor.goto_next_sibling() {
                                        break;
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    if capture_name == "name" {
                        let sym = capture_node
                            .utf8_text(source.as_bytes())
                            .unwrap_or_default();
                        let line = capture_node.start_position().row + 1;
                        let col = capture_node.start_position().column;
                        let context = extract_context(&source, line);

                        results.push(CodeSearchResult {
                            file_path: file_path_str.clone(),
                            line,
                            column: col,
                            symbol: sym.to_string(),
                            symbol_type: symbol_type.clone(),
                            context,
                            language: lang_config.name.to_string(),
                        });
                    }
                }
            }
        }
    }

    results.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.line.cmp(&b.line))
    });

    results.dedup_by(|a, b| a.file_path == b.file_path && a.line == b.line && a.symbol == b.symbol);

    Ok(results)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{codesearch, CodeSearchError};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        std::env::temp_dir().join(format!("icode-codesearch-{name}-{unique}"))
    }

    fn setup_rust_test_dir() -> std::path::PathBuf {
        let dir = temp_dir("rust-funcs");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        std::fs::write(
            dir.join("main.rs"),
            r#"
fn hello() {
    println!("hello");
}

fn world() {
    println!("world");
}

struct MyStruct {
    field: i32,
}

#[test]
fn test_hello() {
    assert_eq!(hello(), "hello");
}

use std::collections::HashMap;
"#,
        )
        .expect("main.rs should write");

        dir
    }

    fn setup_typescript_test_dir() -> std::path::PathBuf {
        let dir = temp_dir("ts-imports");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        std::fs::write(
            dir.join("app.ts"),
            r"
import React from 'react';
import { useState, useEffect } from 'react';
import axios from 'axios';

function App() {
    return null;
}

class MyClass {
    value: number;
}

interface MyInterface {
    id: string;
}
",
        )
        .expect("app.ts should write");

        dir
    }

    fn setup_python_test_dir() -> std::path::PathBuf {
        let dir = temp_dir("py-classes");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        std::fs::write(
            dir.join("app.py"),
            r"
import os
import sys
from pathlib import Path

class MyClass:
    def __init__(self):
        pass

class AnotherClass:
        pass

def helper():
    pass

def test_my_function():
    assert True
",
        )
        .expect("app.py should write");

        dir
    }

    fn setup_go_test_dir() -> std::path::PathBuf {
        let dir = temp_dir("go-funcs");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        std::fs::write(
            dir.join("main.go"),
            r#"
package main

import (
    "fmt"
    "os"
)

func main() {
    fmt.Println("hello")
}

func helper() string {
    return "world"
}

func TestMain(m *testing.M) {
    os.Exit(m.Run())
}
"#,
        )
        .expect("main.go should write");

        dir
    }

    fn setup_gitignore_test_dir() -> std::path::PathBuf {
        let dir = temp_dir("gitignore");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        // Initialize git repo so .gitignore is respected
        std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .current_dir(&dir)
            .output()
            .expect("git init should succeed");

        std::fs::write(dir.join(".gitignore"), "ignored/\n").expect(".gitignore should write");

        std::fs::create_dir_all(dir.join("ignored")).expect("ignored dir should create");
        std::fs::write(
            dir.join("ignored").join("secret.rs"),
            "fn should_not_appear() {}",
        )
        .expect("secret.rs should write");

        std::fs::write(dir.join("visible.rs"), "fn visible_function() {}")
            .expect("visible.rs should write");

        dir
    }

    #[test]
    fn finds_functions_in_rust_file() {
        let dir = setup_rust_test_dir();

        let results = codesearch("function", &dir, &["rust"]).expect("codesearch should succeed");

        let func_results: Vec<_> = results
            .iter()
            .filter(|r| r.symbol_type == "function")
            .collect();

        assert!(!func_results.is_empty(), "should find Rust functions");
        let names: Vec<&str> = func_results.iter().map(|r| r.symbol.as_str()).collect();
        assert!(
            names.contains(&"hello"),
            "should find hello function, found: {names:?}"
        );
        assert!(
            names.contains(&"world"),
            "should find world function, found: {names:?}"
        );

        for r in &func_results {
            assert!(r.line >= 1, "line should be 1-based, got {}", r.line);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_imports_in_typescript_file() {
        let dir = setup_typescript_test_dir();

        let results =
            codesearch("import", &dir, &["typescript"]).expect("codesearch should succeed");

        assert!(
            !results.is_empty(),
            "should find TypeScript imports, got {results:?}"
        );

        let has_react = results.iter().any(|r| {
            r.symbol.to_lowercase().contains("react") || r.symbol.to_lowercase().contains("state")
        });
        assert!(
            has_react,
            "should find react-related imports, got: {results:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_classes_in_python_file() {
        let dir = setup_python_test_dir();

        let results = codesearch("class", &dir, &["python"]).expect("codesearch should succeed");

        let class_results: Vec<_> = results
            .iter()
            .filter(|r| r.symbol_type == "class")
            .collect();

        assert!(
            !class_results.is_empty(),
            "should find Python classes, got {results:?}"
        );

        let names: Vec<&str> = class_results.iter().map(|r| r.symbol.as_str()).collect();
        assert!(
            names.contains(&"MyClass"),
            "should find MyClass, found: {names:?}"
        );
        assert!(
            names.contains(&"AnotherClass"),
            "should find AnotherClass, found: {names:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_functions_in_go_file() {
        let dir = setup_go_test_dir();

        let results = codesearch("function", &dir, &["go"]).expect("codesearch should succeed");

        let func_results: Vec<_> = results
            .iter()
            .filter(|r| r.symbol_type == "function" || r.symbol_type == "method")
            .collect();

        assert!(
            !func_results.is_empty(),
            "should find Go functions, got {results:?}"
        );

        let names: Vec<&str> = func_results.iter().map(|r| r.symbol.as_str()).collect();
        assert!(
            names.contains(&"main"),
            "should find main function, found: {names:?}"
        );
        assert!(
            names.contains(&"helper"),
            "should find helper function, found: {names:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respects_gitignore() {
        let dir = setup_gitignore_test_dir();

        let results = codesearch("function", &dir, &["rust"]).expect("codesearch should succeed");

        let has_visible = results.iter().any(|r| r.symbol == "visible_function");
        assert!(
            has_visible,
            "should find visible_function, got: {results:?}"
        );

        let has_ignored = results.iter().any(|r| r.symbol == "should_not_appear");
        assert!(
            !has_ignored,
            "should NOT find should_not_appear (gitignored), got: {results:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_pattern_returns_error() {
        let dir = setup_rust_test_dir();

        let result = codesearch("nonexistent_pattern_xyz", &dir, &["rust"]);

        assert!(
            result.is_err(),
            "invalid pattern should return error, got: {result:?}"
        );

        match result {
            Err(CodeSearchError::InvalidPattern(_)) => {}
            other => panic!("expected InvalidPattern error, got: {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_language_returns_error() {
        let dir = setup_rust_test_dir();

        let result = codesearch("function", &dir, &["cobol"]);

        assert!(
            result.is_err(),
            "unsupported language should return error, got: {result:?}"
        );

        match result {
            Err(CodeSearchError::UnsupportedLanguage(lang)) => {
                assert_eq!(lang, "cobol");
            }
            other => panic!("expected UnsupportedLanguage error, got: {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn results_are_sorted_by_file_then_line() {
        let dir = setup_rust_test_dir();

        let results = codesearch("function", &dir, &["rust"]).expect("codesearch should succeed");

        for i in 1..results.len() {
            let prev = &results[i - 1];
            let curr = &results[i];
            assert!(
                prev.file_path < curr.file_path
                    || (prev.file_path == curr.file_path && prev.line <= curr.line),
                "results should be sorted: {prev:?} before {curr:?}"
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_pattern_returns_error() {
        let dir = setup_rust_test_dir();

        let result = codesearch("", &dir, &["rust"]);

        assert!(result.is_err(), "empty pattern should return error");
        match result {
            Err(CodeSearchError::InvalidPattern(_)) => {}
            other => panic!("expected InvalidPattern error, got: {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
