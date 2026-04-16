use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::tui::theme::Theme;

pub struct HomeScreenState {
    pub logo_lines: Vec<&'static str>,
}

impl HomeScreenState {
    pub fn new() -> Self {
        let logo_lines = vec![
            "╔══════════════════════════════════════════════╗",
            "║     ██╗ ██████╗ ██████╗ ██████╗ ███████╗     ║",
            "║     ╚═╝██╔════╝██╔═══██╗██╔══██╗██╔════╝     ║",
            "║     ██╗██║     ██║   ██║██║  ██║█████╗       ║",
            "║     ██║██║     ██║   ██║██║  ██║██╔══╝       ║",
            "║     ██║╚██████╗╚██████╔╝██████╔╝███████╗     ║",
            "║     ╚═╝ ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝     ║",
            "╚══════════════════════════════════════════════╝",
        ];
        Self { logo_lines }
    }
}

impl Default for HomeScreenState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_home_content(frame: &mut Frame, area: Rect, state: &HomeScreenState, theme: Theme) {
    let logo_height = state.logo_lines.len() as u16;
    let total_content = logo_height + 1 + 1;
    let top_spacer = if area.height > total_content + 2 {
        (area.height - total_content) / 2
    } else {
        1
    };

    let mut y = area.top() + top_spacer;

    for (i, line) in state.logo_lines.iter().enumerate() {
        let line_width = line.width() as u16;
        let x = area.x + (area.width.saturating_sub(line_width)) / 2;
        let line_widget = Paragraph::new(Line::from(vec![Span::styled(
            *line,
            Style::default().fg(theme.text),
        )]))
        .style(Style::default().bg(theme.background));
        frame.render_widget(
            line_widget,
            Rect {
                x,
                y: y + i as u16,
                width: line_width.min(area.width),
                height: 1,
            },
        );
    }
    y += logo_height + 1;

    let tagline_text = "AI Coding Assistant";
    let tagline_width = tagline_text.width() as u16;
    let tagline_line = Line::from(vec![Span::styled(
        tagline_text,
        Style::default().fg(theme.text_muted),
    )]);
    frame.render_widget(
        Paragraph::new(tagline_line).style(Style::default().bg(theme.background)),
        Rect {
            x: area.x + (area.width.saturating_sub(tagline_width)) / 2,
            y,
            width: tagline_width,
            height: 1,
        },
    );
}
