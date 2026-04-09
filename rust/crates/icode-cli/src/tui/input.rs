use crate::tui::frecency::FrecencyStore;
use crate::tui::theme::Theme;
use ratatui::prelude::Widget;
use ratatui::widgets::{StatefulWidget, Wrap};
use ratatui::{buffer::Buffer, layout::Rect, style::Style, text::Span, widgets::Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const KNOWN_AGENTS: &[&str] = &["build", "plan", "debug", "review", "test"];

#[derive(Debug, Clone)]
enum InputSegment {
    Text(String),
    FileChip(String),
    AgentChip(String),
}

fn parse_input_segments(value: &str) -> Vec<InputSegment> {
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '@' && i + 1 < len && is_ref_char(chars[i + 1]) {
            if !current_text.is_empty() {
                segments.push(InputSegment::Text(std::mem::take(&mut current_text)));
            }
            let start = i;
            i += 1;
            while i < len && is_ref_char(chars[i]) {
                i += 1;
            }
            let ref_text: String = chars[start..i].iter().collect();
            let ref_name = &ref_text[1..];
            if KNOWN_AGENTS.contains(&ref_name) {
                segments.push(InputSegment::AgentChip(ref_text));
            } else {
                segments.push(InputSegment::FileChip(ref_text));
            }
        } else {
            current_text.push(chars[i]);
            i += 1;
        }
    }

    if !current_text.is_empty() {
        segments.push(InputSegment::Text(current_text));
    }

    segments
}

fn is_ref_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-'
}

#[derive(Debug, Default)]
pub struct InputState {
    pub value: String,
    pub cursor: usize,
    pub prompt: String,
    pub placeholder: String,
    pub history: Vec<String>,
    pub history_idx: usize,
    pub history_temp: Option<String>,
    pub frecency: Option<FrecencyStore>,
    available_models: Vec<String>,
    available_sessions: Vec<String>,
    cwd: String,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_width: u16,
    pub scroll_offset: usize,
    pub shell_mode: bool,
}

impl InputState {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            placeholder: "Ask icode to do anything...".into(),
            cursor_x: 0,
            cursor_y: 0,
            cursor_width: 1,
            ..Default::default()
        }
    }

    fn char_to_byte(&self, char_offset: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_offset)
            .map_or(self.value.len(), |(byte_idx, _)| byte_idx)
    }

    pub fn insert_char(&mut self, c: char) {
        if c == '!' && self.cursor == 0 && self.value.is_empty() {
            self.shell_mode = !self.shell_mode;
            return;
        }
        let byte_idx = self.char_to_byte(self.cursor);
        self.value.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        let byte_idx = self.char_to_byte(self.cursor);
        self.value.insert_str(byte_idx, s);
        self.cursor += s.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let byte_idx = self.char_to_byte(self.cursor);
        let prev_byte_idx = self.value[..byte_idx]
            .char_indices()
            .last()
            .map_or(0, |(idx, _)| idx);
        self.value.drain(prev_byte_idx..byte_idx);
        self.cursor -= 1;
        if self.shell_mode && self.cursor == 0 && self.value.is_empty() {
            self.shell_mode = false;
        }
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
    }

    pub fn delete_to_end(&mut self) {
        let byte_idx = self.char_to_byte(self.cursor);
        self.value.drain(byte_idx..);
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
        if target_row < self.scroll_offset {
            self.scroll_offset = target_row;
        }
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
        if target_row < self.scroll_offset {
            self.scroll_offset = target_row;
        }
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
            if row == target_row && col >= target_col {
                return char_offset;
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
        self.shell_mode = false;
    }

    pub fn toggle_shell_mode(&mut self) {
        self.shell_mode = !self.shell_mode;
    }

    pub fn is_shell_mode(&self) -> bool {
        self.shell_mode
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

    pub fn history_suggestions(&self, prefix: &str, limit: usize) -> Vec<String> {
        if let Some(ref frecency) = self.frecency {
            frecency.suggestions(prefix, limit)
        } else {
            let prefix_lower = prefix.to_lowercase();
            self.history
                .iter()
                .filter(|h| h.to_lowercase().starts_with(&prefix_lower))
                .take(limit)
                .cloned()
                .collect()
        }
    }

    pub fn frecency_top_entries(&self, limit: usize) -> Vec<String> {
        if let Some(ref frecency) = self.frecency {
            frecency.top_entries(limit)
        } else {
            self.history.iter().rev().take(limit).cloned().collect()
        }
    }

    pub fn set_models(&mut self, models: Vec<String>) {
        self.available_models = models;
    }

    pub fn set_sessions(&mut self, sessions: Vec<String>) {
        self.available_sessions = sessions;
    }

    pub fn set_cwd(&mut self, cwd: String) {
        self.cwd = cwd;
    }

    pub fn submit(&mut self) -> String {
        self.shell_mode = false;
        let value = std::mem::take(&mut self.value);
        if let Some(ref mut frecency) = self.frecency {
            frecency.record(&value);
        }
        self.cursor = 0;
        value
    }

    pub fn visible_text(&self) -> String {
        if self.value.is_empty() {
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
            let wraps = line_w.div_ceil(avail);
            count += wraps.max(1);
        }
        count.max(1)
    }

    /// Adjusts scroll_offset so the cursor is within the visible area.
    /// Call after any cursor or content change that may move the cursor
    /// outside the visible window.
    pub fn ensure_cursor_visible(&mut self, max_width: usize, max_visible_lines: usize) {
        if max_visible_lines == 0 {
            return;
        }
        let (cursor_row, _) = self.cursor_position(max_width);
        let total = self.total_rows(max_width);

        if cursor_row < self.scroll_offset {
            self.scroll_offset = cursor_row;
        } else if cursor_row >= self.scroll_offset + max_visible_lines {
            self.scroll_offset = cursor_row + 1 - max_visible_lines;
        }

        let max_scroll = total.saturating_sub(max_visible_lines);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
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
        let max_width = area.width as usize;
        let max_visible_lines = area.height as usize;

        state.ensure_cursor_visible(max_width, max_visible_lines);

        let bg_fill = Paragraph::new("").style(Style::default().bg(self.theme.background_element));
        bg_fill.render(area, buf);

        let prompt_color = if state.shell_mode {
            self.theme.warning
        } else {
            self.theme.border_active
        };
        let prompt = Span::styled(&state.prompt, Style::default().fg(prompt_color));

        let all_lines: Vec<ratatui::text::Line<'_>> = if state.value.is_empty() {
            let placeholder = Span::styled(
                &state.placeholder,
                Style::default()
                    .fg(muted_color)
                    .add_modifier(ratatui::style::Modifier::ITALIC),
            );
            vec![ratatui::text::Line::from(vec![prompt.clone(), placeholder])]
        } else {
            let mut result = Vec::new();
            for (line_idx, segment) in state.value.split('\n').enumerate() {
                if line_idx == 0 {
                    let mut spans = vec![prompt.clone()];
                    for chip in parse_input_segments(segment) {
                        match chip {
                            InputSegment::Text(t) => {
                                spans.push(Span::styled(t, Style::default().fg(text_color)));
                            }
                            InputSegment::FileChip(t) => {
                                spans.push(Span::styled(
                                    format!(" {t} "),
                                    Style::default()
                                        .fg(self.theme.info)
                                        .bg(self.theme.background_hover),
                                ));
                            }
                            InputSegment::AgentChip(t) => {
                                spans.push(Span::styled(
                                    format!(" {t} "),
                                    Style::default()
                                        .fg(self.theme.accent)
                                        .bg(self.theme.background_hover),
                                ));
                            }
                        }
                    }
                    result.push(ratatui::text::Line::from(spans));
                } else {
                    let mut spans = Vec::new();
                    for chip in parse_input_segments(segment) {
                        match chip {
                            InputSegment::Text(t) => {
                                spans.push(Span::styled(t, Style::default().fg(text_color)));
                            }
                            InputSegment::FileChip(t) => {
                                spans.push(Span::styled(
                                    format!(" {t} "),
                                    Style::default()
                                        .fg(self.theme.info)
                                        .bg(self.theme.background_hover),
                                ));
                            }
                            InputSegment::AgentChip(t) => {
                                spans.push(Span::styled(
                                    format!(" {t} "),
                                    Style::default()
                                        .fg(self.theme.accent)
                                        .bg(self.theme.background_hover),
                                ));
                            }
                        }
                    }
                    result.push(ratatui::text::Line::from(spans));
                }
            }
            if result.is_empty() {
                result.push(ratatui::text::Line::from(vec![prompt]));
            }
            result
        };

        let visible_lines: Vec<_> = all_lines
            .iter()
            .skip(state.scroll_offset)
            .take(max_visible_lines)
            .cloned()
            .collect();

        let paragraph = Paragraph::new(visible_lines)
            .wrap(Wrap { trim: false })
            .style(
                Style::default()
                    .fg(text_color)
                    .bg(self.theme.background_element),
            );
        paragraph.render(area, buf);

        let prompt_w = state.prompt.width();
        let first_line_avail = max_width.saturating_sub(prompt_w);
        let subsequent_line_avail = max_width;

        let byte_idx = state
            .value
            .char_indices()
            .nth(state.cursor.min(state.value.chars().count()))
            .map_or(state.value.len(), |(i, _)| i);
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

        let visible_row = row as isize - state.scroll_offset as isize;
        let cursor_x = match visible_row.cmp(&0) {
            std::cmp::Ordering::Equal => area.x + prompt_w as u16 + col as u16,
            std::cmp::Ordering::Greater => area.x + col as u16,
            std::cmp::Ordering::Less => 0,
        };
        let cursor_y = area.y + visible_row.max(0) as u16;

        state.cursor_x = cursor_x;
        state.cursor_y = cursor_y;
        state.cursor_width = 1;

        if visible_row >= 0 && cursor_x < area.x + area.width && cursor_y < area.y + area.height {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_text_no_chips() {
        let segments = parse_input_segments("hello world");
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], InputSegment::Text(t) if t == "hello world"));
    }

    #[test]
    fn parse_single_file_chip() {
        let segments = parse_input_segments("@main.rs");
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], InputSegment::FileChip(t) if t == "@main.rs"));
    }

    #[test]
    fn parse_single_agent_chip() {
        let segments = parse_input_segments("@build");
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], InputSegment::AgentChip(t) if t == "@build"));
    }

    #[test]
    fn parse_mixed_text_and_chips() {
        let segments = parse_input_segments("fix @main.rs using @plan");
        assert_eq!(segments.len(), 4);
        assert!(matches!(&segments[0], InputSegment::Text(t) if t == "fix "));
        assert!(matches!(&segments[1], InputSegment::FileChip(t) if t == "@main.rs"));
        assert!(matches!(&segments[2], InputSegment::Text(t) if t == " using "));
        assert!(matches!(&segments[3], InputSegment::AgentChip(t) if t == "@plan"));
    }

    #[test]
    fn parse_file_chip_with_path() {
        let segments = parse_input_segments("@src/main.rs");
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], InputSegment::FileChip(t) if t == "@src/main.rs"));
    }

    #[test]
    fn parse_at_without_space_before_is_still_chip() {
        let segments = parse_input_segments("email@test.com");
        assert_eq!(segments.len(), 2);
        assert!(matches!(&segments[0], InputSegment::Text(t) if t == "email"));
        assert!(matches!(&segments[1], InputSegment::FileChip(t) if t == "@test.com"));
    }

    #[test]
    fn parse_empty_input() {
        let segments = parse_input_segments("");
        assert_eq!(segments.len(), 0);
    }

    #[test]
    fn parse_all_known_agents() {
        for agent in KNOWN_AGENTS {
            let input = format!("@{agent}");
            let segments = parse_input_segments(&input);
            assert_eq!(segments.len(), 1);
            assert!(
                matches!(&segments[0], InputSegment::AgentChip(t) if t == &input),
                "Expected AgentChip for @{}, got {:?}",
                agent,
                segments[0]
            );
        }
    }

    #[test]
    fn parse_multiple_file_chips() {
        let segments = parse_input_segments("@file1.txt and @file2.rs");
        assert_eq!(segments.len(), 3);
        assert!(matches!(&segments[0], InputSegment::FileChip(t) if t == "@file1.txt"));
        assert!(matches!(&segments[1], InputSegment::Text(t) if t == " and "));
        assert!(matches!(&segments[2], InputSegment::FileChip(t) if t == "@file2.rs"));
    }

    #[test]
    fn parse_chip_with_hyphens() {
        let segments = parse_input_segments("@my-file");
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], InputSegment::FileChip(t) if t == "@my-file"));
    }

    #[test]
    fn is_ref_char_valid() {
        assert!(is_ref_char('a'));
        assert!(is_ref_char('Z'));
        assert!(is_ref_char('0'));
        assert!(is_ref_char('_'));
        assert!(is_ref_char('.'));
        assert!(is_ref_char('/'));
        assert!(is_ref_char('-'));
        assert!(!is_ref_char(' '));
        assert!(!is_ref_char('@'));
        assert!(!is_ref_char('\n'));
    }

    #[test]
    fn scroll_offset_defaults_to_zero() {
        let state = InputState::new("> ");
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn ensure_cursor_visible_scrolls_down_when_cursor_below_visible() {
        let mut state = InputState::new("> ");
        state.value = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8".to_string();
        state.cursor = state.value.chars().count();
        state.scroll_offset = 0;

        let max_width = 80;
        let max_visible_lines = 3;
        state.ensure_cursor_visible(max_width, max_visible_lines);

        assert!(
            state.scroll_offset > 0,
            "scroll_offset should increase when cursor is below visible area"
        );
        let (cursor_row, _) = state.cursor_position(max_width);
        assert!(
            cursor_row < state.scroll_offset + max_visible_lines,
            "cursor row {} should be within visible window [{}, {})",
            cursor_row,
            state.scroll_offset,
            state.scroll_offset + max_visible_lines
        );
    }

    #[test]
    fn ensure_cursor_visible_no_scroll_when_cursor_in_view() {
        let mut state = InputState::new("> ");
        state.value = "short text".to_string();
        state.cursor = state.value.chars().count();
        state.scroll_offset = 0;

        let max_width = 80;
        let max_visible_lines = 3;
        state.ensure_cursor_visible(max_width, max_visible_lines);

        assert_eq!(
            state.scroll_offset, 0,
            "scroll_offset should stay 0 when cursor fits"
        );
    }

    #[test]
    fn ensure_cursor_visible_scrolls_up_when_cursor_above_visible() {
        let mut state = InputState::new("> ");
        state.value = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8".to_string();
        state.cursor = state.value.chars().count();
        let max_width = 80;
        let max_visible_lines = 3;
        state.ensure_cursor_visible(max_width, max_visible_lines);
        let scroll_after_fill = state.scroll_offset;
        assert!(
            scroll_after_fill > 0,
            "should have scrolled down to see end"
        );

        state.move_home();
        state.ensure_cursor_visible(max_width, max_visible_lines);
        assert_eq!(
            state.scroll_offset, 0,
            "scroll_offset should return to 0 when cursor is at the top"
        );
    }

    #[test]
    fn move_up_decreases_scroll_offset_when_crossing_boundary() {
        let mut state = InputState::new("> ");
        state.value = "line1\nline2\nline3\nline4\nline5".to_string();
        state.cursor = state.value.chars().count();
        state.scroll_offset = 3;

        let max_width = 80;
        let moved = state.move_up(max_width);
        assert!(moved);

        let (cursor_row, _) = state.cursor_position(max_width);
        assert!(
            cursor_row >= state.scroll_offset,
            "cursor row {} should not be above scroll_offset {}",
            cursor_row,
            state.scroll_offset
        );
    }

    #[test]
    fn multiline_text_produces_correct_row_count() {
        let mut state = InputState::new("> ");
        state.value = "line1\nline2\nline3".to_string();
        state.cursor = state.value.chars().count();

        let max_width = 80;
        let total = state.total_rows(max_width);
        assert_eq!(total, 3, "three explicit newlines should produce 3 rows");
    }

    #[test]
    fn long_text_wraps_into_multiple_rows() {
        let mut state = InputState::new("> ");
        state.value = "a".repeat(200);
        state.cursor = state.value.chars().count();

        let max_width = 40;
        let total = state.total_rows(max_width);
        assert!(
            total > 1,
            "long text should wrap into multiple rows at width {max_width}"
        );
    }

    #[test]
    fn test_shell_mode_toggle_on_bang_at_start() {
        let mut state = InputState::new("> ");
        assert!(!state.shell_mode);
        state.insert_char('!');
        assert!(state.shell_mode);
        assert!(state.value.is_empty());
        state.insert_char('!');
        assert!(!state.shell_mode);
        assert!(state.value.is_empty());
    }

    #[test]
    fn test_shell_mode_not_toggled_with_text() {
        let mut state = InputState::new("> ");
        state.value = "hello".to_string();
        state.cursor = state.value.chars().count();
        state.insert_char('!');
        assert!(!state.shell_mode);
        assert_eq!(state.value, "hello!");
    }

    #[test]
    fn test_shell_mode_clears_on_clear() {
        let mut state = InputState::new("> ");
        state.insert_char('!');
        assert!(state.shell_mode);
        state.value = "some text".to_string();
        state.cursor = state.value.chars().count();
        state.clear();
        assert!(!state.shell_mode);
        assert!(state.value.is_empty());
    }

    #[test]
    fn test_shell_mode_prompt_color() {
        let mut state = InputState::new("> ");
        assert!(!state.is_shell_mode());
        state.toggle_shell_mode();
        assert!(state.is_shell_mode());
        state.toggle_shell_mode();
        assert!(!state.is_shell_mode());
    }

    #[test]
    fn test_shell_mode_clears_on_submit() {
        let mut state = InputState::new("> ");
        state.insert_char('!');
        state.value = "echo hello".to_string();
        state.cursor = state.value.chars().count();
        let submitted = state.submit();
        assert_eq!(submitted, "echo hello");
        assert!(!state.shell_mode);
    }

    #[test]
    fn test_shell_mode_not_toggled_by_non_bang() {
        let mut state = InputState::new("> ");
        state.insert_char('a');
        assert!(!state.shell_mode);
        assert_eq!(state.value, "a");
    }

    #[test]
    fn test_bang_at_non_zero_position_does_not_toggle() {
        let mut state = InputState::new("> ");
        state.value = "test".to_string();
        state.cursor = 2;
        state.insert_char('!');
        assert!(!state.shell_mode);
        assert_eq!(state.value, "te!st");
    }
}
