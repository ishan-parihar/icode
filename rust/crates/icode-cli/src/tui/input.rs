use crate::tui::theme::Theme;
use ratatui::prelude::Widget;
use ratatui::widgets::{StatefulWidget, Wrap};
use ratatui::{buffer::Buffer, layout::Rect, style::Style, text::Span, widgets::Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const SLASH_COMMANDS: &[&str] = &[
    "/help",
    "/status",
    "/cost",
    "/compact",
    "/clear",
    "/model",
    "/permissions",
    "/config",
    "/memory",
    "/diff",
    "/export",
    "/session",
    "/version",
    "/undo",
    "/redo",
];

#[derive(Debug, Default)]
pub struct InputState {
    pub value: String,
    pub cursor: usize,
    pub completions: Vec<String>,
    pub show_completions: bool,
    pub completion_idx: usize,
    pub prompt: String,
    pub placeholder: String,
    pub show_slash_autocomplete: bool,
    pub slash_completions: Vec<String>,
    pub slash_completion_idx: usize,
    pub history: Vec<String>,
    pub history_idx: usize,
    pub history_temp: Option<String>,
}

impl InputState {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            placeholder: "Ask icode to do anything...".into(),
            ..Default::default()
        }
    }

    fn char_to_byte(&self, char_offset: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_offset)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.value.len())
    }

    pub fn insert_char(&mut self, c: char) {
        let byte_idx = self.char_to_byte(self.cursor);
        self.value.insert(byte_idx, c);
        self.cursor += 1;
        self.show_completions = false;
        self.update_slash_autocomplete();
    }

    pub fn insert_str(&mut self, s: &str) {
        let byte_idx = self.char_to_byte(self.cursor);
        self.value.insert_str(byte_idx, s);
        self.cursor += s.chars().count();
        self.show_completions = false;
        self.update_slash_autocomplete();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let byte_idx = self.char_to_byte(self.cursor);
        let prev_byte_idx = self.value[..byte_idx]
            .char_indices()
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.value.drain(prev_byte_idx..byte_idx);
        self.cursor -= 1;
        self.update_slash_autocomplete();
    }

    pub fn delete_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.value.chars().collect();
        let mut i = self.cursor - 1;
        while i > 0 && chars[i].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i].is_whitespace() {
            i -= 1;
        }
        let delete_start = if i == 0 && !chars[0].is_whitespace() {
            0
        } else if chars[i].is_whitespace() {
            i + 1
        } else {
            i
        };
        let byte_start = self.char_to_byte(delete_start);
        let byte_end = self.char_to_byte(self.cursor);
        self.value.drain(byte_start..byte_end);
        self.cursor = delete_start;
        self.update_slash_autocomplete();
    }

    pub fn delete(&mut self) {
        let byte_idx = self.char_to_byte(self.cursor);
        if byte_idx < self.value.len() {
            let mut char_iter = self.value[byte_idx..].char_indices();
            if let Some((start, ch)) = char_iter.next() {
                let end = start + ch.len_utf8();
                self.value.drain(byte_idx + start..byte_idx + end);
            }
        }
        self.update_slash_autocomplete();
    }

    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.value.chars().collect();
        let mut i = self.cursor.saturating_sub(1);
        while i > 0 && chars[i].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i].is_whitespace() {
            i -= 1;
        }
        if chars[i].is_whitespace() {
            i += 1;
        }
        self.cursor = i;
    }

    pub fn move_word_right(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let len = chars.len();
        let mut i = self.cursor;
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        while i < len && !chars[i].is_whitespace() {
            i += 1;
        }
        self.cursor = i;
    }

    pub fn delete_to_start(&mut self) {
        let byte_idx = self.char_to_byte(self.cursor);
        self.value.drain(..byte_idx);
        self.cursor = 0;
        self.update_slash_autocomplete();
    }

    pub fn delete_to_end(&mut self) {
        let byte_idx = self.char_to_byte(self.cursor);
        self.value.drain(byte_idx..);
        self.update_slash_autocomplete();
    }

    pub fn delete_word_right(&mut self) {
        let byte_start = self.char_to_byte(self.cursor);
        if byte_start >= self.value.len() {
            return;
        }
        let chars: Vec<char> = self.value.chars().collect();
        let mut i = self.cursor;
        let len = chars.len();
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        while i < len && !chars[i].is_whitespace() {
            i += 1;
        }
        let byte_end = self.char_to_byte(i);
        self.value.drain(byte_start..byte_end);
        self.update_slash_autocomplete();
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let char_count = self.value.chars().count();
        if self.cursor < char_count {
            self.cursor += 1;
        }
    }

    /// Move cursor to the same column on the line above.
    /// Returns false if already on the first line.
    pub fn move_up(&mut self, max_width: usize) -> bool {
        if max_width == 0 {
            return false;
        }
        let (row, col) = self.cursor_position(max_width);
        if row == 0 {
            return false;
        }
        let target_row = row - 1;
        let target_col = col;
        self.cursor = self.char_offset_at_row_col(target_row, target_col, max_width);
        true
    }

    /// Move cursor to the same column on the line below.
    /// Returns false if already on the last line.
    pub fn move_down(&mut self, max_width: usize) -> bool {
        if max_width == 0 {
            return false;
        }
        let (row, col) = self.cursor_position(max_width);
        let total_rows = self.total_rows(max_width);
        if row + 1 >= total_rows {
            return false;
        }
        let target_row = row + 1;
        let target_col = col;
        self.cursor = self.char_offset_at_row_col(target_row, target_col, max_width);
        true
    }

    pub fn cursor_position(&self, max_width: usize) -> (usize, usize) {
        let prefix_w = self.prompt.width();
        let first_line_avail = max_width.saturating_sub(prefix_w);
        let subsequent_line_avail = max_width;

        let byte_idx = self.char_to_byte(self.cursor);
        let prefix = &self.value[..byte_idx];

        let mut row = 0usize;
        let mut col = 0usize;
        let mut avail = first_line_avail;

        for ch in prefix.chars() {
            if ch == '\n' {
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
                continue;
            }
            let w = ch.width().unwrap_or(1);
            if avail > 0 && col + w > avail {
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
            }
            if avail > 0 {
                col += w;
                avail = avail.saturating_sub(w);
            }
        }
        (row, col)
    }

    pub fn total_rows(&self, max_width: usize) -> usize {
        let prefix_w = self.prompt.width();
        let first_line_avail = max_width.saturating_sub(prefix_w);
        let subsequent_line_avail = max_width;

        let mut row = 0usize;
        let mut col = 0usize;
        let mut avail = first_line_avail;

        for ch in self.value.chars() {
            if ch == '\n' {
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
                continue;
            }
            let w = ch.width().unwrap_or(1);
            if avail > 0 && col + w > avail {
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
            }
            if avail > 0 {
                col += w;
                avail = avail.saturating_sub(w);
            }
        }
        row + 1
    }

    fn char_offset_at_row_col(
        &self,
        target_row: usize,
        target_col: usize,
        max_width: usize,
    ) -> usize {
        let prefix_w = self.prompt.width();
        let first_line_avail = max_width.saturating_sub(prefix_w);
        let subsequent_line_avail = max_width;

        let mut row = 0usize;
        let mut col = 0usize;
        let mut avail = first_line_avail;
        let mut char_offset = 0usize;

        for ch in self.value.chars() {
            if row == target_row {
                if col >= target_col {
                    return char_offset;
                }
            }
            if ch == '\n' {
                if row == target_row {
                    return char_offset;
                }
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
                char_offset += 1;
                continue;
            }
            let w = ch.width().unwrap_or(1);
            if avail > 0 && col + w > avail {
                if row == target_row {
                    return char_offset;
                }
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
            }
            if avail > 0 {
                col += w;
                avail = avail.saturating_sub(w);
            }
            char_offset += 1;
        }
        char_offset
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.show_completions = false;
        self.hide_slash_autocomplete();
    }

    pub fn push_history(&mut self) {
        if !self.value.trim().is_empty() {
            self.history.push(self.value.clone());
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
        self.history_idx = self.history.len();
        self.history_temp = None;
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.history_temp.is_none() {
            self.history_temp = Some(self.value.clone());
        }
        if self.history_idx > 0 {
            self.history_idx -= 1;
            self.value = self.history[self.history_idx].clone();
            self.cursor = self.value.chars().count();
        }
    }

    pub fn history_down(&mut self) {
        if self.history_temp.is_none() && self.history_idx >= self.history.len() {
            return;
        }
        if self.history_idx + 1 >= self.history.len() {
            self.history_idx = self.history.len();
            self.value = self.history_temp.take().unwrap_or_default();
            self.cursor = self.value.chars().count();
        } else {
            self.history_idx += 1;
            self.value = self.history[self.history_idx].clone();
            self.cursor = self.value.chars().count();
        }
    }

    pub fn set_completions(&mut self, completions: Vec<String>) {
        self.completions = completions;
        self.completion_idx = 0;
    }

    pub fn cycle_completion_forward(&mut self) {
        if self.completions.is_empty() {
            return;
        }
        self.show_completions = true;
        self.completion_idx = (self.completion_idx + 1) % self.completions.len();
        self.value
            .clone_from(&self.completions[self.completion_idx]);
        self.cursor = self.value.chars().count();
    }

    pub fn cycle_completion_backward(&mut self) {
        if self.completions.is_empty() {
            return;
        }
        self.show_completions = true;
        if self.completion_idx == 0 {
            self.completion_idx = self.completions.len() - 1;
        } else {
            self.completion_idx -= 1;
        }
        self.value
            .clone_from(&self.completions[self.completion_idx]);
        self.cursor = self.value.chars().count();
    }

    fn update_slash_autocomplete(&mut self) {
        if self.value.starts_with('/') && self.cursor > 0 {
            let prefix = self.value.as_str();
            self.slash_completions = SLASH_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|s| s.to_string())
                .collect();
            self.show_slash_autocomplete = !self.slash_completions.is_empty();
            if self.show_slash_autocomplete {
                self.slash_completion_idx = 0;
            }
        } else {
            self.hide_slash_autocomplete();
        }
    }

    fn hide_slash_autocomplete(&mut self) {
        self.show_slash_autocomplete = false;
        self.slash_completions.clear();
        self.slash_completion_idx = 0;
    }

    pub fn slash_autocomplete_up(&mut self) {
        if self.slash_completions.is_empty() {
            return;
        }
        if self.slash_completion_idx == 0 {
            self.slash_completion_idx = self.slash_completions.len() - 1;
        } else {
            self.slash_completion_idx -= 1;
        }
    }

    pub fn slash_autocomplete_down(&mut self) {
        if self.slash_completions.is_empty() {
            return;
        }
        self.slash_completion_idx = (self.slash_completion_idx + 1) % self.slash_completions.len();
    }

    pub fn slash_autocomplete_select(&mut self) -> bool {
        if self.slash_completions.is_empty() {
            return false;
        }
        let selected = self.slash_completions[self.slash_completion_idx].clone();
        self.value = selected.clone();
        self.cursor = selected.chars().count();
        self.hide_slash_autocomplete();
        true
    }

    pub fn selected_slash_completion(&self) -> Option<&str> {
        self.slash_completions
            .get(self.slash_completion_idx)
            .map(|s| s.as_str())
    }

    pub fn submit(&mut self) -> String {
        let value = std::mem::take(&mut self.value);
        self.cursor = 0;
        self.show_completions = false;
        self.hide_slash_autocomplete();
        value
    }

    pub fn visible_text(&self) -> String {
        if self.value.is_empty() && !self.show_completions {
            return self.placeholder.clone();
        }
        self.value.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn line_count(&self, max_width: usize) -> usize {
        if max_width == 0 {
            return 1;
        }
        let text = if self.value.is_empty() {
            &self.placeholder
        } else {
            &self.value
        };
        let prefix_w = self.prompt.width();
        let avail = max_width.saturating_sub(prefix_w);
        let mut count = 0;
        for line in text.lines() {
            let line_w = line.width();
            if avail == 0 {
                count += 1;
                continue;
            }
            let wraps = (line_w + avail - 1) / avail;
            count += wraps.max(1);
        }
        count.max(1)
    }
}

pub struct InputWidget {
    theme: Theme,
}

impl InputWidget {
    pub fn new(theme: Theme) -> Self {
        Self { theme }
    }
}

impl Default for InputWidget {
    fn default() -> Self {
        Self::new(Theme::default())
    }
}

impl StatefulWidget for InputWidget {
    type State = InputState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let text_color = self.theme.text;
        let muted_color = self.theme.text_muted;

        let bg_fill = Paragraph::new("").style(Style::default().bg(self.theme.background_element));
        bg_fill.render(area, buf);

        let prompt = Span::styled(&state.prompt, Style::default().fg(self.theme.border_active));

        let lines: Vec<ratatui::text::Line<'_>> =
            if state.value.is_empty() && !state.show_completions {
                let placeholder = Span::styled(
                    &state.placeholder,
                    Style::default()
                        .fg(muted_color)
                        .add_modifier(ratatui::style::Modifier::ITALIC),
                );
                vec![ratatui::text::Line::from(vec![prompt.clone(), placeholder])]
            } else {
                let mut result = Vec::new();
                for (i, segment) in state.value.split('\n').enumerate() {
                    if i == 0 {
                        result.push(ratatui::text::Line::from(vec![
                            prompt.clone(),
                            Span::styled(segment.to_string(), Style::default().fg(text_color)),
                        ]));
                    } else {
                        result.push(ratatui::text::Line::from(vec![Span::styled(
                            segment.to_string(),
                            Style::default().fg(text_color),
                        )]));
                    }
                }
                if result.is_empty() {
                    result.push(ratatui::text::Line::from(vec![prompt]));
                }
                result
            };

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).style(
            Style::default()
                .fg(text_color)
                .bg(self.theme.background_element),
        );
        paragraph.render(area, buf);

        let prompt_w = state.prompt.width();
        let first_line_avail = (area.width as usize).saturating_sub(prompt_w);
        let subsequent_line_avail = area.width as usize;

        let byte_idx = state
            .value
            .char_indices()
            .nth(state.cursor.min(state.value.chars().count()))
            .map(|(i, _)| i)
            .unwrap_or(state.value.len());
        let prefix = &state.value[..byte_idx];

        let mut row = 0u16;
        let mut col = 0usize;
        let mut avail = first_line_avail;

        for ch in prefix.chars() {
            if ch == '\n' {
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
                continue;
            }
            let w = ch.width().unwrap_or(1);
            if avail > 0 && col + w > avail {
                row += 1;
                col = 0;
                avail = subsequent_line_avail;
            }
            if avail > 0 {
                col += w;
                avail = avail.saturating_sub(w);
            }
        }

        let cursor_x = if row == 0 {
            area.x + prompt_w as u16 + col as u16
        } else {
            area.x + col as u16
        };
        let cursor_y = area.y + row;

        if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
            buf.cell_mut((cursor_x, cursor_y))
                .unwrap()
                .set_symbol("\u{2588}")
                .set_style(
                    Style::default()
                        .fg(self.theme.primary)
                        .bg(self.theme.primary),
                );
        }
    }
}
