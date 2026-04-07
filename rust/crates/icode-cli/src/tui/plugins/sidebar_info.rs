use crate::tui::plugin::{PluginApi, PluginCommand, PluginRoute, PluginSlot, SlotStyle, TuiPlugin};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct SidebarInfoPlugin;

impl TuiPlugin for SidebarInfoPlugin {
    fn id(&self) -> &'static str {
        "sidebar-info"
    }

    fn name(&self) -> &'static str {
        "Sidebar Info"
    }

    fn description(&self) -> &'static str {
        "Session info, activity stats, and integration counts"
    }

    fn register_slots(&self, api: &mut PluginApi<'_>) {
        api.register_slot_content(
            self.id(),
            PluginSlot::SidebarBottom,
            vec![
                format!(
                    "{} plugins · {} skills",
                    api.state.plugin_count, api.state.skill_count
                ),
                format!("{} LSP · {} MCP", api.state.lsp_count, api.state.mcp_count),
            ],
            SlotStyle::Default,
        );
    }

    fn register_routes(&self, api: &mut PluginApi<'_>) {
        let plugin_id = self.id().to_string();
        api.register_route(PluginRoute {
            id: "sidebar-info:dashboard".to_string(),
            title: "Plugin Dashboard".to_string(),
            icon: "\u{1F4CA}".to_string(),
            category: "Plugins".to_string(),
            render_fn: Box::new(
                move |frame: &mut Frame,
                      area: Rect,
                      state: &crate::tui::app::AppState,
                      theme: crate::tui::Theme| {
                    let mut lines = vec![
                        Line::from(Span::styled(
                            "Plugin Dashboard",
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(
                            format!(
                                "Plugins: {} · Skills: {}",
                                state.plugin_count, state.skill_count
                            ),
                            Style::default().fg(theme.text),
                        )),
                        Line::from(Span::styled(
                            format!(
                                "LSP servers: {} · MCP servers: {}",
                                state.lsp_count, state.mcp_count
                            ),
                            Style::default().fg(theme.text),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Registered Routes:",
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD),
                        )),
                    ];

                    for route in &state.plugin_routes {
                        lines.push(Line::from(Span::styled(
                            format!("  {} {}", route.icon, route.title),
                            Style::default().fg(theme.text),
                        )));
                    }

                    if state.plugin_routes.is_empty() {
                        lines.push(Line::from(Span::styled(
                            "  (no routes registered)",
                            Style::default().fg(theme.text_muted),
                        )));
                    }

                    let para = Paragraph::new(lines).style(Style::default().bg(theme.background));
                    frame.render_widget(para, area);
                },
            ),
        });
    }

    fn register_commands(&self) -> Vec<PluginCommand> {
        vec![
            PluginCommand {
                id: "sidebar-info:show-stats".to_string(),
                title: "Show session stats".to_string(),
                description: "Display current session statistics".to_string(),
                category: "System".to_string(),
                keybind: None,
                on_execute: Box::new(|api| {
                    let s = &api.state.session;
                    let msg_count = api.state.messages.len();
                    let tool_count = api.state.tools.len();
                    let info = format!(
                        "Session: {}\n\
                         Messages: {}\n\
                         Turns: {}\n\
                         Tools used: {}\n\
                         Input tokens: {}\n\
                         Output tokens: {}\n\
                         Model: {}\n\
                         Connected: {}",
                        s.title,
                        msg_count,
                        s.turns,
                        tool_count,
                        s.input_tokens,
                        s.output_tokens,
                        s.model,
                        api.state.connected,
                    );
                    api.toast_success(format!("{} messages, {} turns", msg_count, s.turns));
                    Some(info)
                }),
            },
            PluginCommand {
                id: "sidebar-info:show-integrations".to_string(),
                title: "Show integration counts".to_string(),
                description: "Display LSP, MCP, skill, and plugin counts".to_string(),
                category: "System".to_string(),
                keybind: None,
                on_execute: Box::new(|api| {
                    let info = format!(
                        "LSP servers: {}\n\
                         MCP servers: {}\n\
                         Skills: {}\n\
                         Plugins: {}",
                        api.state.lsp_count,
                        api.state.mcp_count,
                        api.state.skill_count,
                        api.state.plugin_count,
                    );
                    api.toast(format!(
                        "LSP:{} MCP:{} Skills:{} Plugins:{}",
                        api.state.lsp_count,
                        api.state.mcp_count,
                        api.state.skill_count,
                        api.state.plugin_count,
                    ));
                    Some(info)
                }),
            },
        ]
    }
}
