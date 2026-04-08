use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::tui::theme::Theme;

/// Status of a child/sub-agent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubAgentStatus {
    Running,
    Completed,
    Failed,
}

/// Information about a child/sub-agent that ran during a turn.
#[derive(Debug, Clone)]
pub struct SubAgentInfo {
    /// Agent type name, e.g. "explorer", "planner".
    pub name: String,
    /// Current lifecycle status.
    pub status: SubAgentStatus,
    /// Tokens consumed by this sub-agent.
    pub tokens_used: usize,
    /// Number of tool calls made by this sub-agent.
    pub tool_calls: usize,
    /// Brief summary of what the sub-agent did.
    pub summary: String,
    /// Full output from the sub-agent (shown when expanded).
    pub output: Option<String>,
    /// Whether the sub-agent detail is expanded in the UI.
    pub expanded: bool,
}

impl SubAgentInfo {
    /// Visual status indicator symbol and its color.
    pub fn status_indicator(&self, theme: &Theme) -> (&'static str, Color) {
        match self.status {
            SubAgentStatus::Running => ("\u{25d0}", theme.warning),
            SubAgentStatus::Completed => ("\u{25cf}", theme.success),
            SubAgentStatus::Failed => ("\u{2717}", theme.error),
        }
    }
}

/// Render a compact inline sub-agent footer line for a message.
/// Returns the rendered line(s). When collapsed, shows a single summary line.
pub fn render_subagent_footer_collapsed(
    sub_agents: &[SubAgentInfo],
    theme: &Theme,
) -> Vec<Line<'static>> {
    if sub_agents.is_empty() {
        return Vec::new();
    }

    let mut spans = vec![Span::styled(
        "  \u{251c} ",
        Style::default().fg(theme.text_muted),
    )];

    for (i, agent) in sub_agents.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  |  ", Style::default().fg(theme.text_muted)));
        }
        let (icon, color) = agent.status_indicator(theme);
        spans.push(Span::styled(
            format!("{icon} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(agent.name.clone(), Style::default().fg(color)));
        if agent.tokens_used > 0 {
            spans.push(Span::styled(
                format!(" ({}t)", agent.tokens_used),
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            ));
        }
    }

    vec![Line::from(spans)]
}

/// Render the expanded sub-agent detail block.
pub fn render_subagent_footer_expanded(
    sub_agents: &[SubAgentInfo],
    theme: &Theme,
    content_width: usize,
) -> Vec<Line<'static>> {
    if sub_agents.is_empty() {
        return Vec::new();
    }

    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(
            "  \u{25bc} Sub-agents",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({})", sub_agents.len()),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        ),
    ]));

    for agent in sub_agents {
        let (icon, color) = agent.status_indicator(theme);

        let mut agent_line = vec![
            Span::raw("    "),
            Span::styled(
                format!("{icon} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                agent.name.clone(),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
        ];

        if agent.tokens_used > 0 {
            agent_line.push(Span::styled(
                format!(" {} tokens", agent.tokens_used),
                Style::default().fg(theme.text_muted),
            ));
        }
        if agent.tool_calls > 0 {
            agent_line.push(Span::styled(
                format!(" {} tools", agent.tool_calls),
                Style::default().fg(theme.text_muted),
            ));
        }

        let status_label = match agent.status {
            SubAgentStatus::Running => "running",
            SubAgentStatus::Completed => "done",
            SubAgentStatus::Failed => "failed",
        };
        agent_line.push(Span::styled(
            format!(" [{status_label}]"),
            Style::default().fg(color),
        ));

        lines.push(Line::from(agent_line));

        if !agent.summary.is_empty() {
            let summary_lines = wrap_and_prefix(
                &agent.summary,
                content_width.saturating_sub(8),
                "      ",
                theme.text_muted,
            );
            for sl in summary_lines {
                lines.push(Line::from(vec![sl]));
            }
        }

        if agent.expanded {
            if let Some(ref output) = agent.output {
                let out_lines = wrap_and_prefix(
                    output,
                    content_width.saturating_sub(8),
                    "      ",
                    theme.text,
                );
                let max_lines = 10;
                for (i, ol) in out_lines.iter().enumerate().take(max_lines) {
                    lines.push(Line::from(vec![ol.clone()]));
                }
                if out_lines.len() > max_lines {
                    lines.push(Line::from(vec![Span::styled(
                        format!("        ... {} more lines", out_lines.len() - max_lines),
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::ITALIC),
                    )]));
                }
            }
        } else if agent.output.is_some() {
            lines.push(Line::from(vec![Span::styled(
                "      (click to expand output)",
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )]));
        }
    }

    lines
}

fn wrap_and_prefix(text: &str, max_width: usize, prefix: &str, color: Color) -> Vec<Span<'static>> {
    if max_width == 0 {
        return vec![Span::styled(
            format!("{prefix}{text}"),
            Style::default().fg(color),
        )];
    }

    let prefix_width = prefix
        .chars()
        .map(|c| c.width().unwrap_or(1))
        .sum::<usize>();
    let effective = max_width.saturating_sub(prefix_width).max(10);

    let mut spans = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            spans.push(Span::styled(prefix.to_string(), Style::default().fg(color)));
            continue;
        }
        let mut chunk_start = 0;
        let mut current_width = 0;
        for (byte_idx, ch) in line.char_indices() {
            let char_w = ch.width().unwrap_or(1);
            if current_width + char_w > effective {
                spans.push(Span::styled(
                    format!("{prefix}{}", &line[chunk_start..byte_idx]),
                    Style::default().fg(color),
                ));
                chunk_start = byte_idx;
                current_width = char_w;
            } else {
                current_width += char_w;
            }
        }
        if chunk_start < line.len() {
            spans.push(Span::styled(
                format!("{prefix}{}", &line[chunk_start..]),
                Style::default().fg(color),
            ));
        }
    }
    if spans.is_empty() {
        spans.push(Span::styled(prefix.to_string(), Style::default().fg(color)));
    }
    spans
}

/// Total lines the sub-agent footer will occupy.
pub fn subagent_footer_line_count(sub_agents: &[SubAgentInfo], content_width: usize) -> usize {
    if sub_agents.is_empty() {
        return 0;
    }

    let all_expanded = sub_agents.iter().any(|a| a.expanded);
    if !all_expanded {
        return 1;
    }

    let mut count = 1;
    for agent in sub_agents {
        count += 1;
        if !agent.summary.is_empty() {
            let pw = 6;
            let effective = content_width.saturating_sub(8 + pw).max(10);
            count += agent
                .summary
                .lines()
                .map(|l| {
                    if l.is_empty() {
                        1
                    } else {
                        (l.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>() + effective - 1)
                            / effective.max(1)
                    }
                    .max(1)
                })
                .sum::<usize>();
        }
        if agent.expanded {
            if let Some(ref output) = agent.output {
                let pw = 6;
                let effective = content_width.saturating_sub(8 + pw).max(10);
                let out_lines: usize = output
                    .lines()
                    .map(|l| {
                        if l.is_empty() {
                            1
                        } else {
                            (l.chars().map(|c| c.width().unwrap_or(1)).sum::<usize>() + effective
                                - 1)
                                / effective.max(1)
                        }
                        .max(1)
                    })
                    .sum();
                count += out_lines.min(10);
                if out_lines > 10 {
                    count += 1;
                }
            }
        } else if agent.output.is_some() {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_indicator_colors() {
        let theme = Theme::dark();
        let running = SubAgentInfo {
            name: "explorer".into(),
            status: SubAgentStatus::Running,
            tokens_used: 0,
            tool_calls: 0,
            summary: String::new(),
            output: None,
            expanded: false,
        };
        let (icon, color) = running.status_indicator(&theme);
        assert_eq!(icon, "\u{25d0}");
        assert_eq!(color, theme.warning);

        let completed = SubAgentInfo {
            name: "planner".into(),
            status: SubAgentStatus::Completed,
            tokens_used: 100,
            tool_calls: 3,
            summary: "Found 5 files".into(),
            output: None,
            expanded: false,
        };
        let (icon, color) = completed.status_indicator(&theme);
        assert_eq!(icon, "\u{25cf}");
        assert_eq!(color, theme.success);
    }

    #[test]
    fn test_collapsed_footer_empty() {
        let theme = Theme::dark();
        let lines = render_subagent_footer_collapsed(&[], &theme);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_collapsed_footer_single() {
        let theme = Theme::dark();
        let agents = vec![SubAgentInfo {
            name: "explorer".into(),
            status: SubAgentStatus::Completed,
            tokens_used: 50,
            tool_calls: 2,
            summary: "Found 3 files".into(),
            output: None,
            expanded: false,
        }];
        let lines = render_subagent_footer_collapsed(&agents, &theme);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_collapsed_footer_multiple() {
        let theme = Theme::dark();
        let agents = vec![
            SubAgentInfo {
                name: "explorer".into(),
                status: SubAgentStatus::Completed,
                tokens_used: 50,
                tool_calls: 0,
                summary: String::new(),
                output: None,
                expanded: false,
            },
            SubAgentInfo {
                name: "planner".into(),
                status: SubAgentStatus::Running,
                tokens_used: 0,
                tool_calls: 1,
                summary: String::new(),
                output: None,
                expanded: false,
            },
        ];
        let lines = render_subagent_footer_collapsed(&agents, &theme);
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("explorer"));
        assert!(text.contains("planner"));
    }

    #[test]
    fn test_expanded_footer_has_header() {
        let theme = Theme::dark();
        let agents = vec![SubAgentInfo {
            name: "explorer".into(),
            status: SubAgentStatus::Completed,
            tokens_used: 100,
            tool_calls: 5,
            summary: "Explored codebase structure".into(),
            output: Some("Found 10 relevant files".into()),
            expanded: false,
        }];
        let lines = render_subagent_footer_expanded(&agents, &theme, 80);
        assert!(!lines.is_empty());
        let header_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header_text.contains("Sub-agents"));
    }

    #[test]
    fn test_subagent_footer_line_count_empty() {
        assert_eq!(subagent_footer_line_count(&[], 80), 0);
    }

    #[test]
    fn test_subagent_footer_line_count_collapsed() {
        let agents = vec![SubAgentInfo {
            name: "explorer".into(),
            status: SubAgentStatus::Completed,
            tokens_used: 0,
            tool_calls: 0,
            summary: String::new(),
            output: None,
            expanded: false,
        }];
        assert_eq!(subagent_footer_line_count(&agents, 80), 1);
    }
}
