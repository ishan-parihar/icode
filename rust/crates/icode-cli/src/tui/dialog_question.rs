use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Modifier, StatefulWidget, Widget};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::popup_utils;
use crate::tui::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub enum QuestionType {
    Text { placeholder: String },
    Select,
    MultiSelect,
}

impl Default for QuestionType {
    fn default() -> Self {
        QuestionType::Text {
            placeholder: "Type your answer...".into(),
        }
    }
}

impl QuestionType {
    pub fn text(placeholder: &str) -> Self {
        QuestionType::Text {
            placeholder: placeholder.into(),
        }
    }
    pub fn select(_options: Vec<String>, _default_idx: Option<usize>) -> Self {
        QuestionType::Select
    }
    pub fn multi_select(_options: Vec<String>, _defaults: Vec<usize>) -> Self {
        QuestionType::MultiSelect
    }
}

pub struct QuestionResponse {
    pub answer: String,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

impl QuestionOption {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: String::new(),
        }
    }
    pub fn with_description(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
        }
    }
}

pub const CUSTOM_ANSWER_LABEL: &str = "Type your own answer";

#[derive(Debug, Clone)]
pub struct QuestionDef {
    pub header: String,
    pub question: String,
    pub question_type: QuestionType,
    pub options: Vec<QuestionOption>,
}

impl QuestionDef {
    pub fn text(header: impl Into<String>, question: impl Into<String>, placeholder: &str) -> Self {
        Self {
            header: header.into(),
            question: question.into(),
            question_type: QuestionType::Text {
                placeholder: placeholder.into(),
            },
            options: Vec::new(),
        }
    }
    pub fn select(
        header: impl Into<String>,
        question: impl Into<String>,
        options: Vec<QuestionOption>,
    ) -> Self {
        Self {
            header: header.into(),
            question: question.into(),
            question_type: QuestionType::Select,
            options,
        }
    }
    pub fn multi_select(
        header: impl Into<String>,
        question: impl Into<String>,
        options: Vec<QuestionOption>,
    ) -> Self {
        Self {
            header: header.into(),
            question: question.into(),
            question_type: QuestionType::MultiSelect,
            options,
        }
    }
    fn option_count(&self) -> usize {
        match &self.question_type {
            QuestionType::Text { .. } => 0,
            QuestionType::Select | QuestionType::MultiSelect => self.options.len(),
        }
    }
    fn is_custom_answer_idx(&self, idx: usize) -> bool {
        self.options
            .get(idx)
            .is_some_and(|o| o.label == CUSTOM_ANSWER_LABEL)
    }
}

#[derive(Default)]
pub struct QuestionPromptState {
    pub open: bool,
    pub agent: String,
    pub context: String,
    pub questions: Vec<QuestionDef>,
    pub answers: Vec<Vec<String>>,
    pub active_tab: usize,
    pub cursor_idx: usize,
    pub scroll_offset: usize,
    pub custom_input: String,
    pub editing_custom: bool,
    pub answer_tx: Option<std::sync::mpsc::Sender<String>>,
}

impl QuestionPromptState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(
        &mut self,
        questions: Vec<QuestionDef>,
        agent: String,
        context: String,
        answer_tx: Option<std::sync::mpsc::Sender<String>>,
    ) {
        self.open = true;
        self.agent = agent;
        self.context = context;
        self.questions = questions;
        self.answers = vec![Vec::new(); self.questions.len()];
        self.active_tab = 0;
        self.cursor_idx = 0;
        self.scroll_offset = 0;
        self.custom_input.clear();
        self.editing_custom = false;
        self.answer_tx = answer_tx;
    }

    pub fn open_single(
        &mut self,
        question: String,
        agent: String,
        context: String,
        question_type: QuestionType,
        answer_tx: Option<std::sync::mpsc::Sender<String>>,
    ) {
        let header = match &question_type {
            QuestionType::Text { .. } => "Answer",
            QuestionType::Select => "Choose",
            QuestionType::MultiSelect => "Select",
        };
        let qdef = QuestionDef {
            header: header.to_string(),
            question,
            question_type,
            options: Vec::new(),
        };
        self.open(vec![qdef], agent, context, answer_tx);
    }

    pub fn close(&mut self) {
        self.open = false;
        self.agent.clear();
        self.context.clear();
        self.questions.clear();
        self.answers.clear();
        self.active_tab = 0;
        self.cursor_idx = 0;
        self.scroll_offset = 0;
        self.custom_input.clear();
        self.editing_custom = false;
        self.answer_tx = None;
    }

    fn is_last_question(&self) -> bool {
        self.active_tab >= self.questions.len().saturating_sub(1)
    }

    fn advance_tab(&mut self) {
        if self.active_tab < self.questions.len() {
            self.active_tab += 1;
            self.cursor_idx = 0;
            self.scroll_offset = 0;
            self.custom_input.clear();
            self.editing_custom = false;
        }
    }

    fn collect_answers(&self) -> String {
        if self.questions.len() == 1 {
            self.answers
                .first()
                .and_then(|a| a.first())
                .cloned()
                .unwrap_or_default()
        } else {
            let parts: Vec<String> = self
                .questions
                .iter()
                .zip(self.answers.iter())
                .map(|(q, a)| {
                    if a.is_empty() {
                        format!("{}: [skipped]", q.header)
                    } else {
                        format!("{}: {}", q.header, a.join(", "))
                    }
                })
                .collect();
            parts.join(" | ")
        }
    }

    pub fn submit(&mut self) -> QuestionResponse {
        let answer = self.collect_answers();
        let cancelled = answer.is_empty() || answer == "[cancelled]";
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
            self.ensure_visible_options(10);
        }
    }

    pub fn cursor_down(&mut self) {
        if self.active_tab < self.questions.len() {
            let len = self.questions[self.active_tab].option_count();
            if len > 0 && self.cursor_idx < len.saturating_sub(1) {
                self.cursor_idx += 1;
                self.ensure_visible_options(10);
            }
        }
    }

    fn ensure_visible_options(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        if self.cursor_idx < self.scroll_offset {
            self.scroll_offset = self.cursor_idx;
        } else if self.cursor_idx >= self.scroll_offset + visible_count {
            self.scroll_offset = self.cursor_idx - visible_count + 1;
        }
    }

    pub fn tab_left(&mut self) {
        if self.active_tab > 0 {
            self.active_tab -= 1;
            self.cursor_idx = 0;
            self.scroll_offset = 0;
            self.custom_input.clear();
            self.editing_custom = false;
        }
    }

    pub fn tab_right(&mut self) {
        let confirm_idx = self.questions.len();
        if self.active_tab < confirm_idx {
            self.active_tab += 1;
            self.cursor_idx = 0;
            self.scroll_offset = 0;
            self.custom_input.clear();
            self.editing_custom = false;
        }
    }

    fn select_current(&mut self) -> bool {
        if self.active_tab >= self.questions.len() {
            return false;
        }
        let q = &self.questions[self.active_tab];
        if self.cursor_idx >= q.options.len() {
            return false;
        }
        if q.is_custom_answer_idx(self.cursor_idx) {
            if self.editing_custom {
                if !self.custom_input.trim().is_empty() {
                    self.answers[self.active_tab] = vec![self.custom_input.clone()];
                    return true;
                }
                return false;
            }
            self.editing_custom = true;
            self.custom_input.clear();
            return false;
        }
        let opt_label = q.options[self.cursor_idx].label.clone();
        match &q.question_type {
            QuestionType::Select => {
                self.answers[self.active_tab] = vec![opt_label];
                true
            }
            QuestionType::MultiSelect => {
                let sel = &mut self.answers[self.active_tab];
                if let Some(pos) = sel.iter().position(|s| s == &opt_label) {
                    sel.remove(pos);
                } else {
                    sel.push(opt_label);
                }
                false
            }
            QuestionType::Text { .. } => false,
        }
    }

    fn handle_number(&mut self, num: usize) -> Option<QuestionResponse> {
        let idx = num.saturating_sub(1);
        if self.active_tab >= self.questions.len() {
            return None;
        }
        let q = &self.questions[self.active_tab];
        if idx >= q.option_count() {
            return None;
        }
        self.cursor_idx = idx;
        let should_advance = self.select_current();
        if should_advance {
            if self.is_last_question() {
                return Some(self.submit());
            }
            self.advance_tab();
        }
        None
    }

    fn handle_enter(&mut self) -> Option<QuestionResponse> {
        if self.active_tab >= self.questions.len() {
            return None;
        }
        let q = &self.questions[self.active_tab];
        let cursor = self.cursor_idx;
        match &q.question_type {
            QuestionType::Text { .. } => {
                if q.options.is_empty() {
                    self.answers[self.active_tab] = vec!["[text_input]".to_string()];
                } else if cursor < q.options.len() {
                    if q.is_custom_answer_idx(cursor) {
                        self.editing_custom = true;
                        self.custom_input.clear();
                    } else {
                        let _ = self.select_current();
                        if self.is_last_question() {
                            return Some(self.submit());
                        }
                        self.advance_tab();
                    }
                }
            }
            QuestionType::Select => {
                if cursor < q.options.len() {
                    if q.is_custom_answer_idx(cursor) {
                        self.editing_custom = true;
                        self.custom_input.clear();
                    } else {
                        let _ = self.select_current();
                        if self.is_last_question() {
                            return Some(self.submit());
                        }
                        self.advance_tab();
                    }
                }
            }
            QuestionType::MultiSelect => {
                if cursor < q.options.len() {
                    if q.is_custom_answer_idx(cursor) {
                        self.editing_custom = true;
                        self.custom_input.clear();
                    } else {
                        self.select_current();
                        if self.is_last_question() {
                            self.active_tab = self.questions.len();
                            self.cursor_idx = 0;
                        } else {
                            self.advance_tab();
                        }
                    }
                }
            }
        }
        None
    }

    fn handle_space(&mut self) -> Option<QuestionResponse> {
        if self.active_tab >= self.questions.len() {
            return None;
        }
        let q = &self.questions[self.active_tab];
        let cursor = self.cursor_idx;
        match &q.question_type {
            QuestionType::MultiSelect => {
                if cursor < q.options.len() && !q.is_custom_answer_idx(cursor) {
                    self.select_current();
                }
            }
            QuestionType::Select => {
                if cursor < q.options.len() {
                    if q.is_custom_answer_idx(cursor) {
                        self.editing_custom = true;
                        self.custom_input.clear();
                    } else {
                        let _ = self.select_current();
                        if self.is_last_question() {
                            return Some(self.submit());
                        }
                        self.advance_tab();
                    }
                }
            }
            QuestionType::Text { .. } => {}
        }
        None
    }

    fn handle_confirm_tab_key(
        &mut self,
        code: ratatui::crossterm::event::KeyCode,
    ) -> Option<QuestionResponse> {
        use ratatui::crossterm::event::KeyCode;
        match code {
            KeyCode::Enter => Some(self.submit()),
            KeyCode::Esc => self.handle_esc(),
            KeyCode::Left => {
                self.tab_left();
                None
            }
            _ => None,
        }
    }

    fn handle_custom_editing_key(
        &mut self,
        code: ratatui::crossterm::event::KeyCode,
    ) -> Option<QuestionResponse> {
        use ratatui::crossterm::event::KeyCode;
        match code {
            KeyCode::Enter => {
                if !self.custom_input.trim().is_empty() {
                    self.answers[self.active_tab] = vec![self.custom_input.clone()];
                    self.editing_custom = false;
                    if self.is_last_question() {
                        return Some(self.submit());
                    }
                    self.advance_tab();
                }
                None
            }
            KeyCode::Esc => {
                self.editing_custom = false;
                self.custom_input.clear();
                None
            }
            KeyCode::Char(c) => {
                self.custom_input.push(c);
                None
            }
            KeyCode::Backspace => {
                self.custom_input.pop();
                None
            }
            KeyCode::Left => {
                self.tab_left();
                None
            }
            KeyCode::Right => {
                if self.active_tab < self.questions.len() {
                    self.active_tab += 1;
                    self.cursor_idx = 0;
                    self.editing_custom = false;
                    self.custom_input.clear();
                }
                None
            }
            _ => None,
        }
    }

    fn handle_esc(&mut self) -> Option<QuestionResponse> {
        let cancelled = QuestionResponse {
            answer: String::new(),
            cancelled: true,
        };
        if let Some(tx) = self.answer_tx.take() {
            let _ = tx.send(String::new());
        }
        self.close();
        Some(cancelled)
    }

    pub fn handle_key(
        &mut self,
        code: ratatui::crossterm::event::KeyCode,
    ) -> Option<QuestionResponse> {
        use ratatui::crossterm::event::KeyCode;
        if self.active_tab >= self.questions.len() {
            return self.handle_confirm_tab_key(code);
        }
        if self.editing_custom {
            return self.handle_custom_editing_key(code);
        }
        match code {
            KeyCode::Up => self.cursor_up(),
            KeyCode::Down => self.cursor_down(),
            KeyCode::Left => self.tab_left(),
            KeyCode::Right => self.tab_right(),
            KeyCode::Enter => {
                if let Some(r) = self.handle_enter() {
                    return Some(r);
                }
            }
            KeyCode::Char(' ') => {
                if let Some(r) = self.handle_space() {
                    return Some(r);
                }
            }
            KeyCode::Char(c) if ('1'..='9').contains(&c) => {
                if let Some(num) = c.to_digit(10) {
                    if let Some(r) = self.handle_number(num as usize) {
                        return Some(r);
                    }
                }
            }
            KeyCode::Esc => return self.handle_esc(),
            _ => {}
        }
        None
    }
}

fn content_height(state: &QuestionPromptState) -> u16 {
    let base: u16 = 7;
    let opts_h = if state.active_tab < state.questions.len() {
        let q = &state.questions[state.active_tab];
        let visible = q.options.len().min(10) as u16;
        if state.editing_custom {
            visible + 2
        } else {
            visible + 1
        }
    } else {
        0
    };
    let confirm_h = if state.active_tab >= state.questions.len() && !state.questions.is_empty() {
        state.questions.len() as u16 * 2 + 2
    } else {
        0
    };
    (base + opts_h.max(confirm_h)).max(12).min(30)
}

fn popup_area(frame_area: Rect, state: &QuestionPromptState) -> Rect {
    popup_utils::popup_dimensions(frame_area, 0.5, 40, 70, 0.6, content_height(state))
}

pub fn render_question_prompt(
    frame: &mut Frame,
    area: Rect,
    state: &mut QuestionPromptState,
    theme: &Theme,
) {
    if !state.open || state.questions.is_empty() {
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
    let accent = theme.accent;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .border_type(BorderType::Rounded)
        .title(format!(" {} asks ", state.agent))
        .title_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(theme.background_panel));
    frame.render_widget(block.clone(), popup);
    let inner = block.inner(popup);
    if inner.height < 5 {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(inner.height.saturating_sub(2)),
            Constraint::Length(1),
        ])
        .split(inner);
    render_tab_bar(frame, chunks[0], state, theme);
    render_content(frame, chunks[1], state, theme);
    let hints = if state.active_tab >= state.questions.len() {
        vec![("enter", "submit"), ("esc", "dismiss")]
    } else if state.editing_custom {
        vec![("enter", "confirm"), ("esc", "cancel edit"), ("←→", "tab")]
    } else {
        vec![
            ("←→", "tab"),
            ("↑↓", "select"),
            ("enter", "confirm"),
            ("esc", "dismiss"),
        ]
    };
    popup_utils::render_hint_bar(frame, chunks[2], &hints, *theme);
}

fn render_tab_bar(frame: &mut Frame, area: Rect, state: &QuestionPromptState, theme: &Theme) {
    if area.width < 4 || state.questions.is_empty() {
        return;
    }
    let total = state.questions.len() + 1;
    let tw = area.width as usize / total;
    let mut spans = Vec::new();
    for i in 0..total {
        let is_active = state.active_tab == i;
        let is_answered = i < state.questions.len() && !state.answers[i].is_empty();
        let label = if i < state.questions.len() {
            &state.questions[i].header
        } else {
            "Confirm"
        };
        let style = if is_active {
            Style::default()
                .fg(theme.text_inverse)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if is_answered {
            Style::default().fg(theme.text)
        } else {
            Style::default().fg(theme.text_muted)
        };
        let dl: String = label.chars().take(tw.saturating_sub(2)).collect();
        spans.push(Span::styled(format!(" {dl} "), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.background_element)),
        area,
    );
}

fn render_content(frame: &mut Frame, area: Rect, state: &QuestionPromptState, theme: &Theme) {
    if state.active_tab >= state.questions.len() {
        render_confirm_tab(frame, area, state, theme);
        return;
    }
    let q = &state.questions[state.active_tab];
    let is_text = matches!(q.question_type, QuestionType::Text { .. });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(area.height.saturating_sub(2)),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                &q.question,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
        ])),
        chunks[0],
    );
    if is_text && q.options.is_empty() {
        render_custom_textarea(frame, chunks[1], state, theme, "");
    } else {
        let is_multi = matches!(q.question_type, QuestionType::MultiSelect);
        render_options(frame, chunks[1], state, q, theme, is_multi);
    }
}

fn render_options(
    frame: &mut Frame,
    area: Rect,
    state: &QuestionPromptState,
    q: &QuestionDef,
    theme: &Theme,
    is_multi: bool,
) {
    let max_opts = area.height.min(12) as usize;
    let visible_opts: Vec<_> = q
        .options
        .iter()
        .skip(state.scroll_offset)
        .take(max_opts)
        .collect();
    let mut row_pos: u16 = 0;
    for (i, opt) in visible_opts.iter().enumerate() {
        let global_idx = state.scroll_offset + i;
        if row_pos >= area.height {
            break;
        }
        let is_cursor = global_idx == state.cursor_idx;
        let is_custom = opt.label == CUSTOM_ANSWER_LABEL;
        let prefix = if is_custom {
            if state.editing_custom {
                Span::styled(
                    "\u{270e} ",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("\u{270f} ", Style::default().fg(theme.text_muted))
            }
        } else if is_multi {
            let sel = state.answers[state.active_tab].contains(&opt.label);
            if sel {
                Span::styled(
                    "[\u{2713}] ",
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("[ ] ", Style::default().fg(theme.text_muted))
            }
        } else {
            let sel = state.answers[state.active_tab].contains(&opt.label);
            if sel {
                Span::styled(
                    "\u{2713} ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw("  ")
            }
        };
        let num = if global_idx < 9 {
            format!("{}.", global_idx + 1)
        } else {
            "  ".to_string()
        };
        let opt_style = if is_cursor && !state.editing_custom {
            Style::default()
                .fg(theme.secondary)
                .bg(theme.background_element)
        } else {
            Style::default().fg(theme.text)
        };
        let line = Line::from(vec![
            Span::styled(format!(" {num} "), Style::default().fg(theme.text_muted)),
            prefix,
            Span::styled(&opt.label, opt_style),
        ]);
        let row = Rect {
            x: area.x,
            y: area.y + row_pos,
            width: area.width,
            height: 1,
        };
        if is_cursor && !state.editing_custom {
            frame.render_widget(
                Paragraph::new(line).style(Style::default().bg(theme.background_element)),
                row,
            );
        } else {
            frame.render_widget(Paragraph::new(line), row);
        }
        row_pos += 1;

        if !opt.description.is_empty() && row_pos < area.height {
            let desc_row = Rect {
                x: area.x + 3,
                y: area.y + row_pos,
                width: area.width.saturating_sub(3),
                height: 1,
            };
            let desc_style = if is_cursor && !state.editing_custom {
                Style::default()
                    .fg(theme.text_muted)
                    .bg(theme.background_element)
            } else {
                Style::default().fg(theme.text_muted)
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(&opt.description, desc_style))),
                desc_row,
            );
            row_pos += 1;
        }
    }
    if state.editing_custom {
        let ir = row_pos;
        if ir < area.height {
            render_custom_textarea(
                frame,
                Rect {
                    x: area.x,
                    y: area.y + ir,
                    width: area.width,
                    height: area.height.saturating_sub(ir),
                },
                state,
                theme,
                "Your answer",
            );
        }
    }
}

fn render_custom_textarea(
    frame: &mut Frame,
    area: Rect,
    state: &QuestionPromptState,
    theme: &Theme,
    placeholder: &str,
) {
    if area.height < 2 || area.width < 10 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" {placeholder} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme.background_element));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let display = if state.custom_input.is_empty() && placeholder != "Your answer" {
        Line::from(vec![Span::styled(
            placeholder,
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )])
    } else if state.custom_input.is_empty() {
        Line::from(vec![
            Span::styled(
                "\u{2588}",
                Style::default().fg(theme.primary).bg(theme.primary),
            ),
            Span::raw(" "),
        ])
    } else {
        let ci = state.custom_input.chars().count();
        let bi = state
            .custom_input
            .char_indices()
            .nth(ci)
            .map_or(state.custom_input.len(), |(b, _)| b);
        let before = &state.custom_input[..bi];
        Line::from(vec![
            Span::styled(before.to_string(), Style::default().fg(theme.text)),
            Span::styled(
                "\u{2588}",
                Style::default().fg(theme.primary).bg(theme.primary),
            ),
        ])
    };
    frame.render_widget(
        Paragraph::new(display)
            .style(Style::default().fg(theme.text))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        inner,
    );
}

fn render_confirm_tab(frame: &mut Frame, area: Rect, state: &QuestionPromptState, theme: &Theme) {
    if state.questions.is_empty() {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "  Review your answers",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )])),
        area,
    );
    if area.height < 3 {
        return;
    }
    let aa = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    for (i, (q, answers)) in state.questions.iter().zip(state.answers.iter()).enumerate() {
        if i as u16 >= aa.height.saturating_sub(1) {
            break;
        }
        let astr = if answers.is_empty() {
            Span::styled(
                "[not answered]",
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )
        } else {
            Span::styled(answers.join(", "), Style::default().fg(theme.text))
        };
        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{}: ", q.header),
                Style::default()
                    .fg(theme.secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            astr,
        ]);
        let row = Rect {
            x: aa.x,
            y: aa.y + i as u16,
            width: aa.width,
            height: 1,
        };
        let bb =
            popup_utils::left_border_block(*theme, theme.accent, "", Some(theme.background_panel));
        frame.render_widget(bb.clone(), row);
        frame.render_widget(Paragraph::new(line), bb.inner(row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> QuestionPromptState {
        QuestionPromptState::new()
    }
    fn make_single_select() -> Vec<QuestionDef> {
        vec![QuestionDef::select(
            "Pick one",
            "Choose:",
            vec![
                QuestionOption::new("Alpha"),
                QuestionOption::new("Beta"),
                QuestionOption::new("Gamma"),
            ],
        )]
    }
    fn make_multi_select() -> Vec<QuestionDef> {
        vec![QuestionDef::multi_select(
            "Pick many",
            "Select all:",
            vec![
                QuestionOption::new("Option A"),
                QuestionOption::new("Option B"),
                QuestionOption::new("Option C"),
            ],
        )]
    }
    fn make_multi() -> Vec<QuestionDef> {
        vec![
            QuestionDef::select(
                "Q1",
                "First?",
                vec![
                    QuestionOption::new("Choice 1"),
                    QuestionOption::new("Choice 2"),
                ],
            ),
            QuestionDef::multi_select(
                "Q2",
                "Second?",
                vec![
                    QuestionOption::new("Multi A"),
                    QuestionOption::new("Multi B"),
                ],
            ),
            QuestionDef::text("Q3", "Third?", "type here"),
        ]
    }

    #[test]
    fn tab_left_moves_to_previous() {
        let mut s = make_state();
        s.open(make_multi(), "agent".into(), String::new(), None);
        assert_eq!(s.active_tab, 0);
        s.tab_left();
        assert_eq!(s.active_tab, 0);
        s.active_tab = 2;
        s.tab_left();
        assert_eq!(s.active_tab, 1);
    }

    #[test]
    fn tab_right_moves_to_next() {
        let mut s = make_state();
        s.open(make_multi(), "agent".into(), String::new(), None);
        s.tab_right();
        assert_eq!(s.active_tab, 1);
        s.tab_right();
        assert_eq!(s.active_tab, 2);
        s.tab_right();
        assert_eq!(s.active_tab, 3);
        s.tab_right();
        assert_eq!(s.active_tab, 3);
    }

    #[test]
    fn tab_resets_cursor() {
        let mut s = make_state();
        s.open(make_multi(), "agent".into(), String::new(), None);
        s.cursor_idx = 1;
        s.tab_right();
        assert_eq!(s.cursor_idx, 0);
    }

    #[test]
    fn single_select_auto_advances() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        s.cursor_idx = 0;
        let adv = s.select_current();
        assert!(adv);
        assert_eq!(s.answers[0], vec!["Alpha"]);
    }

    #[test]
    fn single_select_last_submits() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        s.cursor_idx = 1;
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Char('2'));
        assert!(r.is_some());
        assert_eq!(r.unwrap().answer, "Beta");
    }

    #[test]
    fn single_select_non_last_advances() {
        let mut s = make_state();
        s.open(make_multi(), "agent".into(), String::new(), None);
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Char('1'));
        assert!(r.is_none());
        assert_eq!(s.active_tab, 1);
        assert_eq!(s.answers[0], vec!["Choice 1"]);
    }

    #[test]
    fn multi_select_space_toggles() {
        let mut s = make_state();
        s.open(make_multi_select(), "agent".into(), String::new(), None);
        s.cursor_idx = 0;
        s.handle_key(ratatui::crossterm::event::KeyCode::Char(' '));
        assert_eq!(s.answers[0], vec!["Option A"]);
        s.handle_key(ratatui::crossterm::event::KeyCode::Char(' '));
        assert!(s.answers[0].is_empty());
        s.cursor_down();
        s.handle_key(ratatui::crossterm::event::KeyCode::Char(' '));
        assert_eq!(s.answers[0], vec!["Option B"]);
    }

    #[test]
    fn multi_select_enter_confirms() {
        let mut s = make_state();
        s.open(make_multi_select(), "agent".into(), String::new(), None);
        s.cursor_idx = 0;
        s.handle_key(ratatui::crossterm::event::KeyCode::Enter);
        assert_eq!(s.active_tab, 1);
    }

    #[test]
    fn multi_select_multiple() {
        let mut s = make_state();
        s.open(make_multi_select(), "agent".into(), String::new(), None);
        s.cursor_idx = 0;
        s.select_current();
        s.cursor_idx = 2;
        s.select_current();
        assert_eq!(s.answers[0], vec!["Option A", "Option C"]);
    }

    #[test]
    fn numbered_shortcut_selects() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Char('3'));
        assert!(r.is_some());
        assert_eq!(r.unwrap().answer, "Gamma");
    }

    #[test]
    fn numbered_shortcut_ooo_ignored() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Char('5'));
        assert!(r.is_none());
        assert!(s.answers[0].is_empty());
    }

    #[test]
    fn numbered_one_selects_first() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Char('1'));
        assert!(r.is_some());
        assert_eq!(r.unwrap().answer, "Alpha");
    }

    #[test]
    fn custom_answer_activates_editing() {
        let mut s = make_state();
        s.open(
            vec![QuestionDef::select(
                "C",
                "Pick:",
                vec![
                    QuestionOption::new("A"),
                    QuestionOption::with_description(CUSTOM_ANSWER_LABEL, "x"),
                ],
            )],
            "agent".into(),
            String::new(),
            None,
        );
        s.cursor_idx = 1;
        s.handle_key(ratatui::crossterm::event::KeyCode::Enter);
        assert!(s.editing_custom);
    }

    #[test]
    fn custom_input_type_and_confirm() {
        let mut s = make_state();
        s.open(
            vec![QuestionDef::select(
                "C",
                "Type:",
                vec![QuestionOption::with_description(CUSTOM_ANSWER_LABEL, "x")],
            )],
            "agent".into(),
            String::new(),
            None,
        );
        s.cursor_idx = 0;
        s.handle_key(ratatui::crossterm::event::KeyCode::Enter);
        for c in ['h', 'e', 'l', 'l', 'o'] {
            s.handle_key(ratatui::crossterm::event::KeyCode::Char(c));
        }
        assert_eq!(s.custom_input, "hello");
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Enter);
        assert!(r.is_some());
        assert_eq!(r.unwrap().answer, "hello");
    }

    #[test]
    fn custom_input_backspace() {
        let mut s = make_state();
        s.open(
            vec![QuestionDef::select(
                "C",
                "Type:",
                vec![QuestionOption::with_description(CUSTOM_ANSWER_LABEL, "x")],
            )],
            "agent".into(),
            String::new(),
            None,
        );
        s.cursor_idx = 0;
        s.handle_key(ratatui::crossterm::event::KeyCode::Enter);
        s.handle_key(ratatui::crossterm::event::KeyCode::Char('a'));
        s.handle_key(ratatui::crossterm::event::KeyCode::Char('b'));
        s.handle_key(ratatui::crossterm::event::KeyCode::Char('c'));
        s.handle_key(ratatui::crossterm::event::KeyCode::Backspace);
        assert_eq!(s.custom_input, "ab");
    }

    #[test]
    fn custom_input_esc_cancels() {
        let mut s = make_state();
        s.open(
            vec![QuestionDef::select(
                "C",
                "Type:",
                vec![QuestionOption::with_description(CUSTOM_ANSWER_LABEL, "x")],
            )],
            "agent".into(),
            String::new(),
            None,
        );
        s.cursor_idx = 0;
        s.handle_key(ratatui::crossterm::event::KeyCode::Enter);
        s.handle_key(ratatui::crossterm::event::KeyCode::Char('x'));
        assert!(s.editing_custom);
        s.handle_key(ratatui::crossterm::event::KeyCode::Esc);
        assert!(!s.editing_custom);
        assert!(s.custom_input.is_empty());
        assert!(s.open);
    }

    #[test]
    fn confirm_esc_dismisses() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        s.active_tab = 1;
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Esc);
        assert!(r.is_some());
        assert!(r.unwrap().cancelled);
        assert!(!s.open);
    }

    #[test]
    fn submit_single_answer() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        s.answers[0] = vec!["Beta".to_string()];
        let r = s.submit();
        assert_eq!(r.answer, "Beta");
        assert!(!r.cancelled);
    }

    #[test]
    fn submit_multiple_answers() {
        let mut s = make_state();
        s.open(make_multi(), "agent".into(), String::new(), None);
        s.answers[0] = vec!["Choice 1".to_string()];
        s.answers[1] = vec!["Multi A".to_string(), "Multi B".to_string()];
        s.answers[2] = vec!["typed".to_string()];
        let r = s.submit();
        assert!(r.answer.contains("Choice 1"));
        assert!(r.answer.contains("Multi A"));
        assert!(r.answer.contains("typed"));
    }

    #[test]
    fn submit_cancelled_empty() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        assert!(s.submit().cancelled);
    }

    #[test]
    fn esc_dismisses() {
        let mut s = make_state();
        s.open(make_single_select(), "agent".into(), String::new(), None);
        let r = s.handle_key(ratatui::crossterm::event::KeyCode::Esc);
        assert!(r.is_some());
        assert!(r.unwrap().cancelled);
        assert!(!s.open);
    }

    #[test]
    fn open_initializes() {
        let mut s = make_state();
        s.open(make_multi(), "ta".into(), "ctx".into(), None);
        assert!(s.open);
        assert_eq!(s.questions.len(), 3);
        assert_eq!(s.answers.len(), 3);
    }

    #[test]
    fn open_single_compat() {
        let mut s = make_state();
        s.open_single(
            "Q?".into(),
            "a".into(),
            "c".into(),
            QuestionType::Text {
                placeholder: "p".into(),
            },
            None,
        );
        assert!(s.open);
        assert_eq!(s.questions.len(), 1);
    }

    #[test]
    fn close_resets() {
        let mut s = make_state();
        s.open(make_multi(), "a".into(), String::new(), None);
        s.close();
        assert!(!s.open);
        assert!(s.questions.is_empty());
    }

    #[test]
    fn qdef_text() {
        let q = QuestionDef::text("H", "Q", "p");
        assert_eq!(q.header, "H");
        assert!(q.options.is_empty());
    }

    #[test]
    fn qdef_select() {
        let q = QuestionDef::select("H", "Q", vec![QuestionOption::new("A")]);
        assert!(matches!(q.question_type, QuestionType::Select));
    }

    #[test]
    fn qdef_multi() {
        let q = QuestionDef::multi_select("H", "Q", vec![QuestionOption::new("A")]);
        assert!(matches!(q.question_type, QuestionType::MultiSelect));
    }

    #[test]
    fn option_with_desc() {
        let o = QuestionOption::with_description("L", "D");
        assert_eq!(o.label, "L");
        assert_eq!(o.description, "D");
    }

    #[test]
    fn is_custom_idx() {
        let q = QuestionDef::select(
            "T",
            "Q",
            vec![
                QuestionOption::new("N"),
                QuestionOption::with_description(CUSTOM_ANSWER_LABEL, "c"),
            ],
        );
        assert!(!q.is_custom_answer_idx(0));
        assert!(q.is_custom_answer_idx(1));
    }
}
