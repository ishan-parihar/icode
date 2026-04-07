use crate::tui::app::{AppMode, AppState, ToastKind};
use crate::tui::command_palette::render_command_palette;
use crate::tui::debug_panel::render_debug_panel;
use crate::tui::dialog_context_viz::render_context_viz_dialog;
use crate::tui::dialog_export_options::render_export_options_dialog;
use crate::tui::dialog_help::render_help_dialog;
use crate::tui::dialog_mcp::render_mcp_dialog;
use crate::tui::dialog_message_actions::render_message_action_dialog;
use crate::tui::dialog_plugins::render_plugins_dialog;
use crate::tui::dialog_prompt_stash::render_prompt_stash_dialog;
use crate::tui::dialog_providers::render_provider_dialog;
use crate::tui::dialog_session_branching::render_session_branching;
use crate::tui::dialog_sessions::render_sessions_dialog;
use crate::tui::dialog_skills::render_skills_dialog;
use crate::tui::dialog_theme_list::render_theme_list_dialog;
use crate::tui::dialog_workspaces::render_workspace_dialog;
use crate::tui::model_picker::render_model_picker;
use crate::tui::widgets::{render_pager, DiffView, MessageList, Sidebar};
use crate::tui::InputWidget;
use crate::tui::Theme;
use api::capabilities_for_model;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{StatefulWidget, Widget};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;
use runtime::{format_usd, pricing_for_model};
use std::time::Instant;

pub fn render_ui(frame: &mut Frame, state: &mut AppState, theme: Theme) {
    let area = frame.area();
    let bg = Paragraph::new("").style(Style::default().bg(state.theme.background));
    frame.render_widget(bg, area);
    let has_sidebar = state.sidebar_visible && area.width > 120;
    let content_width = if has_sidebar {
        area.width.saturating_sub(42)
    } else {
        area.width
    };
    let prompt_lines = state.prompt.line_count(content_width as usize).clamp(1, 6);
    let prompt_height = (prompt_lines as u16) + 3;

    let is_welcome = state.messages.is_empty();
    let constraints = if is_welcome {
        vec![Constraint::Min(1), Constraint::Length(1)]
    } else {
        vec![
            Constraint::Min(1),
            Constraint::Length(prompt_height as u16),
            Constraint::Length(1),
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let main_area = chunks[0];
    let (prompt_area, footer_area) = if is_welcome {
        (None, chunks[1])
    } else {
        (Some(chunks[1]), chunks[2])
    };

    if has_sidebar {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(42)])
            .split(main_area);

        render_messages_panel(frame, state, main_chunks[0], theme);
        Sidebar::render(frame, state, main_chunks[1]);

        let divider_x = main_chunks[0].width.saturating_sub(1);
        if divider_x > 0 {
            for y in main_area.top()..main_area.bottom() {
                if let Some(cell) = frame
                    .buffer_mut()
                    .cell_mut((main_chunks[0].x + divider_x, y))
                {
                    cell.set_char('\u{2502}')
                        .set_style(Style::default().fg(state.theme.border));
                }
            }
        }
    } else {
        render_messages_panel(frame, state, main_area, theme);
    }

    if let Some(pa) = prompt_area {
        render_prompt(frame, state, pa, theme);
        if state.prompt.show_slash_autocomplete && !state.prompt.slash_completions.is_empty() {
            render_slash_autocomplete(frame.buffer_mut(), &state.prompt, pa, theme);
        }
    }
    render_footer(frame, state, footer_area);
    render_toasts(frame, state, area);

    if state.model_picker.open {
        render_model_picker(frame, &mut state.model_picker, area, theme);
    }

    if state.command_palette.open {
        render_command_palette(frame, &mut state.command_palette, area, theme);
    }

    if state.mcp_dialog.open {
        render_mcp_dialog(frame, &mut state.mcp_dialog, area, state.theme);
    }

    if state.skills_dialog.open {
        render_skills_dialog(frame, &mut state.skills_dialog, area, theme);
    }

    render_theme_list_dialog(frame, &state.theme_list_dialog, area, state.theme);

    if state.plugins_dialog.open {
        render_plugins_dialog(frame, &mut state.plugins_dialog, area, theme);
    }

    if state.sessions_dialog.open {
        render_sessions_dialog(frame, &mut state.sessions_dialog, area, state.theme);
    }

    if state.message_action_dialog.open {
        render_message_action_dialog(frame, &state.message_action_dialog, area, state.theme);
    }

    if state.help_dialog.open {
        render_help_dialog(frame, &state.help_dialog, area, state.theme);
    }

    if state.context_viz_dialog.open {
        render_context_viz_dialog(
            frame,
            &state.context_viz_dialog,
            area,
            state.theme,
            &state.session.model,
            state.session.input_tokens,
            state.session.output_tokens,
            state.session.cache_create_tokens,
            state.session.cache_read_tokens,
            state.context_window,
            state.session.turns,
            state.session.message_count,
            state.session.cumulative_cost,
            state.session.budget_max,
            state.session.budget_remaining,
            state.session.compaction_count,
            state.session.compaction_removed_messages,
            &state.session.effort_level,
        );
    }

    if state.branching_dialog.open {
        render_session_branching(frame, &mut state.branching_dialog, area, state.theme);
    }

    if state.prompt_stash.open {
        render_prompt_stash_dialog(frame, &state.prompt_stash, area, state.theme);
    }

    if state.export_options.open {
        render_export_options_dialog(frame, &state.export_options, area, state.theme);
    }

    if state.debug_panel.open {
        render_debug_panel(frame, &state.debug_panel, area, state.theme, state);
    }

    if state.provider_dialog.open {
        render_provider_dialog(frame, &mut state.provider_dialog, area, state.theme);
    }

    if state.workspace_dialog.open {
        render_workspace_dialog(frame, &mut state.workspace_dialog, area, state.theme);
    }

    if let Some(ref mut diff_view) = state.diff_view {
        render_diff_view_overlay(frame, diff_view, area, &state.theme);
    }

    if state.pager.open {
        let theme = state.theme;
        render_pager(frame, &state.pager, area, || {
            (
                theme.background_panel,
                theme.text,
                theme.border_active,
                theme.border,
            )
        });
    }
}

fn render_messages_panel(frame: &mut Frame, state: &mut AppState, area: Rect, theme: Theme) {
    let panel = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(state.theme.border))
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(state.theme.background));

    let inner = panel.inner(area);
    frame.render_widget(panel, area);

    if state.messages.is_empty() {
        frame.render_widget(
            Paragraph::new("").style(Style::default().bg(state.theme.background)),
            inner,
        );
        render_welcome_screen(frame, state, inner, theme);
        return;
    }

    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(state.theme.background)),
        inner,
    );
    MessageList::render(frame, state, inner);

    if let Some(ref selection) = state.selection {
        crate::tui::widgets::message_list::render_selection_highlight(
            frame.buffer_mut(),
            selection,
            inner,
            &state.theme,
        );
    }

    if let AppMode::Error(msg) = &state.mode {
        render_error_block(frame, state, msg, inner);
    }
}

fn render_welcome_screen(frame: &mut Frame, state: &mut AppState, area: Rect, theme: Theme) {
    if area.width < 40 || area.height < 10 {
        let minimal = Paragraph::new(Line::from(Span::styled(
            "icode",
            Style::default()
                .fg(state.theme.primary)
                .add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().bg(state.theme.background));
        frame.render_widget(minimal, area);
        return;
    }

    let logo_lines = build_logo_lines(state);
    let logo_height = logo_lines.len() as u16;

    let prompt_preview_width = area.width.min(75);
    let prompt_height = 4u16;
    let tips_height = 3u16;

    let total_content = 4 + logo_height + 1 + prompt_height + tips_height;
    let top_spacer = if area.height > total_content + 2 {
        (area.height - total_content) / 2
    } else {
        1
    };

    let logo_start_y = area.top() + top_spacer + 4;
    let prompt_y = logo_start_y + logo_height + 1;
    let tips_y = prompt_y + prompt_height + 1;

    for (i, line) in logo_lines.iter().enumerate() {
        let line_width = line.width() as u16;
        let x = area.x + (area.width.saturating_sub(line_width)) / 2;
        frame.render_widget(
            Paragraph::new(line.clone()).style(Style::default().bg(state.theme.background)),
            Rect {
                x,
                y: logo_start_y + i as u16,
                width: line_width,
                height: 1,
            },
        );
    }

    let prompt_area = Rect {
        x: area.x + (area.width.saturating_sub(prompt_preview_width)) / 2,
        y: prompt_y,
        width: prompt_preview_width,
        height: prompt_height,
    };
    render_welcome_prompt_box(frame, state, prompt_area, theme);

    let tips_area = Rect {
        x: area.x + (area.width.saturating_sub(prompt_preview_width)) / 2,
        y: tips_y,
        width: prompt_preview_width,
        height: tips_height,
    };
    render_welcome_tips(frame, state, tips_area);
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

fn build_logo_lines(state: &AppState) -> Vec<Line<'static>> {
    let rows = [
        "__   ______                   __           ",
        "|  \\ /      \\                 |  \\          ",
        " \\$$|  $$$$$$\\  ______    ____| $$  ______  ",
        "| $$| $$   \\$$ /      \\  /      $$ /      \\ ",
        "| $$| $$      |  $$$$$$\\|  $$$$$$$|  $$$$$$\\",
        "| $$| $$   __ | $$  | $$| $$  | $$| $$    $$",
        "| $$| $$__/  \\| $$__/ $$| $$__| $$| $$$$$$$$",
        "| $$ \\$$    $$ \\$$    $$ \\$$    $$ \\$$     \\",
        " \\$$  \\$$$$$$   \\$$$$$$   \\$$$$$$$  \\$$$$$$$ ",
    ];

    let fg = state.theme.text;
    let shadow = tint_color(state.theme.background, fg, 0.25);

    let mut lines = Vec::new();
    for row_text in &rows {
        let mut row_spans = Vec::new();
        let mut current_text = String::new();
        for ch in row_text.chars() {
            if ch == '$' {
                if !current_text.is_empty() {
                    row_spans.push(Span::styled(
                        std::mem::take(&mut current_text),
                        Style::default().fg(state.theme.text_muted),
                    ));
                }
                row_spans.push(Span::styled(ch.to_string(), Style::default().fg(fg)));
            } else {
                current_text.push(ch);
            }
        }
        if !current_text.is_empty() {
            row_spans.push(Span::styled(
                current_text,
                Style::default().fg(state.theme.text_muted),
            ));
        }
        lines.push(Line::from(row_spans));
    }

    lines
}

fn render_welcome_prompt_box(frame: &mut Frame, state: &mut AppState, area: Rect, theme: Theme) {
    let agent_color = state.theme.agent_color("build");

    let border_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(agent_color))
        .style(Style::default().bg(state.theme.background));

    let inner = border_block.inner(area);
    frame.render_widget(border_block.clone(), area);

    if inner.width > 0 && inner.height > 0 {
        InputWidget::new(theme).render(inner, frame.buffer_mut(), &mut state.prompt);
    }

    let info_y = area.bottom().saturating_sub(1);
    let info_area = Rect {
        x: area.x,
        y: info_y,
        width: area.width,
        height: 1,
    };

    let left_spans = vec![
        Span::styled(
            format!("{} ", state.session.permission_mode),
            Style::default()
                .fg(agent_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(&state.session.model, Style::default().fg(state.theme.text)),
        Span::styled(
            " \u{00b7} anthropic",
            Style::default().fg(state.theme.text_muted),
        ),
    ];

    let right_spans = vec![Span::styled(
        "Ctrl+P commands",
        Style::default().fg(state.theme.text_muted),
    )];

    let left_width: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();
    let gap = info_area
        .width
        .saturating_sub(left_width as u16 + right_width as u16 + 1)
        .max(1);

    let mut all_spans = left_spans;
    all_spans.push(Span::raw(" ".repeat(gap as usize)));
    all_spans.extend(right_spans);

    frame.render_widget(
        Paragraph::new(Line::from(all_spans)).style(Style::default().bg(state.theme.background)),
        info_area,
    );
}

fn render_welcome_tips(frame: &mut Frame, state: &AppState, area: Rect) {
    let tips = [
        ("Use ", "Ctrl+P", " to open the command palette"),
        ("Press ", "Ctrl+M", " to switch models"),
        ("Type ", "/help", " to see all available commands"),
        ("Use ", "Alt+S", " to toggle the sidebar"),
    ];
    let tip = tips[state.session.turns as usize % tips.len()];

    let line = Line::from(vec![
        Span::styled(
            "\u{25cf} Tip ",
            Style::default()
                .fg(state.theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(tip.0, Style::default().fg(state.theme.text_muted)),
        Span::styled(
            tip.1,
            Style::default()
                .fg(state.theme.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(tip.2, Style::default().fg(state.theme.text_muted)),
    ]);

    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(state.theme.background)),
        area,
    );
}

fn render_keycap_row(state: &AppState) -> Vec<Span<'static>> {
    let bg = state.theme.background_element;

    fn kbd(label: &str, action: &str, bg: Color, text_color: Color) -> Vec<Span<'static>> {
        vec![
            Span::styled(
                format!("\u{250c}\u{2500}{label}\u{2500}\u{2510}"),
                Style::default().fg(text_color).bg(bg),
            ),
            Span::styled(format!(" {action}  "), Style::default().fg(text_color)),
        ]
    }

    let mut spans = Vec::new();
    spans.extend(kbd("Ctrl+P", "commands", bg, state.theme.text_muted));
    spans.extend(kbd("Ctrl+M", "models", bg, state.theme.text_muted));
    spans.extend(kbd("Alt+S", "sidebar", bg, state.theme.text_muted));
    spans.extend(kbd("Enter", "send", bg, state.theme.text_muted));
    spans
}

fn render_error_block(frame: &mut Frame, state: &AppState, msg: &str, area: Rect) {
    let error_height = 3u16;
    if area.height < error_height + 1 {
        return;
    }

    let error_area = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(error_height),
        width: area.width,
        height: error_height,
    };

    let error_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(state.theme.error))
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " ERROR ",
            Style::default()
                .fg(state.theme.error)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(ratatui::layout::Alignment::Center)
        .padding(Padding::horizontal(1));

    let inner = error_block.inner(error_area);
    frame.render_widget(error_block, error_area);

    let error_lines = vec![
        Line::from(Span::styled(
            msg.to_string(),
            Style::default().fg(state.theme.error),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to dismiss",
            Style::default()
                .fg(state.theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )),
    ];

    let error_para =
        Paragraph::new(error_lines).style(Style::default().bg(state.theme.background_element));
    frame.render_widget(error_para, inner);
}

fn render_prompt(frame: &mut Frame, state: &mut AppState, area: Rect, theme: Theme) {
    let is_streaming = state.is_streaming;

    let prompt_border_color = if is_streaming {
        state.theme.warning
    } else {
        state.theme.border_active
    };

    frame.render_widget(
        Paragraph::new("").style(Style::default().bg(state.theme.background)),
        area,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(prompt_border_color))
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .style(Style::default().bg(state.theme.background));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 || inner.width < 10 {
        return;
    }

    let input_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    let bar_area = Rect {
        x: inner.x,
        y: inner.bottom().saturating_sub(1),
        width: inner.width,
        height: 1,
    };

    if input_area.width > 0 && input_area.height > 0 {
        InputWidget::new(theme).render(input_area, frame.buffer_mut(), &mut state.prompt);
    }

    render_prompt_info_bar(frame, state, bar_area);
}

fn render_prompt_info_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let agent_color = state.theme.agent_color("build");

    let left_spans = vec![
        Span::styled(
            format!("{} ", state.session.permission_mode),
            Style::default()
                .fg(agent_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(&state.session.model, Style::default().fg(state.theme.text)),
    ];

    let total_tokens = state.session.input_tokens + state.session.output_tokens;
    let caps = capabilities_for_model(&state.session.model);
    let usage_pct = if caps.context_window > 0 {
        (total_tokens as f64 / caps.context_window as f64 * 100.0).round() as u32
    } else {
        0
    };

    let right_spans = if total_tokens > 0 {
        let usage_color = if usage_pct < 50 {
            state.theme.success
        } else if usage_pct < 80 {
            state.theme.warning
        } else {
            state.theme.error
        };
        vec![
            Span::styled(
                " \u{00b7} anthropic",
                Style::default().fg(state.theme.text_muted),
            ),
            Span::styled(format!(" ({usage_pct}%)"), Style::default().fg(usage_color)),
        ]
    } else if state.leader_active {
        vec![Span::styled(
            "u:undo  r:redo  m:model  n:new",
            Style::default().fg(state.theme.primary),
        )]
    } else if state.interrupt_count > 0 {
        vec![Span::styled(
            "Esc again to cancel",
            Style::default()
                .fg(state.theme.primary)
                .add_modifier(Modifier::BOLD),
        )]
    } else if !state.command_palette.open {
        vec![Span::styled(
            "Ctrl+P commands",
            Style::default().fg(state.theme.text_muted),
        )]
    } else {
        vec![]
    };

    let left_width: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();
    let gap = area
        .width
        .saturating_sub(left_width as u16 + right_width as u16 + 1)
        .max(1);

    let mut all_spans = left_spans;
    all_spans.push(Span::raw(" ".repeat(gap as usize)));
    all_spans.extend(right_spans);

    let bar =
        Paragraph::new(Line::from(all_spans)).style(Style::default().bg(state.theme.background));
    frame.render_widget(bar, area);
}

fn render_footer(frame: &mut Frame, state: &AppState, area: Rect) {
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

    const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    let mut right_spans = Vec::new();

    if state.lsp_count > 0 {
        right_spans.push(Span::styled("•", Style::default().fg(state.theme.success)));
        right_spans.push(Span::raw(" "));
        right_spans.push(Span::styled(
            format!("{} LSP", state.lsp_count),
            Style::default().fg(state.theme.text),
        ));
        right_spans.push(Span::raw("  "));
    }

    if state.mcp_dialog.servers.len() > 0 {
        right_spans.push(Span::styled(
            "\u{2299}",
            Style::default().fg(state.theme.success),
        ));
        right_spans.push(Span::raw(" "));
        right_spans.push(Span::styled(
            format!("{} MCP", state.mcp_dialog.servers.len()),
            Style::default().fg(state.theme.text),
        ));
        right_spans.push(Span::raw("  "));
    }

    // Turn duration timer
    let turn_duration_str = if let Some(started) = state.turn_started_at {
        let elapsed = started.elapsed();
        format_duration(elapsed)
    } else if let Some(duration) = state.last_turn_duration {
        format_duration(duration)
    } else {
        String::new()
    };

    if !turn_duration_str.is_empty() {
        right_spans.push(Span::styled(
            format!("\u{23f1} {} ", turn_duration_str),
            Style::default().fg(state.theme.info),
        ));
    }

    // Streaming indicator or context usage
    let total_tokens = state.session.input_tokens + state.session.output_tokens;
    if state.is_streaming {
        let frame_idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 100) as usize
            % SPINNER_FRAMES.len();
        right_spans.push(Span::styled(
            format!("{} generating ", SPINNER_FRAMES[frame_idx]),
            Style::default().fg(state.theme.warning),
        ));
    } else if total_tokens > 0 {
        let caps = capabilities_for_model(&state.session.model);
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
        right_spans.push(Span::styled(
            format!("{usage_pct}% "),
            Style::default().fg(usage_color),
        ));
    }

    // Cost calculation
    if total_tokens > 0 {
        if let Some(pricing) = pricing_for_model(&state.session.model) {
            let input_cost =
                (state.session.input_tokens as f64 / 1_000_000.0) * pricing.input_cost_per_million;
            let output_cost = (state.session.output_tokens as f64 / 1_000_000.0)
                * pricing.output_cost_per_million;
            let total_cost = input_cost + output_cost;
            right_spans.push(Span::styled(
                format!("{} ", format_usd(total_cost)),
                Style::default().fg(state.theme.text_muted),
            ));
        }
    }

    // Permission mode (abbreviated + color-coded)
    let (perm_label, perm_color) = match state.session.permission_mode.as_str() {
        "read-only" => ("r/o", state.theme.info),
        "workspace-write" => ("w/w", state.theme.warning),
        "danger-full-access" => ("full", state.theme.error),
        _ => (
            state.session.permission_mode.as_str(),
            state.theme.text_muted,
        ),
    };
    right_spans.push(Span::styled(
        format!("{perm_label} "),
        Style::default().fg(perm_color),
    ));

    // Model name
    right_spans.push(Span::styled(
        &state.session.model,
        Style::default().fg(state.theme.text_muted),
    ));

    // Theme toggle indicator
    let is_dark = state.theme.background == ratatui::style::Color::Rgb(10, 10, 10);
    right_spans.push(Span::raw("  "));
    right_spans.push(Span::styled(
        if is_dark { "● dark" } else { "○ light" },
        Style::default().fg(state.theme.text_muted),
    ));

    let left_str: String = left_spans.iter().map(|s| s.content.as_ref()).collect();
    let right_str: String = right_spans.iter().map(|s| s.content.as_ref()).collect();
    let left_width = left_str.chars().count();
    let right_width = right_str.chars().count();
    let gap = area
        .width
        .saturating_sub(left_width as u16 + right_width as u16);

    let mut combined = left_spans;
    if gap > 0 {
        combined.push(Span::raw(" ".repeat(gap as usize)));
    }
    combined.extend(right_spans);

    let footer =
        Paragraph::new(Line::from(combined)).style(Style::default().bg(state.theme.background));

    frame.render_widget(footer, area);
}

/// Format a duration as "M:SS" or "S:SS" for longer durations.
fn format_duration(d: std::time::Duration) -> String {
    let total_secs = d.as_secs();
    if total_secs < 60 {
        format!("0:{:02}", total_secs)
    } else {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{mins}:{secs:02}")
    }
}

fn render_slash_autocomplete(
    buf: &mut ratatui::buffer::Buffer,
    state: &crate::tui::input::InputState,
    prompt_area: Rect,
    theme: Theme,
) {
    let completions = &state.slash_completions;
    let selected = state.slash_completion_idx;
    let max_items = 5;
    let visible: Vec<_> = completions.iter().take(max_items).collect();
    if visible.is_empty() {
        return;
    }

    let max_len = visible.iter().map(|s| s.len()).max().unwrap_or(0) + 4;
    let width = (max_len as u16)
        .min(prompt_area.width.saturating_sub(2))
        .max(12);
    let height = (visible.len() as u16 + 2).min(prompt_area.height.saturating_sub(1).max(1));

    let dropdown_y = prompt_area.bottom().min(prompt_area.y + prompt_area.height);
    let dropdown_area = Rect {
        x: prompt_area.x + 1,
        y: dropdown_y,
        width,
        height,
    };

    let screen_bottom = prompt_area.y + prompt_area.height + 4;
    if dropdown_area.y >= screen_bottom {
        return;
    }
    let clamped_height = height.min(screen_bottom.saturating_sub(dropdown_area.y));
    let dropdown_area = Rect {
        height: clamped_height,
        ..dropdown_area
    };
    if dropdown_area.height < 2 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_active));
    block.render(dropdown_area, buf);

    let selected_style = Style::default().fg(theme.primary).bold();
    let normal_style = Style::default().fg(theme.text);

    for (i, cmd) in visible.iter().enumerate() {
        let line_y = dropdown_area.y + 1 + i as u16;
        if line_y >= dropdown_area.y + dropdown_area.height {
            break;
        }
        let (prefix, style) = if i == selected {
            ("\u{25b8} ", &selected_style)
        } else {
            ("  ", &normal_style)
        };
        let text = format!("{prefix}{cmd}");
        let text_width = (width as usize).saturating_sub(2);
        for (j, ch) in text.chars().enumerate() {
            if j >= text_width {
                break;
            }
            let cell_x = dropdown_area.x + 1 + j as u16;
            if let Some(cell) = buf.cell_mut((cell_x, line_y)) {
                cell.set_char(ch);
                cell.set_style(*style);
            }
        }
    }
}

fn render_toasts(frame: &mut Frame, state: &AppState, area: Rect) {
    if state.toasts.is_empty() {
        return;
    }
    let toast = state.toasts.first().unwrap();
    let (icon, color) = match toast.kind {
        ToastKind::Info => ("\u{2139}", state.theme.info),
        ToastKind::Success => ("\u{2713}", state.theme.success),
        ToastKind::Warning => ("\u{26A0}", state.theme.warning),
        ToastKind::Error => ("\u{2717}", state.theme.error),
    };
    let text = format!(" {icon} {}", toast.message);
    let toast_width = (text.chars().count() as u16 + 4)
        .min(60)
        .min(area.width.saturating_sub(4));
    let toast_height = 3u16;
    let toast_x = area.x + area.width.saturating_sub(toast_width + 2);
    let toast_y = area.y + 2;
    let toast_area = Rect {
        x: toast_x,
        y: toast_y,
        width: toast_width,
        height: toast_height,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(state.theme.background_panel));
    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);
    if inner.height > 0 {
        let para = Paragraph::new(Line::from(Span::styled(text, Style::default().fg(color))))
            .style(Style::default().bg(state.theme.background_panel));
        frame.render_widget(para, inner);
    }
}

fn render_diff_view_overlay(
    frame: &mut Frame,
    diff_view: &mut DiffView,
    area: Rect,
    theme: &Theme,
) {
    let overlay_width = area.width.saturating_sub(4).min(120).max(40);
    let overlay_height = area.height.saturating_sub(6).max(10);
    let overlay_x = (area.width.saturating_sub(overlay_width)) / 2;
    let overlay_y = (area.height.saturating_sub(overlay_height)) / 2;

    let overlay_area = Rect {
        x: area.x + overlay_x,
        y: area.y + overlay_y,
        width: overlay_width,
        height: overlay_height,
    };

    let border_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_active))
        .border_type(BorderType::Rounded)
        .title(format!(
            " {} (j/k:scroll, g/G:top/bottom, q:close) ",
            diff_view.title
        ))
        .title_style(
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.background_panel));

    let inner = border_block.inner(overlay_area);
    frame.render_widget(border_block, overlay_area);

    diff_view.render(frame, inner, theme);
}
