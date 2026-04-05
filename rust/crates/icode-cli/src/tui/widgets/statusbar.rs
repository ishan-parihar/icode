use crate::tui::app::AppState;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct StatusBar;

impl StatusBar {
    pub const fn new() -> Self {
        Self
    }

    pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
        let mut left_spans = vec![
            Span::styled(
                "\u{2022}",
                Style::default().fg(if state.connected {
                    state.theme.success
                } else {
                    state.theme.text_muted
                }),
            ),
            Span::raw(" "),
            Span::styled(&state.cwd, Style::default().fg(state.theme.text_muted)),
        ];

        if let Some(ref branch) = state.git_branch {
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(
                if state.git_dirty {
                    "\u{25b2}"
                } else {
                    "\u{2022}"
                },
                Style::default().fg(if state.git_dirty {
                    state.theme.warning
                } else {
                    state.theme.text_muted
                }),
            ));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(
                branch,
                Style::default().fg(state.theme.text_muted),
            ));
        }

        let mut right_spans = Vec::new();

        if state.lsp_count > 0 {
            right_spans.push(Span::styled(
                "\u{2022}",
                Style::default().fg(state.theme.success),
            ));
            right_spans.push(Span::raw(" "));
            right_spans.push(Span::styled(
                format!("{} LSP", state.lsp_count),
                Style::default().fg(state.theme.text),
            ));
            right_spans.push(Span::raw("  "));
        }

        if state.mcp_count > 0 {
            right_spans.push(Span::styled(
                "\u{2299}",
                Style::default().fg(state.theme.success),
            ));
            right_spans.push(Span::raw(" "));
            right_spans.push(Span::styled(
                format!("{} MCP", state.mcp_count),
                Style::default().fg(state.theme.text),
            ));
            right_spans.push(Span::raw("  "));
        }

        if state.is_streaming {
            right_spans.push(Span::styled(
                "Working...",
                Style::default()
                    .fg(state.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            right_spans.push(Span::styled(
                "Tab",
                Style::default()
                    .fg(state.theme.text)
                    .add_modifier(Modifier::BOLD),
            ));
            right_spans.push(Span::styled(
                " complete  ",
                Style::default().fg(state.theme.text_muted),
            ));
            right_spans.push(Span::styled(
                "Enter",
                Style::default()
                    .fg(state.theme.text)
                    .add_modifier(Modifier::BOLD),
            ));
            right_spans.push(Span::styled(
                " send",
                Style::default().fg(state.theme.text_muted),
            ));
        }

        let left_line = Line::from(left_spans);
        let right_line = Line::from(right_spans);

        let width = area.width as usize;
        let left_text: String = line_width(&left_line);
        let right_text: String = line_width(&right_line);
        let padding = width.saturating_sub(left_text.len() + right_text.len());

        let mut combined = Vec::new();
        combined.extend(left_line.spans);
        combined.push(Span::raw(" ".repeat(padding)));
        combined.extend(right_line.spans);

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::TOP)
            .border_style(Style::default().fg(state.theme.border));

        let paragraph = Paragraph::new(Line::from(combined)).block(block);
        frame.render_widget(paragraph, area);
    }
}

fn line_width(line: &Line) -> String {
    let mut s = String::new();
    for span in &line.spans {
        s.push_str(&span.content);
    }
    s
}
