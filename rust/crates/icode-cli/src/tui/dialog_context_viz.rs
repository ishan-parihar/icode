use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, Clear, Gauge, Paragraph};
use ratatui::Frame;

use crate::tui::context_suggestions::{generate_suggestions, ContextVizData, SuggestionSeverity};
use crate::tui::popup_utils::PopupConfig;
use crate::tui::theme::Theme;

#[derive(Debug, Clone)]
pub struct ContextVizDialogState {
    pub open: bool,
}

impl ContextVizDialogState {
    pub fn new() -> Self {
        Self { open: false }
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }
}

impl Default for ContextVizDialogState {
    fn default() -> Self {
        Self::new()
    }
}

fn dialog_width(term_width: u16) -> u16 {
    ((term_width as f32 * 0.6) as u16).clamp(55, 80)
}

fn dialog_height(_term_height: u16) -> u16 {
    28
}

pub fn render_context_viz_dialog(
    frame: &mut Frame,
    state: &ContextVizDialogState,
    area: Rect,
    theme: Theme,
    current_model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_create_tokens: u32,
    cache_read_tokens: u32,
    context_window: u32,
    turns: u32,
    message_count: usize,
    cumulative_cost: f64,
    budget_max: Option<f64>,
    budget_remaining: Option<f64>,
    compaction_count: u32,
    compaction_removed_messages: u32,
    effort_level: &str,
) {
    if !state.open {
        return;
    }

    let width = dialog_width(area.width).min(area.width.saturating_sub(4));
    let height = dialog_height(area.height).min(area.height.saturating_sub(4));
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let config = PopupConfig::full("Context");
    let block = config.to_block(theme);

    frame.render_widget(block.clone(), dialog_area);
    let inner = dialog_area.inner(Margin::new(1, 1));

    let constraints = [
        Constraint::Length(1), // model + effort
        Constraint::Length(3), // usage gauge
        Constraint::Length(1), // separator
        Constraint::Length(1), // token breakdown
        Constraint::Length(1), // cache breakdown
        Constraint::Length(1), // session stats
        Constraint::Length(1), // cost estimate
        Constraint::Length(1), // budget section (or spacer)
        Constraint::Length(1), // compaction info
        Constraint::Length(1), // separator
        Constraint::Min(1),    // optimization suggestions
        Constraint::Length(1), // footer hint
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // Section 1: Model name + effort level
    let model_line = Line::from(vec![
        Span::styled("Model: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            current_model,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  ", Style::default().fg(theme.text_muted)),
        Span::styled("Effort: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            effort_level,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(model_line), chunks[0]);

    // Section 2: Usage gauge
    let total_tokens = input_tokens + output_tokens + cache_create_tokens + cache_read_tokens;
    let usage_pct = if context_window > 0 {
        (total_tokens as f64 / context_window as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let usage_ratio = usage_pct / 100.0;

    let gauge_color = if usage_pct < 50.0 {
        theme.success
    } else if usage_pct < 80.0 {
        theme.warning
    } else {
        theme.error
    };

    let gauge_label = if context_window > 0 {
        format!(
            "{} / {} tokens ({:.0}%)",
            format_token_count(total_tokens),
            format_token_count(context_window),
            usage_pct
        )
    } else {
        format!("{} tokens used", format_token_count(total_tokens))
    };

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(gauge_color))
        .ratio(usage_ratio)
        .label(Span::styled(
            gauge_label,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(gauge, chunks[1]);

    // Section 3: Token breakdown (Input / Output)
    let token_breakdown = Line::from(vec![
        Span::styled("Input:  ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format_token_count(input_tokens),
            Style::default().fg(theme.info),
        ),
        Span::styled("   Output: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format_token_count(output_tokens),
            Style::default().fg(theme.accent),
        ),
    ]);
    frame.render_widget(Paragraph::new(token_breakdown), chunks[3]);

    // Section 3b: Cache breakdown
    let cache_line = Line::from(vec![
        Span::styled("Cache Create: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format_token_count(cache_create_tokens),
            Style::default().fg(theme.info),
        ),
        Span::styled("   Cache Read: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format_token_count(cache_read_tokens),
            Style::default().fg(theme.success),
        ),
    ]);
    frame.render_widget(Paragraph::new(cache_line), chunks[4]);

    // Section 4: Session stats
    let session_stats = Line::from(vec![
        Span::styled("Turns:  ", Style::default().fg(theme.text_muted)),
        Span::styled(
            turns.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("   Messages: ", Style::default().fg(theme.text_muted)),
        Span::styled(
            message_count.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(session_stats), chunks[5]);

    // Section 5: Cost estimate
    let cost_line = render_cost_estimate(
        input_tokens,
        output_tokens,
        cache_create_tokens,
        cache_read_tokens,
        current_model,
        theme,
    );
    frame.render_widget(Paragraph::new(cost_line), chunks[6]);

    // Section 6: Budget section (conditional)
    if let (Some(max), Some(remaining)) = (budget_max, budget_remaining) {
        let budget_pct = if max > 0.0 {
            (remaining / max * 100.0).min(100.0)
        } else {
            0.0
        };
        let budget_color = if budget_pct > 50.0 {
            theme.success
        } else if budget_pct > 20.0 {
            theme.warning
        } else {
            theme.error
        };
        let budget_line = Line::from(vec![
            Span::styled("Budget: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("${remaining:.2} / ${max:.2}"),
                Style::default()
                    .fg(budget_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" ({budget_pct:.0}%)"),
                Style::default().fg(budget_color),
            ),
        ]);
        frame.render_widget(Paragraph::new(budget_line), chunks[7]);
    } else {
        let no_budget = Line::from(vec![Span::styled(
            "Budget: no limit set",
            Style::default().fg(theme.text_muted),
        )]);
        frame.render_widget(Paragraph::new(no_budget), chunks[7]);
    }

    // Section 7: Compaction info
    let compaction_line = if compaction_count > 0 {
        Line::from(vec![
            Span::styled("Compaction: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{compaction_count}x"),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" ({compaction_removed_messages} msgs removed)"),
                Style::default().fg(theme.text_muted),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("Compaction: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                "none",
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(compaction_line), chunks[8]);

    // Section 8: Separator + optimization suggestions
    let sep = Span::styled(
        "\u{2500}".repeat(dialog_area.width.saturating_sub(2) as usize),
        Style::default().fg(theme.border),
    );
    frame.render_widget(Paragraph::new(Line::from(sep)), chunks[9]);

    let viz_data = ContextVizData {
        model: current_model.into(),
        input_tokens,
        output_tokens,
        cache_create_tokens,
        cache_read_tokens,
        context_window,
        turns,
        message_count,
        cumulative_cost,
        budget_max,
        budget_remaining,
        compaction_count,
        compaction_removed: compaction_removed_messages,
    };
    let suggestions = generate_suggestions(&viz_data);

    if suggestions.is_empty() {
        let no_suggestions = Line::from(vec![Span::styled(
            " Suggestions: all systems nominal ",
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::ITALIC),
        )]);
        frame.render_widget(
            Paragraph::new(no_suggestions).alignment(Alignment::Center),
            chunks[10],
        );
    } else {
        let mut suggestion_lines: Vec<Line> = Vec::new();
        for (i, s) in suggestions.iter().enumerate() {
            let severity_icon = match s.severity {
                SuggestionSeverity::Info => "\u{2139}",
                SuggestionSeverity::Warning => "\u{26A0}",
                SuggestionSeverity::Critical => "\u{2717}",
            };
            let severity_color = match s.severity {
                SuggestionSeverity::Info => theme.info,
                SuggestionSeverity::Warning => theme.warning,
                SuggestionSeverity::Critical => theme.error,
            };
            let mut spans = vec![
                Span::styled(
                    format!("{severity_icon} "),
                    Style::default().fg(severity_color),
                ),
                Span::styled(
                    &s.message,
                    Style::default()
                        .fg(severity_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if let Some(action) = &s.action {
                spans.push(Span::styled(
                    format!("  [{action}]"),
                    Style::default()
                        .fg(theme.text_muted)
                        .add_modifier(Modifier::ITALIC),
                ));
            }
            suggestion_lines.push(Line::from(spans));
            if i < suggestions.len() - 1 {
                suggestion_lines.push(Line::from(""));
            }
        }
        frame.render_widget(Paragraph::new(suggestion_lines), chunks[10]);
    }

    // Section 9: Footer hint
    let hint = Span::styled(
        " Esc / q: close ",
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(
        Paragraph::new(hint).alignment(Alignment::Center),
        chunks[11],
    );
}

fn format_token_count(count: u32) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn render_cost_estimate(
    input_tokens: u32,
    output_tokens: u32,
    cache_create_tokens: u32,
    cache_read_tokens: u32,
    model: &str,
    theme: Theme,
) -> Line<'static> {
    let (input_per_m, output_per_m, cache_create_per_m, cache_read_per_m) =
        if model.contains("opus") {
            (15.0, 75.0, 18.75, 1.5)
        } else if model.contains("sonnet") {
            (3.0, 15.0, 18.75, 1.5)
        } else if model.contains("haiku") {
            (0.80, 4.0, 1.25, 0.1)
        } else if model.contains("gpt-4o") {
            (2.50, 10.0, 3.0, 0.75)
        } else if model.contains("gemini") {
            (1.25, 5.0, 1.50, 0.50)
        } else {
            (3.0, 15.0, 18.75, 1.5)
        };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * input_per_m;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * output_per_m;
    let cache_create_cost = (cache_create_tokens as f64 / 1_000_000.0) * cache_create_per_m;
    let cache_read_cost = (cache_read_tokens as f64 / 1_000_000.0) * cache_read_per_m;
    let total_cost = input_cost + output_cost + cache_create_cost + cache_read_cost;

    let cost_color = if total_cost < 0.10 {
        theme.success
    } else if total_cost < 1.00 {
        theme.warning
    } else {
        theme.error
    };

    Line::from(vec![
        Span::styled("Est. cost:  ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format!("${total_cost:.4}"),
            Style::default().fg(cost_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  (in ${input_per_m:.2}/M, out ${output_per_m:.2}/M)"),
            Style::default().fg(theme.text_muted),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    #[test]
    fn test_dialog_renders_with_all_data() {
        let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                let mut state = ContextVizDialogState::new();
                state.open();
                render_context_viz_dialog(
                    frame,
                    &state,
                    area,
                    Theme::dark(),
                    "claude-sonnet-4-6",
                    10_000,
                    5_000,
                    3_000,
                    2_000,
                    200_000,
                    10,
                    20,
                    0.42,
                    Some(10.0),
                    Some(9.58),
                    1,
                    4,
                    "balanced",
                );
            })
            .unwrap();
    }

    #[test]
    fn test_dialog_handles_none_budget() {
        let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                let mut state = ContextVizDialogState::new();
                state.open();
                render_context_viz_dialog(
                    frame,
                    &state,
                    area,
                    Theme::dark(),
                    "claude-sonnet-4-6",
                    10_000,
                    5_000,
                    3_000,
                    2_000,
                    200_000,
                    10,
                    20,
                    0.42,
                    None,
                    None,
                    0,
                    0,
                    "balanced",
                );
            })
            .unwrap();
    }
}
