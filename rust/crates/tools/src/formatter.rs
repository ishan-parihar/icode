//! Language-aware code formatting tool.
//!
//! Auto-detects language from file extension and dispatches to the appropriate
//! external formatter (rustfmt, prettier, black, gofmt, etc.). Returns a
//! structured result with before/after line counts and change status.
//!
//! If the target formatter is not installed, returns a warning result rather
//! than an error, allowing graceful degradation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FormatterInput {
    pub path: String,
    pub language: Option<String>,
}

/// Output describing the result of a formatting operation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FormatterOutput {
    pub success: bool,
    pub path: String,
    pub language: String,
    pub formatter_used: String,
    pub lines_before: usize,
    pub lines_after: usize,
    pub changed: bool,
}

fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        "py" => Some("python"),
        "go" => Some("go"),
        "rb" => Some("ruby"),
        "java" => Some("java"),
        "c" | "cpp" | "h" | "hpp" | "cc" | "cxx" => Some("cpp"),
        "css" => Some("css"),
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        _ => None,
    }
}

fn detect_language(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| extension_to_language(ext))
        .flatten()
        .map(String::from)
}

struct FormatterCommand {
    binary: &'static str,
    args: &'static [&'static str],
}

fn formatter_for_language(language: &str) -> Option<FormatterCommand> {
    match language {
        "rust" => Some(FormatterCommand {
            binary: "rustfmt",
            args: &[],
        }),
        "typescript" => Some(FormatterCommand {
            binary: "prettier",
            args: &["--parser", "typescript"],
        }),
        "javascript" => Some(FormatterCommand {
            binary: "prettier",
            args: &["--parser", "babel"],
        }),
        "python" => Some(FormatterCommand {
            binary: "black",
            args: &["--quiet", "--line-length", "100"],
        }),
        "go" => Some(FormatterCommand {
            binary: "gofmt",
            args: &["-w"],
        }),
        "ruby" => Some(FormatterCommand {
            binary: "rubocop",
            args: &["-a"],
        }),
        "java" => Some(FormatterCommand {
            binary: "google-java-format",
            args: &["-i"],
        }),
        "cpp" => Some(FormatterCommand {
            binary: "clang-format",
            args: &["-i"],
        }),
        "css" => Some(FormatterCommand {
            binary: "prettier",
            args: &["--parser", "css"],
        }),
        "json" => Some(FormatterCommand {
            binary: "prettier",
            args: &["--parser", "json"],
        }),
        "yaml" => Some(FormatterCommand {
            binary: "prettier",
            args: &["--parser", "yaml"],
        }),
        _ => None,
    }
}

fn count_lines(path: &str) -> usize {
    fs::read_to_string(path)
        .map(|content| {
            if content.is_empty() {
                0
            } else {
                content.lines().count()
            }
        })
        .unwrap_or(0)
}

/// Execute the formatter on the given file.
///
/// Returns `Ok(FormatterOutput)` even when the formatter binary is not found —
/// in that case `success` is `false` and the output carries a warning message
/// in the `formatter_used` field.
pub fn execute_formatter(input: FormatterInput) -> Result<FormatterOutput, String> {
    let path = input.path.clone();

    let language = input
        .language
        .clone()
        .or_else(|| detect_language(&path))
        .ok_or_else(|| {
            format!(
                "could not determine language for file: {path} (unknown extension, specify language manually)"
            )
        })?;

    let Some(fmt_cmd) = formatter_for_language(&language) else {
        return Ok(FormatterOutput {
            success: false,
            path: path.clone(),
            language: language.clone(),
            formatter_used: format!("formatter not installed (language: {language})"),
            lines_before: 0,
            lines_after: 0,
            changed: false,
        });
    };

    if Command::new("which").arg(fmt_cmd.binary).output().is_err()
        || !Command::new("which")
            .arg(fmt_cmd.binary)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        return Ok(FormatterOutput {
            success: false,
            path: path.clone(),
            language: language.clone(),
            formatter_used: format!("formatter not installed ({})", fmt_cmd.binary),
            lines_before: count_lines(&path),
            lines_after: 0,
            changed: false,
        });
    }

    let lines_before = count_lines(&path);

    let mut cmd = Command::new(fmt_cmd.binary);
    for arg in fmt_cmd.args {
        cmd.arg(arg);
    }

    if fmt_cmd.binary == "prettier" {
        cmd.arg("--write");
    }

    cmd.arg(&path);

    let output = cmd.output().map_err(|e| {
        format!(
            "failed to execute formatter '{}' for {path}: {e}",
            fmt_cmd.binary
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "formatter '{}' exited with errors for {path}: {stderr}",
            fmt_cmd.binary
        ));
    }

    let lines_after = count_lines(&path);
    let changed = lines_before != lines_after;

    Ok(FormatterOutput {
        success: true,
        path: path.clone(),
        language: language.clone(),
        formatter_used: fmt_cmd.binary.to_string(),
        lines_before,
        lines_after,
        changed,
    })
}

pub fn formatter_tool_spec() -> Value {
    serde_json::to_value(schemars::schema_for!(FormatterInput)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rust_from_rs_extension() {
        assert_eq!(detect_language("src/main.rs"), Some("rust".to_string()));
    }

    #[test]
    fn detects_typescript_from_ts_extension() {
        assert_eq!(detect_language("app.ts"), Some("typescript".to_string()));
    }

    #[test]
    fn detects_typescript_from_tsx_extension() {
        assert_eq!(
            detect_language("component.tsx"),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn detects_javascript_from_js_extension() {
        assert_eq!(detect_language("index.js"), Some("javascript".to_string()));
    }

    #[test]
    fn detects_javascript_from_jsx_extension() {
        assert_eq!(
            detect_language("component.jsx"),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn detects_python_from_py_extension() {
        assert_eq!(detect_language("script.py"), Some("python".to_string()));
    }

    #[test]
    fn detects_go_from_go_extension() {
        assert_eq!(detect_language("main.go"), Some("go".to_string()));
    }

    #[test]
    fn detects_ruby_from_rb_extension() {
        assert_eq!(detect_language("app.rb"), Some("ruby".to_string()));
    }

    #[test]
    fn detects_java_from_java_extension() {
        assert_eq!(detect_language("Main.java"), Some("java".to_string()));
    }

    #[test]
    fn detects_cpp_from_c_extension() {
        assert_eq!(detect_language("main.c"), Some("cpp".to_string()));
    }

    #[test]
    fn detects_cpp_from_cpp_extension() {
        assert_eq!(detect_language("main.cpp"), Some("cpp".to_string()));
    }

    #[test]
    fn detects_cpp_from_h_extension() {
        assert_eq!(detect_language("header.h"), Some("cpp".to_string()));
    }

    #[test]
    fn detects_css_from_css_extension() {
        assert_eq!(detect_language("styles.css"), Some("css".to_string()));
    }

    #[test]
    fn detects_json_from_json_extension() {
        assert_eq!(detect_language("data.json"), Some("json".to_string()));
    }

    #[test]
    fn detects_yaml_from_yaml_extension() {
        assert_eq!(detect_language("config.yaml"), Some("yaml".to_string()));
    }

    #[test]
    fn detects_yaml_from_yml_extension() {
        assert_eq!(detect_language("config.yml"), Some("yaml".to_string()));
    }

    #[test]
    fn returns_none_for_unknown_extension() {
        assert_eq!(detect_language("file.xyz"), None);
    }

    #[test]
    fn returns_none_for_no_extension() {
        assert_eq!(detect_language("Makefile"), None);
    }

    #[test]
    fn maps_rust_to_rustfmt() {
        let fmt = formatter_for_language("rust");
        assert!(fmt.is_some());
        assert_eq!(fmt.unwrap().binary, "rustfmt");
    }

    #[test]
    fn maps_python_to_black() {
        let fmt = formatter_for_language("python");
        assert!(fmt.is_some());
        assert_eq!(fmt.unwrap().binary, "black");
    }

    #[test]
    fn maps_go_to_gofmt() {
        let fmt = formatter_for_language("go");
        assert!(fmt.is_some());
        assert_eq!(fmt.unwrap().binary, "gofmt");
    }

    #[test]
    fn maps_typescript_to_prettier_with_typescript_parser() {
        let fmt = formatter_for_language("typescript");
        assert!(fmt.is_some());
        let f = fmt.unwrap();
        assert_eq!(f.binary, "prettier");
        assert!(f.args.contains(&"typescript"));
    }

    #[test]
    fn maps_javascript_to_prettier_with_babel_parser() {
        let fmt = formatter_for_language("javascript");
        assert!(fmt.is_some());
        let f = fmt.unwrap();
        assert_eq!(f.binary, "prettier");
        assert!(f.args.contains(&"babel"));
    }

    #[test]
    fn maps_unknown_language_to_none() {
        assert!(formatter_for_language("brainfuck").is_none());
    }

    #[test]
    fn returns_warning_when_formatter_not_installed() {
        let input = FormatterInput {
            path: "test.java".to_string(),
            language: Some("java".to_string()),
        };
        let result = execute_formatter(input);
        assert!(result.is_ok());
        let output = result.unwrap();
        if !output.success {
            assert!(output.formatter_used.contains("formatter not installed"));
        }
    }

    #[test]
    fn returns_error_for_unknown_language() {
        let input = FormatterInput {
            path: "test.unknown".to_string(),
            language: None,
        };
        let result = execute_formatter(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("could not determine language"));
    }

    #[test]
    fn uses_explicit_language_when_provided() {
        let dir = std::env::temp_dir().join(format!("formatter_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("somefile.xyz");
        std::fs::write(&path, "fn main() {}\n").expect("write temp file");

        let input = FormatterInput {
            path: path.to_string_lossy().to_string(),
            language: Some("rust".to_string()),
        };
        let result = execute_formatter(input);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.language, "rust");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn formatter_tool_spec_has_required_path() {
        let spec = formatter_tool_spec();
        let required = spec["required"].as_array();
        assert!(required.is_some());
        let required = required.unwrap();
        assert!(required.contains(&json!("path")));
    }

    #[test]
    fn formatter_tool_spec_has_properties() {
        let spec = formatter_tool_spec();
        let props = spec["properties"].as_object();
        assert!(props.is_some());
        let props = props.unwrap();
        assert!(props.contains_key("path"));
        assert!(props.contains_key("language"));
    }
}
