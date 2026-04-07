use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Modifier, StatefulWidget, Widget};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use ratatui::Frame;

use crate::tui::input::InputState;
use crate::tui::markdown;
use crate::tui::theme::Theme;

// ---------------------------------------------------------------------------
// Question types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum QuestionType {
    Text {
        placeholder: String,
    },
    Select {
        options: Vec<String>,
        default_idx: Option<usize>,
    },
    MultiSelect {
        options: Vec<String>,
        defaults: Vec<usize>,
    },
}

impl Default for QuestionType {
    fn default() -> Self {
        QuestionType::Text {
            placeholder: "Type your answer...".into(),
        }
    }
}

pub struct QuestionResponse {
    pub answer: String,
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub struct QuestionPromptState {
    pub open: bool,
    pub question: String,
    pub agent: String,
    pub context: String,
    pub question_type: QuestionType,
    pub answer: InputState,
    pub selected_options: Vec<bool>,
    pub cursor_idx: usize,
    pub answer_tx: Option<std::sync::mpsc::Sender<String>>,
}

impl Default for QuestionPromptState {
    fn default() -> Self {
        Self {
            open: false,
            question: String::new(),
            agent: String::new(),
            context: String::new(),
            question_type: QuestionType::default(),
            answer: InputState::new("\u{203a} "),
            selected_options: Vec::new(),
            cursor_idx: 0,
            answer_tx: None,
        }
    }
}

impl QuestionPromptState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(
        &mut self,
        question: String,
        agent: String,
        context: String,
        question_type: QuestionType,
        answer_tx: Option<std::sync::mpsc::Sender<String>>,
    ) {
        self.open = true;
        self.question = question;
        self.agent = agent;
        self.context = context;
        self.question_type = question_type;
        self.answer_tx = answer_tx;

        match &self.question_type {
            QuestionType::Text { placeholder } => {
                self.answer = InputState::new("\u{203a} ");
                self.answer.placeholder = placeholder.clone();
                self.selected_options.clear();
                self.cursor_idx = 0;
            }
            QuestionType::Select {
                options,
                default_idx,
            } => {
                let count = options.len();
                self.selected_options = vec![false; count];
                if let Some(idx) = *default_idx {
                    if idx < count {
                        self.selected_options[idx] = true;
                        self.cursor_idx = idx;
                    }
                } else {
                    self.cursor_idx = 0;
                }
            }
            QuestionType::MultiSelect { options, defaults } => {
                let count = options.len();
                self.selected_options = vec![false; count];
                for &idx in defaults {
                    if idx < count {
                        self.selected_options[idx] = true;
                    }
                }
                self.cursor_idx = 0;
            }
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.question.clear();
        self.agent.clear();
        self.context.clear();
        self.question_type = QuestionType::default();
        self.answer_tx = None;
        self.answer = InputState::new("\u{203a} ");
        self.selected_options.clear();
        self.cursor_idx = 0;
    }

    pub fn submit(&mut self) -> QuestionResponse {
        let answer = match &self.question_type {
            QuestionType::Text { .. } => {
                let val = self.answer.submit();
                if val.is_empty() {
                    String::from("[cancelled]")
                } else {
                    val
                }
            }
            QuestionType::Select { options, .. } => {
                let idx = self
                    .selected_options
                    .iter()
                    .position(|&s| s)
                    .unwrap_or(self.cursor_idx.min(options.len().saturating_sub(1)));
                options.get(idx).cloned().unwrap_or_default()
            }
            QuestionType::MultiSelect { options, .. } => {
                let selected: Vec<&str> = self
                    .selected_options
                    .iter()
                    .zip(options.iter())
                    .filter_map(|(&sel, opt)| if sel { Some(opt.as_str()) } else { None })
                    .collect();
                if selected.is_empty() {
                    String::from("[cancelled]")
                } else {
                    selected.join(", ")
                }
            }
        };

        let cancelled = answer == "[cancelled]";

        if let Some(tx) = self.answer_tx.take() {
            let _ = tx.send(if cancelled {
                String::new()
            } else {
                answer.clone()
            });
        }
        self.close();
        QuestionResponse { answer, cancelled }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor_idx > 0 {
            self.cursor_idx -= 1;
        }
    }

    pub fn cursor_down(&mut self) {
        let len = self.selected_options.len();
        if len > 0 && self.cursor_idx < len.saturating_sub(1) {
            self.cursor_idx += 1;
        }
    }

    pub fn toggle_current(&mut self) {
        if self.cursor_idx < self.selected_options.len() {
            if matches!(self.question_type, QuestionType::Select { .. }) {
                self.selected_options.fill(false);
            }
            self.selected_options[self.cursor_idx] = !self.selected_options[self.cursor_idx];
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

fn popup_width(screen_width: u16) -> u16 {
    let w = (screen_width as f32 * 0.60) as u16;
    w.clamp(40, 80)
}

fn popup_height(screen_height: u16, state: &QuestionPromptState) -> u16 {
    let base: u16 = 8;
    let question_lines = {
        let w = (popup_width(screen_height) as usize).saturating_sub(4);
        let md = markdown::render_markdown_to_lines(&state.question, w, &Theme::dark());
        md.len().saturating_sub(1) as u16
    };
    let option_lines = match &state.question_type {
        QuestionType::Text { .. } => 2u16,
        QuestionType::Select { options, .. } | QuestionType::MultiSelect { options, .. } => {
            options.len() as u16 + 1
        }
    };
    let context_lines = if state.context.is_empty() { 0 } else { 2 };
    (base + question_lines + option_lines + context_lines)
        .min(screen_height.saturating_sub(4))
        .max(8)
}

fn popup_area(frame: Rect, state: &QuestionPromptState) -> Rect {
    let width = popup_width(frame.width);
    let height = popup_height(frame.height, state);
    let x = frame.x + (frame.width - width) / 2;
    let y = frame.y + (frame.height - height) / 2;
    Rect::new(x, y, width, height)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render_question_prompt(
    frame: &mut Frame,
    area: Rect,
    state: &mut QuestionPromptState,
    theme: &Theme,
) {
    if !state.open {
        return;
    }

    let popup = popup_area(area, state);
    let popup = Rect {
        x: popup.x.max(area.x),
        y: popup.y.max(area.y),
        width: popup.width.min(area.width),
        height: popup.height.min(area.height),
    };

    if popup.width < 20 || popup.height < 6 {
        return;
    }

    frame.render_widget(Clear, popup);

    let question_color = theme.accent;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(question_color))
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" {} asks ", state.agent),
            Style::default()
                .fg(question_color)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(theme.background_panel));

    frame.render_widget(block.clone(), popup);
    let inner = block.inner(popup);

    // Layout: question area | (context) | input/options | hints
    let content_width = inner.width as usize;
    let md_lines =
        markdown::render_markdown_to_lines(&state.question, content_width.saturating_sub(2), theme);
    let max_question_lines = ((inner.height as usize).saturating_sub(6)).max(1);
    let question_text: Vec<Line<'_>> = md_lines
        .iter()
        .take(max_question_lines)
        .map(|l| {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(l.spans.clone());
            Line::from(spans)
        })
        .collect();

    let has_context = !state.context.is_empty();
    let is_text = matches!(state.question_type, QuestionType::Text { .. });
    let options_len = match &state.question_type {
        QuestionType::Text { .. } => 0,
        QuestionType::Select { options, .. } | QuestionType::MultiSelect { options, .. } => {
            options.len()
        }
    };

    let mut constraints = vec![Constraint::Length((question_text.len() as u16).max(1))];
    if has_context {
        constraints.push(Constraint::Length(2));
    }
    if is_text {
        constraints.push(Constraint::Length(3));
    } else {
        constraints.push(Constraint::Length((options_len as u16).max(1) + 1));
    }
    constraints.push(Constraint::Length(1));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut chunk_idx = 0;

    let question_para = Paragraph::new(question_text)
        .alignment(Alignment::Left)
        .style(Style::default().fg(theme.text));
    frame.render_widget(question_para, chunks[chunk_idx]);
    chunk_idx += 1;

    if has_context {
        let ctx_para = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                state.context.clone(),
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        frame.render_widget(ctx_para, chunks[chunk_idx]);
        chunk_idx += 1;
    }

    if is_text {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(question_color))
            .border_type(BorderType::Rounded)
            .padding(Padding::horizontal(1))
            .style(Style::default().bg(theme.background_element));
        let input_inner = input_block.inner(chunks[chunk_idx]);
        frame.render_widget(input_block.clone(), chunks[chunk_idx]);

        if input_inner.width > 0 && input_inner.height > 0 {
            crate::tui::InputWidget::new(*theme).render(
                input_inner,
                frame.buffer_mut(),
                &mut state.answer,
            );
        }
    } else {
        let opt_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                (0..options_len)
                    .map(|_| Constraint::Length(1))
                    .collect::<Vec<_>>(),
            )
            .split(chunks[chunk_idx]);

        let is_multi = matches!(state.question_type, QuestionType::MultiSelect { .. });

        let option_iter: Box<dyn Iterator<Item = (usize, &String)>> = match &state.question_type {
            QuestionType::Select { options, .. } => Box::new(options.iter().enumerate()),
            QuestionType::MultiSelect { options, .. } => Box::new(options.iter().enumerate()),
            QuestionType::Text { .. } => Box::new(std::iter::empty()),
        };

        for (i, opt) in option_iter {
            let is_selected = i == state.cursor_idx;
            let is_chosen = state.selected_options.get(i).copied().unwrap_or(false);

            let prefix = if is_multi {
                if is_chosen {
                    Span::styled(
                        "\u{2705} ",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw("\u{2b1c} ")
                }
            } else if is_chosen {
                Span::styled(
                    "\u{25b6} ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw("  ")
            };

            let cursor_indicator = if is_selected {
                Span::styled(
                    "\u{2022} ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw("  ")
            };

            let label_style = if is_selected {
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };

            let line = Line::from(vec![
                cursor_indicator,
                prefix,
                Span::styled(opt.to_string(), label_style),
            ]);

            let line_area = Rect {
                x: opt_chunks[i].x,
                y: opt_chunks[i].y,
                width: opt_chunks[i].width,
                height: 1,
            };

            if is_selected {
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().bg(theme.background_element)),
                    line_area,
                );
            } else {
                frame.render_widget(Paragraph::new(line), line_area);
            }
        }
    }
    chunk_idx += 1;

    let hints = build_hints(&state.question_type, theme);
    frame.render_widget(
        Paragraph::new(hints).style(Style::default().bg(theme.background_panel)),
        chunks[chunk_idx],
    );
}

fn build_hints(qt: &QuestionType, theme: &Theme) -> Line<'static> {
    let key_style = |bg| {
        Style::default()
            .fg(theme.background_panel)
            .bg(bg)
            .add_modifier(Modifier::BOLD)
    };
    let muted = Style::default().fg(theme.text_muted);

    match qt {
        QuestionType::Text { .. } => Line::from(vec![
            Span::styled(" Enter ", key_style(theme.success)),
            Span::styled(" to submit  ", muted),
            Span::styled(" Esc ", key_style(theme.error)),
            Span::styled(" to skip", muted),
        ]),
        QuestionType::Select { .. } => Line::from(vec![
            Span::styled(" \u{2191}/\u{2193} ", key_style(theme.accent)),
            Span::styled(" navigate  ", muted),
            Span::styled(" Enter ", key_style(theme.success)),
            Span::styled(" select  ", muted),
            Span::styled(" Esc ", key_style(theme.error)),
            Span::styled(" to skip", muted),
        ]),
        QuestionType::MultiSelect { .. } => Line::from(vec![
            Span::styled(" \u{2191}/\u{2193} ", key_style(theme.accent)),
            Span::styled(" navigate  ", muted),
            Span::styled(" Space ", key_style(theme.accent)),
            Span::styled(" toggle  ", muted),
            Span::styled(" Enter ", key_style(theme.success)),
            Span::styled(" confirm  ", muted),
            Span::styled(" Esc ", key_style(theme.error)),
            Span::styled(" to skip", muted),
        ]),
    }
}
