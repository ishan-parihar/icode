use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

#[derive(Default)]
pub struct PagerState {
    pub open: bool,
    pub title: String,
    pub content: String,
    pub scroll: usize,
}

impl PagerState {
    pub fn new(title: String, content: String) -> Self {
        Self {
            open: true,
            title,
            content,
            scroll: 0,
        }
    }

    pub fn open(&mut self, title: String, content: String) {
        self.open = true;
        self.title = title;
        self.content = content;
        self.scroll = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn total_lines(&self) -> usize {
        self.content.lines().count().max(1)
    }

    pub fn scroll_down(&mut self) {
        let max_scroll = self.total_lines().saturating_sub(1);
        if self.scroll < max_scroll {
            self.scroll += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_page_down(&mut self, visible_lines: usize) {
        let step = (visible_lines / 2).max(1);
        let max_scroll = self.total_lines().saturating_sub(1);
        self.scroll = (self.scroll + step).min(max_scroll);
    }

    pub fn scroll_page_up(&mut self, visible_lines: usize) {
        let step = (visible_lines / 2).max(1);
        self.scroll = self.scroll.saturating_sub(step);
    }

    pub fn go_to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn go_to_bottom(&mut self, visible_lines: usize) {
        let max_scroll = self.total_lines().saturating_sub(visible_lines);
        self.scroll = max_scroll;
    }
}


pub fn render_pager(
    frame: &mut Frame,
    state: &PagerState,
    area: Rect,
    theme_colors: impl Fn() -> (Color, Color, Color, Color),
) {
    if !state.open {
        return;
    }

    let (bg_color, text_color, border_color, status_bg) = theme_colors();

    let overlay_width = area.width.saturating_sub(4).min(120).max(40);
    let overlay_height = area.height.saturating_sub(4).max(10);
    let overlay_x = (area.width.saturating_sub(overlay_width)) / 2;
    let overlay_y = (area.height.saturating_sub(overlay_height)) / 2;

    let overlay_area = Rect {
        x: area.x + overlay_x,
        y: area.y + overlay_y,
        width: overlay_width,
        height: overlay_height,
    };

    let block = Block::default()
        .title(format!(" {} (Pager) ", state.title))
        .title_style(
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg_color));

    let inner_area = block.inner(overlay_area);
    frame.render_widget(block, overlay_area);

    if inner_area.height < 2 {
        return;
    }

    let content_height = inner_area.height.saturating_sub(1) as usize;
    let content_width = inner_area.width as usize;

    let wrapped_lines = wrap_content_lines(&state.content, content_width);

    let total_wrapped = wrapped_lines.len();
    let max_scroll = total_wrapped.saturating_sub(content_height);
    let scroll = state.scroll.min(max_scroll);

    let visible: Vec<Line> = wrapped_lines
        .iter()
        .skip(scroll)
        .take(content_height)
        .map(|text| Line::from(Span::styled(text.clone(), Style::default().fg(text_color))))
        .collect();

    let paragraph = Paragraph::new(visible);
    let content_area = Rect {
        x: inner_area.x,
        y: inner_area.y,
        width: inner_area.width,
        height: content_height as u16,
    };
    frame.render_widget(paragraph, content_area);

    let total_displayed = state.total_lines();
    let pct = if total_displayed > 0 {
        ((scroll as f64 / total_wrapped.max(1) as f64) * 100.0) as u32
    } else {
        0
    };
    let status = format!(
        " {}/{} ({}%) j/k scroll, g/G top/bottom, PgUp/Dn page, q quit ",
        scroll + 1,
        total_wrapped,
        pct
    );
    let status_bar = Paragraph::new(status).style(Style::default().bg(status_bg).fg(text_color));

    let status_area = Rect {
        x: inner_area.x,
        y: inner_area.bottom().saturating_sub(1),
        width: inner_area.width,
        height: 1,
    };
    frame.render_widget(status_bar, status_area);
}

fn wrap_content_lines(content: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for line in content.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut remaining = line;
        while !remaining.is_empty() {
            if remaining.chars().count() <= width {
                result.push(remaining.to_string());
                break;
            }
            let mut split_at = width;
            let mut char_idx = 0;
            for (i, ch) in remaining.char_indices() {
                if char_idx >= width {
                    split_at = i;
                    break;
                }
                char_idx += 1;
            }
            let mut break_pos = None;
            let chars: Vec<(usize, char)> = remaining.char_indices().take(split_at).collect();
            for (idx, ch) in chars.iter().rev() {
                if *ch == ' ' {
                    break_pos = Some(*idx);
                    break;
                }
            }
            let (chunk, rest) = if let Some(pos) = break_pos {
                (&remaining[..pos], &remaining[pos + 1..])
            } else {
                let mut byte_pos = 0;
                let mut count = 0;
                for (i, ch) in remaining.char_indices() {
                    if count >= width {
                        byte_pos = i;
                        break;
                    }
                    count += 1;
                    byte_pos = i + ch.len_utf8();
                }
                (&remaining[..byte_pos], &remaining[byte_pos..])
            };
            result.push(chunk.to_string());
            remaining = rest;
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}
