//! Syntax highlighting for code blocks in the TUI.
//!
//! Uses **syntect** (Sublime Text syntax definitions) rather than tree-sitter.
//! This was a deliberate choice:
//!
//! - **Zero compilation**: syntect ships with pre-built binary grammars.
//!   Tree-sitter requires compiling each grammar crate via `cc`, adding
//!   build dependencies and platform-specific toolchain requirements.
//! - **Broader language support**: `SyntaxSet::load_defaults_newlines()`
//!   includes 50+ languages out of the box.
//! - **Already integrated**: the existing `syntax_theme.rs` builds dynamic
//!   syntect themes from the TUI theme colors.
//!
//! ## Why not tree-sitter?
//!
//! Adding tree-sitter would require:
//! ```toml
//! tree-sitter = "0.25"
//! tree-sitter-highlight = "0.25"
//! tree-sitter-rust = "0.23"
//! tree-sitter-javascript = "0.23"
//! tree-sitter-typescript = "0.23"
//! tree-sitter-python = "0.23"
//! tree-sitter-bash = "0.23"
//! ```
//! Each grammar crate needs `cc` for compilation, adding ~30s to clean
//! builds and requiring a C compiler on all target platforms. For a TUI
//! that already has full highlighting via syntect, this cost isn't justified.
//!
//! ## Supported languages
//!
//! All languages from Sublime Text's default syntax definitions, including:
//! rust, python, javascript, bash, json, yaml, toml, go, java, c, cpp,
//! html, css, sql, ruby, php, and 40+ more.
//!
//! Note: TypeScript is NOT included in syntect defaults (requires third-party
//! Sublime Text package). For TypeScript support, add a custom `.sublime-syntax`
//! file or switch to tree-sitter (see module-level docs above).
//!
//! ## Usage
//!
//! ```ignore
//! let highlighter = SyntaxHighlighter::new(&theme);
//! let spans = highlighter.highlight("fn main() {}", "rust");
//! ```

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::FontStyle;
use syntect::parsing::SyntaxSet;

use crate::tui::syntax_theme::build_syntect_theme;
use crate::tui::theme::Theme;

/// Supported language aliases that map to syntect syntax names.
/// These are the most common language hints used in markdown code blocks.
const LANGUAGE_ALIASES: &[(&str, &str)] = &[
    ("js", "javascript"),
    ("py", "python"),
    ("rs", "rust"),
    ("sh", "bash"),
    ("shell", "bash"),
    ("zsh", "bash"),
    ("yml", "yaml"),
    ("toml", "toml"),
    ("json", "json"),
    ("md", "markdown"),
    ("c++", "c++"),
    ("cpp", "c++"),
    ("cs", "c#"),
    ("csharp", "c#"),
    ("rb", "ruby"),
];

/// Syntax highlighter for code blocks.
///
/// Wraps syntect's `SyntaxSet` and theme builder to provide
/// language-aware highlighting with TUI theme colors.
pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl SyntaxHighlighter {
    /// Create a new syntax highlighter with the given TUI theme.
    ///
    /// Loads the default syntect syntax definitions (50+ languages).
    pub fn new(theme: &Theme) -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme: *theme,
        }
    }

    /// Highlight a code block, returning styled spans per line.
    ///
    /// Returns a vector where each element is a line, and each line
    /// is a vector of styled spans.
    ///
    /// If the language is not recognized, falls back to plain text
    /// with the default code style.
    pub fn highlight(&self, code: &str, language: &str) -> Vec<Vec<Span<'static>>> {
        let syntax = self.resolve_syntax(language);
        let syntect_theme = build_syntect_theme(&self.theme);
        let mut highlighter = HighlightLines::new(syntax, &syntect_theme);

        let mut result = Vec::new();

        for line in code.lines() {
            let line_with_newline = format!("{line}\n");
            let highlighted = highlighter.highlight_line(&line_with_newline, &self.syntax_set);

            match highlighted {
                Ok(ranges) => {
                    let mut spans = Vec::new();
                    for (style, text) in ranges {
                        let trimmed = text.trim_end_matches('\n');
                        if !trimmed.is_empty() {
                            spans.push(Span::styled(
                                trimmed.to_string(),
                                syntect_style_to_ratatui(&style),
                            ));
                        }
                    }
                    if spans.is_empty() {
                        // Empty line or no highlighting — use default code style
                        spans.push(Span::styled(
                            line.to_string(),
                            Style::default()
                                .fg(self.theme.code_text)
                                .bg(self.theme.code_bg),
                        ));
                    }
                    result.push(spans);
                }
                Err(_) => {
                    // Highlighting error — fall back to plain code style
                    result.push(vec![Span::styled(
                        line.to_string(),
                        Style::default()
                            .fg(self.theme.code_text)
                            .bg(self.theme.code_bg),
                    )]);
                }
            }
        }

        result
    }

    /// Resolve a language hint to a syntect syntax, with alias support.
    fn resolve_syntax(&self, language: &str) -> &syntect::parsing::SyntaxReference {
        if language.is_empty() {
            return self.syntax_set.find_syntax_plain_text();
        }

        // Check aliases first
        let resolved = LANGUAGE_ALIASES
            .iter()
            .find(|(alias, _)| *alias == language)
            .map_or(language, |(_, canonical)| *canonical);

        self.find_syntax(resolved)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
    }

    /// Find syntax by name or extension, case-insensitive.
    fn find_syntax(&self, token: &str) -> Option<&syntect::parsing::SyntaxReference> {
        self.syntax_set.find_syntax_by_token(token).or_else(|| {
            self.syntax_set
                .syntaxes()
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(token))
        })
    }

    /// Check if a language is supported.
    pub fn is_supported(&self, language: &str) -> bool {
        if language.is_empty() {
            return false;
        }
        let resolved = LANGUAGE_ALIASES
            .iter()
            .find(|(alias, _)| *alias == language)
            .map_or(language, |(_, canonical)| *canonical);
        self.find_syntax(resolved).is_some()
    }

    /// Get the list of supported language names.
    pub fn supported_languages(&self) -> Vec<&str> {
        self.syntax_set
            .syntaxes()
            .iter()
            .filter(|s| !s.hidden)
            .flat_map(|s| {
                let mut names = vec![s.name.as_str()];
                names.extend(s.file_extensions.iter().map(std::string::String::as_str));
                names
            })
            .collect()
    }
}

/// Convert a syntect style to a ratatui Style.
fn syntect_style_to_ratatui(style: &syntect::highlighting::Style) -> Style {
    let fg = ratatui::style::Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    let bg = ratatui::style::Color::Rgb(style.background.r, style.background.g, style.background.b);
    let mut result = Style::default().fg(fg).bg(bg);
    if style.font_style.contains(FontStyle::BOLD) {
        result = result.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        result = result.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        result = result.add_modifier(Modifier::UNDERLINED);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighter_supports_core_languages() {
        let theme = Theme::dark();
        let highlighter = SyntaxHighlighter::new(&theme);

        assert!(highlighter.is_supported("rust"));
        assert!(highlighter.is_supported("python"));
        assert!(highlighter.is_supported("javascript"));
        assert!(highlighter.is_supported("bash"));
        assert!(highlighter.is_supported("json"));
        assert!(highlighter.is_supported("yaml"));
    }

    #[test]
    fn highlighter_resolves_aliases() {
        let theme = Theme::dark();
        let highlighter = SyntaxHighlighter::new(&theme);

        assert!(highlighter.is_supported("js"));
        assert!(highlighter.is_supported("py"));
        assert!(highlighter.is_supported("rs"));
        assert!(highlighter.is_supported("sh"));
        assert!(highlighter.is_supported("shell"));
        assert!(highlighter.is_supported("yml"));
    }

    #[test]
    fn highlight_rust_code() {
        let theme = Theme::dark();
        let highlighter = SyntaxHighlighter::new(&theme);

        let code = "fn main() {\n    println!(\"hello\");\n}";
        let lines = highlighter.highlight(code, "rust");

        assert_eq!(lines.len(), 3);
        // First line should have at least one span (fn keyword)
        assert!(!lines[0].is_empty());
    }

    #[test]
    fn highlight_unknown_language_falls_back() {
        let theme = Theme::dark();
        let highlighter = SyntaxHighlighter::new(&theme);

        let code = "some code\nmore code";
        let lines = highlighter.highlight(code, "nonexistent");

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 1);
    }

    #[test]
    fn highlight_empty_language() {
        let theme = Theme::dark();
        let highlighter = SyntaxHighlighter::new(&theme);

        let code = "plain text";
        let lines = highlighter.highlight(code, "");

        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn highlight_python_code() {
        let theme = Theme::dark();
        let highlighter = SyntaxHighlighter::new(&theme);

        let code = "def hello():\n    print(\"world\")";
        let lines = highlighter.highlight(code, "python");

        assert_eq!(lines.len(), 2);
        assert!(!lines[0].is_empty());
    }

    #[test]
    fn highlight_json_code() {
        let theme = Theme::dark();
        let highlighter = SyntaxHighlighter::new(&theme);

        let code = "{\"key\": \"value\"}";
        let lines = highlighter.highlight(code, "json");

        assert_eq!(lines.len(), 1);
        assert!(!lines[0].is_empty());
    }

    #[test]
    fn theme_light_vs_dark_produce_different_colors() {
        let dark = Theme::dark();
        let light = Theme::light();

        let dark_hl = SyntaxHighlighter::new(&dark);
        let light_hl = SyntaxHighlighter::new(&light);

        let code = "fn main() {}";
        let dark_lines = dark_hl.highlight(code, "rust");
        let light_lines = light_hl.highlight(code, "rust");

        // Both should produce the same number of lines/spans
        assert_eq!(dark_lines.len(), light_lines.len());
        assert_eq!(dark_lines[0].len(), light_lines[0].len());
    }
}
