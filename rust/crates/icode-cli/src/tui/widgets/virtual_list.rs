use ratatui::layout::Rect;
use ratatui::prelude::StatefulWidget;
use ratatui::style::Style;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

pub struct VirtualListState {
    pub offset: usize,
    pub focus_index: usize,
    pub item_heights: Vec<Option<u16>>,
    pub total_height: Option<u16>,
    pub section_header_at: Option<usize>,
}

impl VirtualListState {
    pub fn new() -> Self {
        Self {
            offset: 0,
            focus_index: 0,
            item_heights: Vec::new(),
            total_height: None,
            section_header_at: None,
        }
    }

    pub fn scroll_up(&mut self) {
        if self.focus_index == 0 {
            return;
        }
        self.focus_index -= 1;

        let focus_height = self.cumulative_height_up_to(self.focus_index);
        let current_top = self.cumulative_height_up_to(self.offset);
        if focus_height < current_top {
            self.offset = self.focus_index;
            self.total_height = None;
        }
    }

    pub fn scroll_down(&mut self, viewport_height: u16) {
        let total = self.item_heights.len();
        if self.focus_index + 1 >= total {
            return;
        }
        self.focus_index += 1;

        let focus_bottom = self.cumulative_height_up_to(self.focus_index + 1);
        let visible_bottom = self.cumulative_height_up_to(self.offset) + viewport_height;
        if focus_bottom > visible_bottom {
            self.scroll_to_focus_bottom(viewport_height);
        }
    }

    pub fn page_up(&mut self, viewport_height: u16) {
        if self.offset == 0 {
            self.focus_index = 0;
            return;
        }

        let target_top = self
            .cumulative_height_up_to(self.offset)
            .saturating_sub(viewport_height);
        self.offset = self.find_item_at_or_below_height(target_top);
        self.total_height = None;
        self.focus_index = self.offset;
    }

    pub fn page_down(&mut self, viewport_height: u16) {
        let current_bottom = self.cumulative_height_up_to(self.offset) + viewport_height;
        let total_h = self.estimated_total_height(viewport_height);
        if current_bottom >= total_h {
            if !self.item_heights.is_empty() {
                self.focus_index = self.item_heights.len() - 1;
            }
            return;
        }

        let target_bottom = current_bottom + viewport_height;
        self.offset = self.find_item_at_or_below_height(target_bottom);
        self.total_height = None;
        self.focus_index = self.offset;
    }

    pub fn go_to_top(&mut self) {
        self.offset = 0;
        self.focus_index = 0;
        self.total_height = None;
    }

    pub fn go_to_bottom(&mut self, viewport_height: u16) {
        let total = self.item_heights.len();
        if total == 0 {
            return;
        }
        self.focus_index = total - 1;
        self.offset = total - 1;
        self.total_height = None;
        self.scroll_to_focus_bottom(viewport_height);
    }

    pub fn ensure_visible(&mut self, index: usize, viewport_height: u16) {
        let total = self.item_heights.len();
        if index >= total {
            return;
        }

        self.focus_index = index;

        let item_top = self.cumulative_height_up_to(index);
        let item_bottom = self.cumulative_height_up_to(index + 1);
        let current_top = self.cumulative_height_up_to(self.offset);
        let current_bottom = current_top + viewport_height;

        if item_top < current_top {
            self.offset = index;
            self.total_height = None;
        } else if item_bottom > current_bottom {
            self.offset = index;
            self.total_height = None;
            let item_height = item_bottom.saturating_sub(item_top);
            if item_height < viewport_height {
                let desired_top = item_bottom.saturating_sub(viewport_height);
                self.offset = self.find_item_at_or_below_height(desired_top);
            }
        }
    }

    pub fn visible_range(
        &mut self,
        viewport_height: u16,
        total_items: usize,
        estimate_height: u16,
    ) -> (usize, usize) {
        if total_items == 0 || viewport_height == 0 {
            return (0, 0);
        }

        self.ensure_heights(total_items, estimate_height);

        let mut start = self.offset;
        if start >= total_items {
            start = total_items.saturating_sub(1);
        }

        let mut end = start;
        let mut accumulated: u16 = 0;
        let top_offset = self.cumulative_height_up_to(start);

        while end < total_items {
            let h = self.height_of(end, estimate_height);
            if accumulated > 0 && accumulated + h > viewport_height {
                break;
            }
            accumulated += h;
            end += 1;
        }

        if end < total_items {
            end += 1;
        }

        if start > 0 {
            start -= 1;
        }

        (start, end.min(total_items))
    }

    pub fn render<F, H>(
        &mut self,
        f: &mut Frame,
        area: Rect,
        total_items: usize,
        estimate_height: u16,
        mut render_item: F,
        mut section_header_renderer: Option<&mut H>,
    ) where
        F: FnMut(&mut Frame, Rect, usize) -> u16,
        H: FnMut(&mut Frame, Rect, usize),
    {
        if area.height == 0 || total_items == 0 {
            return;
        }

        self.ensure_heights(total_items, estimate_height);

        let (start, end) = self.visible_range(area.height, total_items, estimate_height);
        if start >= end {
            return;
        }

        let top_offset = self.cumulative_height_up_to(start);
        let scroll_offset_within_start = self
            .cumulative_height_up_to(self.offset)
            .saturating_sub(top_offset);

        let mut y_offset: i32 = -(scroll_offset_within_start as i32);
        let mut measured_height: u16 = 0;

        for idx in start..end {
            if idx >= total_items {
                break;
            }

            let item_area = Rect {
                x: area.x,
                y: area.y.saturating_add(y_offset.max(0) as u16),
                width: area.width,
                height: area.height,
            };

            let h = self.height_of(idx, estimate_height);
            if y_offset + h as i32 > 0 && y_offset < area.height as i32 {
                let actual_height = render_item(f, item_area, idx);
                self.item_heights[idx] = Some(actual_height);
                measured_height += actual_height;
            }

            y_offset += h as i32;
        }

        self.total_height = Some(measured_height);

        let total_h = self.estimated_total_height(area.height);
        if total_h > area.height {
            let scroll_pos = self.cumulative_height_up_to(self.offset);
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("\u{2191}"))
                .end_symbol(Some("\u{2193}"))
                .track_symbol(Some(" "))
                .thumb_symbol("\u{2588}")
                .style(Style::default())
                .render(
                    area,
                    f.buffer_mut(),
                    &mut ScrollbarState::new(total_h as usize)
                        .position(scroll_pos as usize)
                        .viewport_content_length(area.height as usize),
                );
        }

        if let Some(header_fn) = section_header_renderer.as_mut() {
            if let Some(section_idx) = self.section_header_at {
                let next_section = self.find_next_section(section_idx, start, end, total_items);
                header_fn(f, area, next_section);
            }
        }
    }

    fn ensure_heights(&mut self, total_items: usize, estimate_height: u16) {
        let current_len = self.item_heights.len();
        if current_len < total_items {
            self.item_heights.resize(total_items, Some(estimate_height));
            self.total_height = None;
        }
    }

    fn height_of(&self, index: usize, estimate: u16) -> u16 {
        self.item_heights
            .get(index)
            .and_then(|h| *h)
            .unwrap_or(estimate)
    }

    fn cumulative_height_up_to(&self, index: usize) -> u16 {
        let mut total: u16 = 0;
        for i in 0..index.min(self.item_heights.len()) {
            total += self.item_heights[i].unwrap_or(1);
        }
        total
    }

    fn estimated_total_height(&self, viewport_height: u16) -> u16 {
        if let Some(h) = self.total_height {
            return h;
        }
        let mut total: u16 = 0;
        for h in &self.item_heights {
            total += h.unwrap_or(viewport_height / 4);
        }
        total
    }

    fn find_item_at_or_below_height(&self, target: u16) -> usize {
        let mut accumulated: u16 = 0;
        for (i, h) in self.item_heights.iter().enumerate() {
            let item_h = h.unwrap_or(1);
            if accumulated + item_h > target {
                return i;
            }
            accumulated += item_h;
        }
        self.item_heights.len().saturating_sub(1)
    }

    fn scroll_to_focus_bottom(&mut self, viewport_height: u16) {
        let focus_bottom = self.cumulative_height_up_to(self.focus_index + 1);
        if focus_bottom > self.cumulative_height_up_to(self.offset) + viewport_height {
            let desired_top = focus_bottom.saturating_sub(viewport_height);
            self.offset = self.find_item_at_or_below_height(desired_top);
        }
    }

    fn find_next_section(
        &self,
        current_section: usize,
        _start: usize,
        _end: usize,
        total_items: usize,
    ) -> usize {
        current_section.min(total_items.saturating_sub(1))
    }
}

impl Default for VirtualListState {
    fn default() -> Self {
        Self::new()
    }
}
