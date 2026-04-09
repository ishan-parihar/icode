use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

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
        let line_width = line.len() as u16;
        let x = area.x + (area.width.saturating_sub(line_width)) / 2;
        let spans = build_logo_spans(line, theme);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.background)),
            Rect {
                x,
                y: y + i as u16,
                width: line_width.min(area.width),
                height: 1,
            },
        );
    }
    y += logo_height + 1;

    let tagline = Line::from(vec![Span::styled(
        "AI Coding Assistant",
        Style::default().fg(theme.text_muted),
    )]);
    let tagline_width = 19u16;
    frame.render_widget(
        Paragraph::new(tagline).style(Style::default().bg(theme.background)),
        Rect {
            x: area.x + (area.width.saturating_sub(tagline_width)) / 2,
            y,
            width: tagline_width,
            height: 1,
        },
    );
}

fn tint_color(base: Color, into: Color, factor: f32) -> Color {
    let (br, bg, bb) = match base {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return base,
    };
    let (ir, ig, ib) = match into {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return into,
    };
    let r = (br + (ir - br) * factor).round() as u8;
    let g = (bg + (ig - bg) * factor).round() as u8;
    let b = (bb + (ib - bb) * factor).round() as u8;
    Color::Rgb(r, g, b)
}

fn build_logo_spans(line: &str, theme: Theme) -> Vec<Span<'static>> {
    let i_color = tint_color(theme.background, theme.primary, 0.35);
    let fg = theme.text;
    let mut spans = Vec::new();
    let mut current_i = String::new();
    let mut current_other = String::new();
    for (idx, ch) in line.chars().enumerate() {
        let is_i = (5..=7).contains(&idx);
        if is_i {
            if !current_other.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut current_other),
                    Style::default().fg(fg),
                ));
            }
            current_i.push(ch);
        } else {
            if !current_i.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut current_i),
                    Style::default().fg(i_color),
                ));
            }
            current_other.push(ch);
        }
    }
    if !current_other.is_empty() {
        spans.push(Span::styled(current_other, Style::default().fg(fg)));
    }
    if !current_i.is_empty() {
        spans.push(Span::styled(current_i, Style::default().fg(i_color)));
    }
    spans
}
