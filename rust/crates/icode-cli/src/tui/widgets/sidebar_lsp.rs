use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::app::AppState;
use crate::tui::theme::Theme;

/// Status of an LSP server connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspStatus {
    Running,
    Error,
    Idle,
    Initializing,
}

/// Information about a single LSP server.
#[derive(Debug, Clone)]
pub struct LspServerInfo {
    pub name: String,
    pub status: LspStatus,
    pub diagnostics: usize,
}

/// State for the LSP panel in the sidebar.
#[derive(Debug, Clone)]
pub struct LspPanelState {
    pub servers: Vec<LspServerInfo>,
    pub total_diagnostics: usize,
    pub expanded: bool,
}

impl LspPanelState {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            total_diagnostics: 0,
            expanded: false,
        }
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn update_count(&mut self, lsp_count: usize, lsp_errors: usize) {
        let current_len = self.servers.len();

        if lsp_count > current_len {
            for i in current_len..lsp_count {
                self.servers.push(LspServerInfo {
                    name: format!("LSP {}", i + 1),
                    status: LspStatus::Running,
                    diagnostics: 0,
                });
            }
        } else if lsp_count < current_len {
            self.servers.truncate(lsp_count);
        }

        if let Some(first) = self.servers.first_mut() {
            first.diagnostics = lsp_errors;
            if lsp_errors > 0 {
                first.status = LspStatus::Error;
            }
        }

        if lsp_count == 0 {
            self.servers.clear();
        }

        self.total_diagnostics = lsp_errors;
    }
}

const MAX_VISIBLE_LSP: usize = 5;

/// Render the LSP panel in the sidebar.
pub fn render_lsp_panel(frame: &mut Frame, state: &AppState, area: Rect, theme: &Theme) {
    let lsp_panel = &state.lsp_panel;
    let total = lsp_panel.servers.len();

    if total == 0 {
        return;
    }

    let diag = lsp_panel.total_diagnostics;

    let mut lines: Vec<Line> = Vec::new();

    let header_icon = if lsp_panel.expanded {
        "\u{25bc}"
    } else {
        "\u{25b6}"
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {header_icon} LSP "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "({total} server{pl}, {diag} diagnostic{dl})",
                pl = if total != 1 { "s" } else { "" },
                dl = if diag != 1 { "s" } else { "" },
            ),
            Style::default().fg(theme.text_muted),
        ),
    ]));

    if lsp_panel.expanded {
        let visible = if total > MAX_VISIBLE_LSP {
            &lsp_panel.servers[..MAX_VISIBLE_LSP]
        } else {
            &lsp_panel.servers[..]
        };

        for server in visible {
            let (icon, color) = match server.status {
                LspStatus::Running => ("\u{25cf}", theme.success), // ● green
                LspStatus::Error => ("\u{25cf}", theme.error),     // ● red
                LspStatus::Idle => ("\u{25cb}", theme.warning),    // ○ yellow
                LspStatus::Initializing => ("\u{25d0}", theme.info), // ◐ blue
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
                Span::styled(display_name, Style::default().fg(theme.text)),
                Span::styled(diag_text, Style::default().fg(theme.text_muted)),
            ]));
        }

        if total > MAX_VISIBLE_LSP {
            let remaining = total - MAX_VISIBLE_LSP;
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

    #[test]
    fn test_new_panel_is_empty() {
        let panel = LspPanelState::new();
        assert!(panel.servers.is_empty());
        assert_eq!(panel.total_diagnostics, 0);
        assert!(!panel.expanded);
    }

    #[test]
    fn test_toggle_expanded() {
        let mut panel = LspPanelState::new();
        assert!(!panel.expanded);

        panel.toggle();
        assert!(panel.expanded);

        panel.toggle();
        assert!(!panel.expanded);
    }

    #[test]
    fn test_update_count_adds_servers() {
        let mut panel = LspPanelState::new();
        panel.update_count(3, 0);

        assert_eq!(panel.servers.len(), 3);
        assert_eq!(panel.servers[0].name, "LSP 1");
        assert_eq!(panel.servers[1].name, "LSP 2");
        assert_eq!(panel.servers[2].name, "LSP 3");
        assert!(matches!(panel.servers[0].status, LspStatus::Running));
    }

    #[test]
    fn test_update_count_removes_excess_servers() {
        let mut panel = LspPanelState::new();
        panel.update_count(3, 0);
        assert_eq!(panel.servers.len(), 3);

        panel.update_count(1, 0);
        assert_eq!(panel.servers.len(), 1);
        assert_eq!(panel.servers[0].name, "LSP 1");
    }

    #[test]
    fn test_update_count_assigns_diagnostics() {
        let mut panel = LspPanelState::new();
        panel.update_count(2, 5);

        assert_eq!(panel.total_diagnostics, 5);
        assert_eq!(panel.servers[0].diagnostics, 5);
        assert!(matches!(panel.servers[0].status, LspStatus::Error));
        assert_eq!(panel.servers[1].diagnostics, 0);
    }

    #[test]
    fn test_update_count_clears_on_zero() {
        let mut panel = LspPanelState::new();
        panel.update_count(3, 5);
        assert_eq!(panel.servers.len(), 3);

        panel.update_count(0, 0);
        assert!(panel.servers.is_empty());
        assert_eq!(panel.total_diagnostics, 0);
    }

    #[test]
    fn test_update_count_preserves_expanded() {
        let mut panel = LspPanelState::new();
        panel.toggle();
        assert!(panel.expanded);

        panel.update_count(2, 0);
        assert!(panel.expanded);
    }

    #[test]
    fn test_update_count_zero_diagnostics_keeps_running() {
        let mut panel = LspPanelState::new();
        panel.update_count(2, 0);

        assert!(matches!(panel.servers[0].status, LspStatus::Running));
        assert_eq!(panel.servers[0].diagnostics, 0);
    }

    #[test]
    fn test_update_count_incremental_add() {
        let mut panel = LspPanelState::new();
        panel.update_count(1, 0);
        panel.update_count(2, 0);
        panel.update_count(3, 0);

        assert_eq!(panel.servers.len(), 3);
    }

    #[test]
    fn test_lsp_status_derives() {
        let s1 = LspStatus::Running;
        let s2 = LspStatus::Running.clone();
        assert_eq!(s1, s2);

        let debug_str = format!("{s1:?}");
        assert!(debug_str.contains("Running"));
    }

    #[test]
    fn test_lsp_server_info_clone() {
        let server = LspServerInfo {
            name: "rust-analyzer".into(),
            status: LspStatus::Running,
            diagnostics: 3,
        };
        let cloned = server.clone();
        assert_eq!(server.name, cloned.name);
        assert_eq!(server.status, cloned.status);
        assert_eq!(server.diagnostics, cloned.diagnostics);
    }

    #[test]
    fn test_update_count_error_status_persists_on_zero_diagnostics() {
        let mut panel = LspPanelState::new();
        panel.update_count(1, 5);
        assert!(matches!(panel.servers[0].status, LspStatus::Error));

        panel.update_count(1, 0);
        assert!(matches!(panel.servers[0].status, LspStatus::Error));
    }

    #[test]
    fn test_update_count_handles_same_count() {
        let mut panel = LspPanelState::new();
        panel.update_count(2, 3);
        let before_len = panel.servers.len();

        panel.update_count(2, 5);
        assert_eq!(panel.servers.len(), before_len);
        assert_eq!(panel.total_diagnostics, 5);
    }
}
