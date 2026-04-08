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
            cap_badges.push('\u{1f4f7}');
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

        let total_files = state.files_panel.files.len();
        if total_files > 0 {
            let header_icon = if state.files_panel.expanded {
                "▼"
            } else {
                "▶"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {header_icon} Files "),
                    Style::default()
                        .fg(state.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({total_files})"),
                    Style::default().fg(state.theme.text_muted),
                ),
            ]));

            if state.files_panel.expanded {
                let max_visible = 8;
                let visible = if total_files > max_visible {
                    &state.files_panel.files[..max_visible]
                } else {
                    &state.files_panel.files[..]
                };

                for entry in visible {
                    let (icon, color) = match entry.status {
                        crate::tui::widgets::FileStatus::Modified => {
                            ("M", state.theme.diff_changed)
                        }
                        crate::tui::widgets::FileStatus::Created => ("A", state.theme.diff_added),
                        crate::tui::widgets::FileStatus::Deleted => ("D", state.theme.diff_removed),
                    };

                    let display_name = if entry.path.len() > 30 {
                        entry.path.rsplit('/').next().unwrap_or(&entry.path)
                    } else {
                        &entry.path
                    };

                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("[{icon}]"),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(display_name, Style::default().fg(state.theme.text)),
                    ]));
                }

                if total_files > max_visible {
                    let remaining = total_files - max_visible;
                    lines.push(Line::from(vec![Span::styled(
                        format!("  ...{remaining} more"),
                        Style::default().fg(state.theme.text_muted),
                    )]));
                }
            }

            lines.push(Line::from(""));
        }

        let total_todos = state.todo_panel.todos.len();
        if total_todos > 0 {
            let pending = state
                .todo_panel
                .todos
                .iter()
                .filter(|t| matches!(t.status, crate::tui::widgets::TodoStatus::Pending))
                .count();
            let completed = total_todos - pending;

            let todo_header_icon = if state.todo_panel.expanded {
                "\u{25bc}"
            } else {
                "\u{25b6}"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {todo_header_icon} Todos "),
                    Style::default()
                        .fg(state.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({pending}/{completed})"),
                    Style::default().fg(state.theme.text_muted),
                ),
            ]));

            if state.todo_panel.expanded {
                let max_visible = 8;
                let visible = if total_todos > max_visible {
                    &state.todo_panel.todos[..max_visible]
                } else {
                    &state.todo_panel.todos[..]
                };

                for item in visible {
                    let (icon, color) = match item.status {
                        crate::tui::widgets::TodoStatus::Completed => {
                            ("\u{2713}", state.theme.success)
                        }
                        crate::tui::widgets::TodoStatus::Pending => (" ", state.theme.text_muted),
                    };

                    let display_text = if item.text.len() > 35 {
                        format!("{}...", &item.text[..32])
                    } else {
                        item.text.clone()
                    };

                    let item_style =
                        if matches!(item.status, crate::tui::widgets::TodoStatus::Completed) {
                            Style::default()
                                .fg(state.theme.text_muted)
                                .add_modifier(Modifier::DIM)
                        } else {
                            Style::default().fg(state.theme.text)
                        };

                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("[{icon}]"),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(display_text, item_style),
                    ]));
                }

                if total_todos > max_visible {
                    let remaining = total_todos - max_visible;
                    lines.push(Line::from(vec![Span::styled(
                        format!("  ...{remaining} more"),
                        Style::default().fg(state.theme.text_muted),
                    )]));
                }
            }

            lines.push(Line::from(""));
        }

        let mcp_total = state.mcp_panel.servers.len();
        if mcp_total > 0 {
            let mcp_connected = state
                .mcp_panel
                .servers
                .iter()
                .filter(|s| matches!(s.status, crate::tui::widgets::McpStatus::Connected))
                .count();
            let mcp_tools = state.mcp_panel.total_tools;

            let mcp_header_icon = if state.mcp_panel.expanded {
                "\u{25bc}"
            } else {
                "\u{25b6}"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {mcp_header_icon} MCP "),
                    Style::default()
                        .fg(state.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({mcp_connected}/{mcp_total}, {mcp_tools} tools)"),
                    Style::default().fg(state.theme.text_muted),
                ),
            ]));

            if state.mcp_panel.expanded {
                let max_visible = 5;
                let visible = if mcp_total > max_visible {
                    &state.mcp_panel.servers[..max_visible]
                } else {
                    &state.mcp_panel.servers[..]
                };

                for server in visible {
                    let (icon, color) = match server.status {
                        crate::tui::widgets::McpStatus::Connected => {
                            ("\u{2713}", state.theme.success)
                        }
                        crate::tui::widgets::McpStatus::Disconnected => {
                            ("\u{25cb}", state.theme.text_muted)
                        }
                        crate::tui::widgets::McpStatus::Error => ("\u{2717}", state.theme.error),
                    };

                    let display_name = if server.name.len() > 25 {
                        format!("{}...", &server.name[..22])
                    } else {
                        server.name.clone()
                    };

                    let name_style =
                        if matches!(server.status, crate::tui::widgets::McpStatus::Disconnected) {
                            Style::default()
                                .fg(state.theme.text_muted)
                                .add_modifier(Modifier::DIM)
                        } else {
                            Style::default().fg(state.theme.text)
                        };

                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("[{icon}]"),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(display_name, name_style),
                        Span::styled(
                            format!(" ({})", server.tool_count),
                            Style::default().fg(state.theme.text_muted),
                        ),
                    ]));
                }

                if mcp_total > max_visible {
                    let remaining = mcp_total - max_visible;
                    lines.push(Line::from(vec![Span::styled(
                        format!("  ...{remaining} more"),
                        Style::default().fg(state.theme.text_muted),
                    )]));
                }
            }

            lines.push(Line::from(""));
        }

        let lsp_total = state.lsp_panel.servers.len();
        if lsp_total > 0 {
            let lsp_diag = state.lsp_panel.total_diagnostics;

            let lsp_header_icon = if state.lsp_panel.expanded {
                "\u{25bc}"
            } else {
                "\u{25b6}"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {lsp_header_icon} LSP "),
                    Style::default()
                        .fg(state.theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "({lsp_total} server{pl}, {lsp_diag} diagnostic{dl})",
                        pl = if lsp_total == 1 { "" } else { "s" },
                        dl = if lsp_diag == 1 { "" } else { "s" },
                    ),
                    Style::default().fg(state.theme.text_muted),
                ),
            ]));

            if state.lsp_panel.expanded {
                let max_visible = 5;
                let visible = if lsp_total > max_visible {
                    &state.lsp_panel.servers[..max_visible]
                } else {
                    &state.lsp_panel.servers[..]
                };

                for server in visible {
                    let (icon, color) = match server.status {
                        crate::tui::widgets::LspStatus::Running => {
                            ("\u{25cf}", state.theme.success)
                        }
                        crate::tui::widgets::LspStatus::Error => ("\u{25cf}", state.theme.error),
                        crate::tui::widgets::LspStatus::Idle => ("\u{25cb}", state.theme.warning),
                        crate::tui::widgets::LspStatus::Initializing => {
                            ("\u{25d0}", state.theme.info)
                        }
                    };

                    let diag_text = if server.diagnostics > 0 {
                        format!(" ({})", server.diagnostics)
                    } else {
                        String::new()
                    };

                    let display_name = if server.name.len() > 25 {
                        format!("{}...", &server.name[..22])
                    } else {
                        server.name.clone()
                    };

                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("[{icon}]"),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(display_name, Style::default().fg(state.theme.text)),
                        Span::styled(diag_text, Style::default().fg(state.theme.text_muted)),
                    ]));
                }

                if lsp_total > max_visible {
                    let remaining = lsp_total - max_visible;
                    lines.push(Line::from(vec![Span::styled(
                        format!("  ...{remaining} more"),
                        Style::default().fg(state.theme.text_muted),
                    )]));
                }
            }

            lines.push(Line::from(""));
        }

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
