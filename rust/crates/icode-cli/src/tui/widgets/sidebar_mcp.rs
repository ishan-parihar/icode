use crate::tui::dialog_mcp::{McpDialogState, McpServerEntry, McpServerStatus};
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Status of an MCP server connection shown in the sidebar panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpStatus {
    Connected,
    Disconnected,
    Error,
}

/// A single MCP server entry for sidebar display.
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: McpStatus,
    pub tool_count: usize,
}

/// State for the MCP panel in the sidebar.
#[derive(Debug, Clone)]
pub struct McpPanelState {
    pub servers: Vec<McpServerInfo>,
    pub total_tools: usize,
    pub expanded: bool,
}

impl McpPanelState {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            total_tools: 0,
            expanded: false,
        }
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn update_from_dialog(&mut self, dialog: &McpDialogState) {
        self.servers.clear();
        self.total_tools = 0;

        for entry in &dialog.servers {
            let status = match &entry.status {
                McpServerStatus::Connected => McpStatus::Connected,
                McpServerStatus::Disconnected | McpServerStatus::Starting => {
                    McpStatus::Disconnected
                }
                McpServerStatus::Error(_) => McpStatus::Error,
            };

            self.total_tools += entry.tool_count;
            self.servers.push(McpServerInfo {
                name: entry.name.clone(),
                status,
                tool_count: entry.tool_count,
            });
        }
    }
}

impl Default for McpPanelState {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_VISIBLE_MCP_SERVERS: usize = 5;

/// Render the MCP panel in the sidebar.
pub fn render_mcp_panel(
    frame: &mut Frame,
    state: &crate::tui::app::AppState,
    area: Rect,
    theme: &Theme,
) {
    let mcp_panel = &state.mcp_panel;
    let total = mcp_panel.servers.len();

    if total == 0 {
        return;
    }

    let connected = mcp_panel
        .servers
        .iter()
        .filter(|s| matches!(s.status, McpStatus::Connected))
        .count();
    let total_tools = mcp_panel.total_tools;

    let mut lines: Vec<Line> = Vec::new();

    let header_icon = if mcp_panel.expanded {
        "\u{25bc}"
    } else {
        "\u{25b6}"
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {header_icon} MCP "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({connected}/{total}, {total_tools} tools)"),
            Style::default().fg(theme.text_muted),
        ),
    ]));

    if mcp_panel.expanded {
        let visible = if total > MAX_VISIBLE_MCP_SERVERS {
            &mcp_panel.servers[..MAX_VISIBLE_MCP_SERVERS]
        } else {
            &mcp_panel.servers[..]
        };

        for server in visible {
            let (icon, color) = match server.status {
                McpStatus::Connected => ("\u{2713}", theme.success),
                McpStatus::Disconnected => ("\u{25cb}", theme.text_muted),
                McpStatus::Error => ("\u{2717}", theme.error),
            };

            let display_name = if server.name.chars().count() > 25 {
                format!("{}...", server.name.chars().take(22).collect::<String>())
            } else {
                server.name.clone()
            };

            let name_style = if matches!(server.status, McpStatus::Disconnected) {
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::DIM)
            } else {
                Style::default().fg(theme.text)
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
                    Style::default().fg(theme.text_muted),
                ),
            ]));
        }

        if total > MAX_VISIBLE_MCP_SERVERS {
            let remaining = total - MAX_VISIBLE_MCP_SERVERS;
            lines.push(Line::from(vec![Span::styled(
                format!("  ...{remaining} more"),
                Style::default().fg(theme.text_muted),
            )]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::dialog_mcp::{McpDialogState, McpServerEntry, McpServerStatus};

    fn make_test_dialog() -> McpDialogState {
        let mut dialog = McpDialogState::new();
        dialog.servers = vec![
            McpServerEntry {
                name: "filesystem".into(),
                transport: "stdio".into(),
                scope: "project".into(),
                status: McpServerStatus::Connected,
                tool_count: 5,
            },
            McpServerEntry {
                name: "github".into(),
                transport: "sse".into(),
                scope: "user".into(),
                status: McpServerStatus::Connected,
                tool_count: 12,
            },
            McpServerEntry {
                name: "database".into(),
                transport: "stdio".into(),
                scope: "project".into(),
                status: McpServerStatus::Disconnected,
                tool_count: 3,
            },
            McpServerEntry {
                name: "failing-server".into(),
                transport: "http".into(),
                scope: "global".into(),
                status: McpServerStatus::Error("Connection refused".into()),
                tool_count: 0,
            },
        ];
        dialog
    }

    #[test]
    fn test_mcp_panel_new() {
        let panel = McpPanelState::new();
        assert!(panel.servers.is_empty());
        assert_eq!(panel.total_tools, 0);
        assert!(!panel.expanded);
    }

    #[test]
    fn test_mcp_panel_default() {
        let panel = McpPanelState::default();
        assert!(panel.servers.is_empty());
        assert_eq!(panel.total_tools, 0);
    }

    #[test]
    fn test_toggle_expanded() {
        let mut panel = McpPanelState::new();
        assert!(!panel.expanded);

        panel.toggle();
        assert!(panel.expanded);

        panel.toggle();
        assert!(!panel.expanded);
    }

    #[test]
    fn test_update_from_dialog_populates_servers() {
        let dialog = make_test_dialog();
        let mut panel = McpPanelState::new();
        panel.update_from_dialog(&dialog);

        assert_eq!(panel.servers.len(), 4);
        assert_eq!(panel.total_tools, 20); // 5 + 12 + 3 + 0

        assert_eq!(panel.servers[0].name, "filesystem");
        assert!(matches!(panel.servers[0].status, McpStatus::Connected));
        assert_eq!(panel.servers[0].tool_count, 5);

        assert_eq!(panel.servers[1].name, "github");
        assert!(matches!(panel.servers[1].status, McpStatus::Connected));
        assert_eq!(panel.servers[1].tool_count, 12);

        assert_eq!(panel.servers[2].name, "database");
        assert!(matches!(panel.servers[2].status, McpStatus::Disconnected));
        assert_eq!(panel.servers[2].tool_count, 3);

        assert_eq!(panel.servers[3].name, "failing-server");
        assert!(matches!(panel.servers[3].status, McpStatus::Error));
        assert_eq!(panel.servers[3].tool_count, 0);
    }

    #[test]
    fn test_update_from_dialog_starting_maps_to_disconnected() {
        let mut dialog = McpDialogState::new();
        dialog.servers = vec![McpServerEntry {
            name: "starting-server".into(),
            transport: "stdio".into(),
            scope: "project".into(),
            status: McpServerStatus::Starting,
            tool_count: 0,
        }];

        let mut panel = McpPanelState::new();
        panel.update_from_dialog(&dialog);

        assert_eq!(panel.servers.len(), 1);
        assert!(matches!(panel.servers[0].status, McpStatus::Disconnected));
    }

    #[test]
    fn test_update_from_dialog_empty() {
        let dialog = McpDialogState::new();
        let mut panel = McpPanelState::new();
        panel.update_from_dialog(&dialog);

        assert!(panel.servers.is_empty());
        assert_eq!(panel.total_tools, 0);
    }

    #[test]
    fn test_update_from_dialog_clears_previous_state() {
        // First populate with data
        let dialog1 = make_test_dialog();
        let mut panel = McpPanelState::new();
        panel.update_from_dialog(&dialog1);
        assert_eq!(panel.servers.len(), 4);

        // Then update with empty dialog
        let dialog2 = McpDialogState::new();
        panel.update_from_dialog(&dialog2);

        assert!(panel.servers.is_empty());
        assert_eq!(panel.total_tools, 0);
    }

    #[test]
    fn test_mcp_status_derives() {
        let s1 = McpStatus::Connected;
        let s2 = McpStatus::Connected.clone();
        assert_eq!(s1, s2);

        let debug_str = format!("{s1:?}");
        assert!(debug_str.contains("Connected"));

        assert_ne!(McpStatus::Connected, McpStatus::Disconnected);
        assert_ne!(McpStatus::Connected, McpStatus::Error);
    }

    #[test]
    fn test_mcp_server_info_clone() {
        let info = McpServerInfo {
            name: "test-server".into(),
            status: McpStatus::Connected,
            tool_count: 7,
        };
        let cloned = info.clone();
        assert_eq!(info.name, cloned.name);
        assert_eq!(info.status, cloned.status);
        assert_eq!(info.tool_count, cloned.tool_count);
    }

    #[test]
    fn test_mcp_status_equality() {
        assert_eq!(McpStatus::Connected, McpStatus::Connected);
        assert_eq!(McpStatus::Disconnected, McpStatus::Disconnected);
        assert_eq!(McpStatus::Error, McpStatus::Error);

        assert_ne!(McpStatus::Connected, McpStatus::Disconnected);
        assert_ne!(McpStatus::Connected, McpStatus::Error);
        assert_ne!(McpStatus::Disconnected, McpStatus::Error);
    }

    #[test]
    fn test_total_tools_accurate() {
        let mut dialog = McpDialogState::new();
        dialog.servers = vec![
            McpServerEntry {
                name: "a".into(),
                transport: "stdio".into(),
                scope: "project".into(),
                status: McpServerStatus::Connected,
                tool_count: 10,
            },
            McpServerEntry {
                name: "b".into(),
                transport: "stdio".into(),
                scope: "project".into(),
                status: McpServerStatus::Disconnected,
                tool_count: 5,
            },
        ];

        let mut panel = McpPanelState::new();
        panel.update_from_dialog(&dialog);

        assert_eq!(panel.total_tools, 15);
    }
}
