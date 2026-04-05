use crate::tui::app::AppState;
use crate::tui::theme::Theme;
use api::capabilities_for_model;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

pub struct Sidebar;

impl Sidebar {
    pub const fn new() -> Self {
        Self
    }

    pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
        if !state.sidebar_visible {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(vec![Span::styled(
            " Session",
            Style::default()
                .fg(state.theme.primary)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("  Title    ", Style::default().fg(state.theme.text_muted)),
            Span::styled(&state.session.title, Style::default().fg(state.theme.text)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Model    ", Style::default().fg(state.theme.text_muted)),
            Span::styled(&state.session.model, Style::default().fg(state.theme.text)),
        ]));

        let caps = capabilities_for_model(&state.session.model);
        let total_tokens = state.session.input_tokens + state.session.output_tokens;
        let usage_pct = if caps.context_window > 0 {
            (total_tokens as f64 / caps.context_window as f64 * 100.0).round() as u32
        } else {
            0
        };
        let usage_color = if usage_pct < 50 {
            state.theme.success
        } else if usage_pct < 80 {
            state.theme.warning
        } else {
            state.theme.error
        };
        lines.push(Line::from(vec![
            Span::styled("  Context  ", Style::default().fg(state.theme.text_muted)),
            Span::styled(
                format!("{usage_pct}%"),
                Style::default()
                    .fg(usage_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" ({}/{}k)", total_tokens / 1000, caps.context_window / 1000),
                Style::default().fg(state.theme.text_muted),
            ),
        ]));

        let mut cap_badges = String::new();
        if caps.supports_reasoning {
            cap_badges.push_str("\u{1f9e0} ");
        }
        if caps.supports_tools {
            cap_badges.push_str("\u{1f527} ");
        }
        if caps.supports_images {
            cap_badges.push_str("\u{1f4f7}");
        }
        if !cap_badges.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Caps     ", Style::default().fg(state.theme.text_muted)),
                Span::styled(cap_badges.trim_end(), Style::default().fg(state.theme.text)),
            ]));
        }

        lines.push(Line::from(vec![
            Span::styled("  Mode     ", Style::default().fg(state.theme.text_muted)),
            Span::styled(
                &state.session.permission_mode,
                Style::default().fg(state.theme.text),
            ),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            " Activity",
            Style::default()
                .fg(state.theme.primary)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("  Turns    ", Style::default().fg(state.theme.text_muted)),
            Span::styled(
                format!("{}", state.session.turns),
                Style::default().fg(state.theme.text),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Messages ", Style::default().fg(state.theme.text_muted)),
            Span::styled(
                format!("{}", state.session.message_count),
                Style::default().fg(state.theme.text),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Tokens   ", Style::default().fg(state.theme.text_muted)),
            Span::styled(
                format!(
                    "{} in / {} out",
                    state.session.input_tokens, state.session.output_tokens
                ),
                Style::default().fg(state.theme.text),
            ),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            " Integrations",
            Style::default()
                .fg(state.theme.primary)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        let mcp_count = state.mcp_dialog.servers.len();
        let mcp_status = if mcp_count == 0 {
            "None configured".to_string()
        } else {
            format!("{mcp_count} server(s)")
        };
        lines.push(Line::from(vec![
            Span::styled("  MCP      ", Style::default().fg(state.theme.text_muted)),
            Span::styled(mcp_status, Style::default().fg(state.theme.text)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Skills   ", Style::default().fg(state.theme.text_muted)),
            Span::styled(
                format!("{}", state.skills_dialog.skills.len()),
                Style::default().fg(state.theme.text),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Plugins  ", Style::default().fg(state.theme.text_muted)),
            Span::styled(
                format!("{}", state.plugins_dialog.plugins.len()),
                Style::default().fg(state.theme.text),
            ),
        ]));

        lines.push(Line::from(""));

        if !state.tools.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                " Tools",
                Style::default()
                    .fg(state.theme.primary)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(""));

            let recent_tools: Vec<_> = state.tools.iter().rev().take(5).collect();
            for tool in recent_tools {
                let icon = match tool.status {
                    crate::tui::app::ToolStatus::Running => "\u{25cb}",
                    crate::tui::app::ToolStatus::Completed => "\u{2713}",
                    crate::tui::app::ToolStatus::Failed => "\u{2717}",
                    crate::tui::app::ToolStatus::Pending => "\u{25cb}",
                };
                let color = match tool.status {
                    crate::tui::app::ToolStatus::Running => state.theme.warning,
                    crate::tui::app::ToolStatus::Completed => state.theme.success,
                    crate::tui::app::ToolStatus::Failed => state.theme.error,
                    crate::tui::app::ToolStatus::Pending => state.theme.text_muted,
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(&tool.name, Style::default().fg(state.theme.text_muted)),
                ]));
            }
            lines.push(Line::from(""));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " \u{2022} icode",
            Style::default().fg(state.theme.text_muted),
        )));

        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(state.theme.border))
            .border_type(ratatui::widgets::BorderType::Plain);

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}
