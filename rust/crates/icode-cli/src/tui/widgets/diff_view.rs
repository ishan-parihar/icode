use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme::Theme;

/// Represents a single line in a unified git diff.
#[derive(Debug, Clone)]
pub enum DiffLine {
    /// Lines starting with `+` (but not `+++`)
    Added(String),
    /// Lines starting with `-` (but not `---`)
    Removed(String),
    /// Context lines starting with space
    Context(String),
    /// File headers: `diff --git`, `---`, `+++`
    Header(String),
    /// Hunk headers: `@@ ... @@`
    HunkHeader(String),
    /// Meta lines: `index`, `old mode`, `new mode`, etc.
    Meta(String),
}

/// A widget that renders colored git diff output.
pub struct DiffView {
    pub lines: Vec<DiffLine>,
    pub scroll: usize,
    pub title: String,
}

impl DiffView {
    /// Create a new empty `DiffView`.
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll: 0,
            title: String::from("Diff"),
        }
    }

    /// Parse unified git diff output into a vector of `DiffLine`.
    pub fn parse(diff: &str) -> Vec<DiffLine> {
        let mut result = Vec::new();

        for line in diff.lines() {
            if line.starts_with("diff --git") {
                result.push(DiffLine::Header(line.to_string()));
            } else if line.starts_with("+++") {
                result.push(DiffLine::Header(line.to_string()));
            } else if line.starts_with("---") {
                result.push(DiffLine::Header(line.to_string()));
            } else if line.starts_with("@@") {
                result.push(DiffLine::HunkHeader(line.to_string()));
            } else if line.starts_with('+') {
                result.push(DiffLine::Added(line.to_string()));
            } else if line.starts_with('-') {
                result.push(DiffLine::Removed(line.to_string()));
            } else if line.starts_with(' ') {
                result.push(DiffLine::Context(line.to_string()));
            } else if line.starts_with("index")
                || line.starts_with("old mode")
                || line.starts_with("new mode")
                || line.starts_with("new file mode")
                || line.starts_with("deleted file mode")
                || line.starts_with("similarity index")
                || line.starts_with("rename from")
                || line.starts_with("rename to")
                || line.starts_with("copy from")
                || line.starts_with("copy to")
            {
                result.push(DiffLine::Meta(line.to_string()));
            } else if line.is_empty() {
                result.push(DiffLine::Context(String::new()));
            } else {
                // Any other line (e.g., "Staged changes:", "Unstaged changes:")
                result.push(DiffLine::Header(line.to_string()));
            }
        }

        result
    }

    /// Parse diff output and create a `DiffView` with auto-populated lines.
    pub fn from_diff(diff: &str, title: &str) -> Self {
        Self {
            lines: Self::parse(diff),
            scroll: 0,
            title: title.to_string(),
        }
    }

    /// Scroll down by one line.
    pub fn scroll_down(&mut self) {
        if self.scroll < self.lines.len().saturating_sub(1) {
            self.scroll += 1;
        }
    }

    /// Scroll up by one line.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down by half a page.
    pub fn scroll_page_down(&mut self, height: usize) {
        let step = (height / 2).max(1);
        self.scroll = (self.scroll + step).min(self.lines.len().saturating_sub(1));
    }

    /// Scroll up by half a page.
    pub fn scroll_page_up(&mut self, height: usize) {
        let step = (height / 2).max(1);
        self.scroll = self.scroll.saturating_sub(step);
    }

    /// Go to top.
    pub fn go_to_top(&mut self) {
        self.scroll = 0;
    }

    /// Go to bottom.
    pub fn go_to_bottom(&mut self, height: usize) {
        self.scroll = self.lines.len().saturating_sub(height);
    }

    /// Render the diff view within the given area.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let visible_lines = area.height as usize;
        if visible_lines == 0 {
            return;
        }

        let start = self.scroll;
        let end = (start + visible_lines).min(self.lines.len());

        let mut rendered_lines: Vec<Line<'static>> = Vec::with_capacity(visible_lines);

        for line in self.lines.iter().skip(start).take(visible_lines) {
            let styled_line = match line {
                DiffLine::Added(text) => Line::from(Span::styled(
                    text.clone(),
                    Style::default()
                        .fg(theme.diff_added)
                        .add_modifier(Modifier::BOLD),
                )),
                DiffLine::Removed(text) => Line::from(Span::styled(
                    text.clone(),
                    Style::default()
                        .fg(theme.diff_removed)
                        .add_modifier(Modifier::BOLD),
                )),
                DiffLine::Context(text) => {
                    Line::from(Span::styled(text.clone(), Style::default().fg(theme.text)))
                }
                DiffLine::Header(text) => {
                    if text.starts_with("diff --git") {
                        Line::from(Span::styled(
                            text.clone(),
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD),
                        ))
                    } else {
                        // --- or +++ lines
                        Line::from(Span::styled(
                            text.clone(),
                            Style::default()
                                .fg(if text.starts_with("+++") {
                                    theme.diff_added
                                } else {
                                    theme.diff_removed
                                })
                                .add_modifier(Modifier::BOLD),
                        ))
                    }
                }
                DiffLine::HunkHeader(text) => {
                    Line::from(Span::styled(text.clone(), Style::default().fg(theme.info)))
                }
                DiffLine::Meta(text) => Line::from(Span::styled(
                    text.clone(),
                    Style::default().fg(theme.text_muted),
                )),
            };
            rendered_lines.push(styled_line);
        }

        // Fill remaining visible area with empty lines if diff is shorter
        while rendered_lines.len() < visible_lines {
            rendered_lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(rendered_lines);
        frame.render_widget(paragraph, area);
    }
}

impl Default for DiffView {
    fn default() -> Self {
        Self::new()
    }
}

/// Render colored diff output for REPL mode using ANSI escape codes.
pub fn render_colored_diff(diff: &str) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            let _ = writeln!(output, "\x1b[1;35m{line}\x1b[0m");
        } else if line.starts_with("+++") {
            let _ = writeln!(output, "\x1b[1;32m{line}\x1b[0m");
        } else if line.starts_with("---") {
            let _ = writeln!(output, "\x1b[1;31m{line}\x1b[0m");
        } else if line.starts_with("@@") {
            let _ = writeln!(output, "\x1b[36m{line}\x1b[0m");
        } else if line.starts_with('+') {
            let _ = writeln!(output, "\x1b[32m{line}\x1b[0m");
        } else if line.starts_with('-') {
            let _ = writeln!(output, "\x1b[31m{line}\x1b[0m");
        } else if line.starts_with(' ') {
            let _ = writeln!(output, "\x1b[0m{line}\x1b[0m");
        } else if line.starts_with("index")
            || line.starts_with("old mode")
            || line.starts_with("new mode")
            || line.starts_with("new file mode")
            || line.starts_with("deleted file mode")
            || line.starts_with("similarity index")
            || line.starts_with("rename from")
            || line.starts_with("rename to")
        {
            let _ = writeln!(output, "\x1b[2m{line}\x1b[0m");
        } else if line.is_empty() {
            output.push('\n');
        } else {
            let _ = writeln!(output, "\x1b[1m{line}\x1b[0m");
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_diff() {
        let lines = DiffView::parse("");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_parse_added_line() {
        let lines = DiffView::parse("+added line");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::Added(s) => assert_eq!(s, "+added line"),
            _ => panic!("Expected Added variant"),
        }
    }

    #[test]
    fn test_parse_removed_line() {
        let lines = DiffView::parse("-removed line");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::Removed(s) => assert_eq!(s, "-removed line"),
            _ => panic!("Expected Removed variant"),
        }
    }

    #[test]
    fn test_parse_context_line() {
        let lines = DiffView::parse(" context line");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::Context(s) => assert_eq!(s, " context line"),
            _ => panic!("Expected Context variant"),
        }
    }

    #[test]
    fn test_parse_hunk_header() {
        let lines = DiffView::parse("@@ -1,3 +1,4 @@");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::HunkHeader(s) => assert_eq!(s, "@@ -1,3 +1,4 @@"),
            _ => panic!("Expected HunkHeader variant"),
        }
    }

    #[test]
    fn test_parse_file_header() {
        let lines = DiffView::parse("diff --git a/file.txt b/file.txt");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::Header(s) => assert_eq!(s, "diff --git a/file.txt b/file.txt"),
            _ => panic!("Expected Header variant"),
        }
    }

    #[test]
    fn test_parse_plus_plus_plus_as_header() {
        let lines = DiffView::parse("+++ b/file.txt");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::Header(s) => assert_eq!(s, "+++ b/file.txt"),
            _ => panic!("Expected Header variant for +++"),
        }
    }

    #[test]
    fn test_parse_minus_minus_minus_as_header() {
        let lines = DiffView::parse("--- a/file.txt");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::Header(s) => assert_eq!(s, "--- a/file.txt"),
            _ => panic!("Expected Header variant for ---"),
        }
    }

    #[test]
    fn test_parse_meta_line() {
        let lines = DiffView::parse("index 1234567..abcdefg 100644");
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DiffLine::Meta(s) => assert_eq!(s, "index 1234567..abcdefg 100644"),
            _ => panic!("Expected Meta variant"),
        }
    }

    #[test]
    fn test_scroll_bounds() {
        let mut view = DiffView::new();
        view.lines = vec![
            DiffLine::Context("line 1".into()),
            DiffLine::Added("+line 2".into()),
            DiffLine::Removed("-line 3".into()),
        ];

        // Scroll down should work
        view.scroll_down();
        assert_eq!(view.scroll, 1);
        view.scroll_down();
        assert_eq!(view.scroll, 2);
        // Should not go past end
        view.scroll_down();
        assert_eq!(view.scroll, 2);

        // Scroll up should work
        view.scroll_up();
        assert_eq!(view.scroll, 1);
        view.scroll_up();
        assert_eq!(view.scroll, 0);
        // Should not go below 0
        view.scroll_up();
        assert_eq!(view.scroll, 0);
    }

    #[test]
    fn test_go_to_top_and_bottom() {
        let mut view = DiffView::new();
        for i in 0..20 {
            view.lines.push(DiffLine::Context(format!("line {i}")));
        }
        view.scroll = 10;

        view.go_to_top();
        assert_eq!(view.scroll, 0);

        view.go_to_bottom(5);
        assert_eq!(view.scroll, 15);
    }
}
