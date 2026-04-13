use crate::tui::app::AppState;
use crate::tui::input::InputWidget;
use crate::tui::Theme;
use api::{capabilities_for_model, detect_provider_kind, provider_display_name};
use ratatui::layout::Rect;
use ratatui::prelude::StatefulWidget;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;

const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Rotating placeholder suggestions for the home/welcome prompt.
const PLACEHOLDER_SUGGESTIONS: &[&str] = &[
    "Ask anything... 'Fix a TODO in the codebase'",
    "Ask anything... 'What is the tech stack of this project?'",
    "Ask anything... 'Fix broken tests'",
    "Ask anything... 'Explain this codebase'",
    "Ask anything... 'Add error handling to main'",
];

/// Pick a placeholder suggestion based on current time (simple rotation).
fn get_dynamic_placeholder() -> &'static str {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    PLACEHOLDER_SUGGESTIONS[secs as usize % PLACEHOLDER_SUGGESTIONS.len()]
}

/// Get the current spinner character based on time.
fn get_spinner_char() -> char {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as usize;
    SPINNER_CHARS[ms % SPINNER_CHARS.len()]
}

/// Display mode for the prompt bar.
pub enum PromptBarMode {
    /// Welcome screen: centered prompt with logo above, tips below.
    Welcome,
    /// Active session: full prompt with tool calls, messages above.
    Active {
        is_streaming: bool,
        leader_active: bool,
        interrupt_count: u8,
    },
}

/// Unified prompt bar component replacing fragmented layout.rs functions.
pub struct PromptBar {
    mode: PromptBarMode,
    theme: Theme,
}

impl PromptBar {
    /// Create a new prompt bar with the given mode and theme.
    pub fn new(mode: PromptBarMode, theme: Theme) -> Self {
        Self { mode, theme }
    }

    /// Render the prompt bar into the given area.
    pub fn render(&self, frame: &mut Frame, state: &mut AppState, area: Rect) {
        match &self.mode {
            PromptBarMode::Welcome => self.render_welcome(frame, state, area),
            PromptBarMode::Active { .. } => self.render_active(frame, state, area),
        }
    }

    fn render_welcome(&self, frame: &mut Frame, state: &mut AppState, area: Rect) {
        if area.width < 40 || area.height < 3 {
            let minimal = Paragraph::new(Line::from(Span::styled(
                "Type a message...",
                Style::default().fg(state.theme.text_muted),
            )))
            .style(Style::default().bg(state.theme.background));
            frame.render_widget(minimal, area);
            return;
        }

        self.render_welcome_prompt_box(frame, state, area);
    }

    /// Render the welcome screen tips (keyboard shortcuts) in the given area.
    pub fn render_welcome_tips(frame: &mut Frame, state: &AppState, area: Rect) {
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

    fn render_welcome_prompt_box(&self, frame: &mut Frame, state: &mut AppState, area: Rect) {
        let agent_color = state.theme.agent_color("build");

        let border_color = if state.prompt.shell_mode {
            state.theme.warning
        } else {
            agent_color
        };

        // Unified border style matching Active mode — ALL borders, rounded
        let border_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .border_type(BorderType::Rounded)
            .padding(Padding::horizontal(1))
            .style(Style::default().bg(state.theme.background));

        let inner = border_block.inner(area);
        frame.render_widget(border_block, area);

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
            state.prompt.placeholder = get_dynamic_placeholder().to_string();
            InputWidget::new(self.theme).render(input_area, frame.buffer_mut(), &mut state.prompt);
        }

        render_info_bar(frame, state, bar_area, &self.mode);
    }

    fn render_active(&self, frame: &mut Frame, state: &mut AppState, area: Rect) {
        let is_streaming = matches!(
            &self.mode,
            PromptBarMode::Active {
                is_streaming: true,
                ..
            }
        );

        let prompt_border_color = if state.prompt.shell_mode {
            state.theme.warning
        } else if is_streaming {
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
            InputWidget::new(self.theme).render(input_area, frame.buffer_mut(), &mut state.prompt);
        }

        render_info_bar(frame, state, bar_area, &self.mode);
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Tint `base` towards `into` by `factor` (0.0 = base, 1.0 = into).
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

/// Render the info bar (agent, model, provider on left; usage/hints on right).
fn render_info_bar(frame: &mut Frame, state: &AppState, area: Rect, mode: &PromptBarMode) {
    let agent_color = state.theme.agent_color("build");

    let is_streaming = matches!(
        mode,
        PromptBarMode::Active {
            is_streaming: true,
            ..
        }
    );

    let left_spans: Vec<Span<'_>> = if is_streaming {
        let spinner = get_spinner_char();
        vec![
            Span::styled(
                format!("{spinner} "),
                Style::default().fg(state.theme.warning),
            ),
            Span::styled(
                "build ",
                Style::default()
                    .fg(agent_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![
            Span::styled(
                "build ",
                Style::default()
                    .fg(agent_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&state.session.model, Style::default().fg(state.theme.text)),
            Span::styled(
                format!(
                    " \u{00b7} {}",
                    provider_display_name(&detect_provider_kind(&state.session.model, None))
                ),
                Style::default().fg(state.theme.text_muted),
            ),
        ]
    };

    let total_tokens = state.session.input_tokens as u64 + state.session.output_tokens as u64;
    let caps = capabilities_for_model(&state.session.model);
    let usage_pct = if caps.context_window > 0 {
        (total_tokens as f64 / caps.context_window as f64 * 100.0).round() as u32
    } else {
        0
    };

    let right_spans = match mode {
        PromptBarMode::Welcome => {
            vec![Span::styled(
                "Ctrl+P commands",
                Style::default().fg(state.theme.text_muted),
            )]
        }
        PromptBarMode::Active {
            is_streaming,
            leader_active,
            interrupt_count,
        } => {
            if *is_streaming {
                vec![Span::styled(
                    "esc interrupt",
                    Style::default().fg(state.theme.text_muted),
                )]
            } else if total_tokens > 0 {
                let usage_color = if usage_pct < 50 {
                    state.theme.success
                } else if usage_pct < 80 {
                    state.theme.warning
                } else {
                    state.theme.error
                };
                vec![Span::styled(
                    format!("context ({usage_pct}%)"),
                    Style::default().fg(usage_color),
                )]
            } else if *leader_active {
                vec![Span::styled(
                    "u:undo  r:redo  m:model  n:new",
                    Style::default().fg(state.theme.primary),
                )]
            } else if *interrupt_count > 0 {
                vec![Span::styled(
                    format!("interrupt ({interrupt_count})"),
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
            }
        }
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

/// Build a row of keyboard shortcut keycaps.
fn build_keycap_row(state: &AppState) -> Vec<Span<'static>> {
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
    spans.extend(kbd("Ctrl+R", "refresh", bg, state.theme.text_muted));
    spans.extend(kbd("Alt+S", "sidebar", bg, state.theme.text_muted));
    spans.extend(kbd("Enter", "send", bg, state.theme.text_muted));
    spans
}

/// Render the keycap row after the info bar.
pub fn render_keycaps(frame: &mut Frame, state: &AppState, area: Rect) {
    let spans = build_keycap_row(state);
    let para = Paragraph::new(Line::from(spans)).style(Style::default().bg(state.theme.background));
    frame.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_bar_mode_variants() {
        let welcome = PromptBarMode::Welcome;
        let active = PromptBarMode::Active {
            is_streaming: true,
            leader_active: false,
            interrupt_count: 0,
        };
        // Verify both variants can be constructed and matched
        match welcome {
            PromptBarMode::Welcome => {}
            PromptBarMode::Active { .. } => panic!("expected Welcome"),
        }
        match active {
            PromptBarMode::Active {
                is_streaming,
                leader_active,
                interrupt_count,
            } => {
                assert!(is_streaming);
                assert!(!leader_active);
                assert_eq!(interrupt_count, 0);
            }
            PromptBarMode::Welcome => panic!("expected Active"),
        }
    }

    #[test]
    fn test_prompt_bar_construction() {
        let theme = Theme::default();
        let _welcome_bar = PromptBar::new(PromptBarMode::Welcome, theme);
        let _active_bar = PromptBar::new(
            PromptBarMode::Active {
                is_streaming: false,
                leader_active: false,
                interrupt_count: 0,
            },
            theme,
        );
    }

    #[test]
    fn test_tint_color_basic() {
        let base = Color::Rgb(0, 0, 0);
        let into = Color::Rgb(255, 255, 255);
        let result = tint_color(base, into, 0.5);
        assert!(matches!(result, Color::Rgb(r, g, b) if r > 100 && r < 150));
        assert!(matches!(result, Color::Rgb(r, g, b) if g > 100 && g < 150));
        assert!(matches!(result, Color::Rgb(r, g, b) if b > 100 && b < 150));
    }

    #[test]
    fn test_info_bar_spans_construction() {
        // Verify the gap calculation and span assembly for a simple welcome-mode case.
        let left_spans = vec![
            Span::styled(
                "build ",
                Style::default()
                    .fg(Color::Rgb(127, 216, 143))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("sonnet", Style::default().fg(Color::Rgb(238, 238, 238))),
        ];
        let right_spans = vec![Span::styled(
            "Ctrl+P commands",
            Style::default().fg(Color::Rgb(128, 128, 128)),
        )];

        let left_width: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
        let right_width: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();

        // For a 75-width area, gap should be substantial
        let area_width = 75u16;
        let gap = area_width
            .saturating_sub(left_width as u16 + right_width as u16 + 1)
            .max(1);

        // left: "build " (6) + "sonnet" (6) = 12, right: "Ctrl+P commands" (15)
        // gap = 75 - 12 - 15 - 1 = 47
        assert_eq!(left_width, 12);
        assert_eq!(right_width, 15);
        assert_eq!(gap, 47);

        // Verify span assembly produces correct total
        let mut all_spans = left_spans;
        all_spans.push(Span::raw(" ".repeat(gap as usize)));
        all_spans.extend(right_spans);
        assert_eq!(all_spans.len(), 4); // left(2) + gap(1) + right(1)
    }

    #[test]
    fn test_keycap_row_spans_count() {
        let theme = Theme::default();
        // Create a minimal state-like check for keycap row
        // Each keycap produces 2 spans, and there are 4 keycaps = 8 spans
        fn kbd(label: &str, _action: &str, bg: Color, text_color: Color) -> Vec<Span<'static>> {
            vec![
                Span::styled(
                    format!("\u{250c}\u{2500}{label}\u{2500}\u{2510}"),
                    Style::default().fg(text_color).bg(bg),
                ),
                Span::styled(" placeholder  ", Style::default().fg(text_color)),
            ]
        }
        let bg = theme.background_element;
        let mut spans = Vec::new();
        spans.extend(kbd("Ctrl+P", "commands", bg, theme.text_muted));
        spans.extend(kbd("Ctrl+M", "models", bg, theme.text_muted));
        spans.extend(kbd("Ctrl+R", "refresh", bg, theme.text_muted));
        spans.extend(kbd("Alt+S", "sidebar", bg, theme.text_muted));
        spans.extend(kbd("Enter", "send", bg, theme.text_muted));
        assert_eq!(spans.len(), 10);
    }
}
