use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear};
use ratatui::Frame;

use crate::tui::theme::Theme;

/// Calculate a centered popup area.
///
/// `width_pct` is the percentage of screen width to use (e.g. 0.5 for 50%).
/// `min_w` / `max_w` clamp the resulting width.
/// `max_h_pct` is the maximum height as a fraction of screen height (e.g. 0.5).
/// `content_height` is the natural height of the content; the popup will not
/// exceed it.
pub fn popup_dimensions(
    area: Rect,
    width_pct: f32,
    min_w: u16,
    max_w: u16,
    max_h_pct: f32,
    content_height: u16,
) -> Rect {
    let width = (((area.width as f32) * width_pct).round() as u16)
        .clamp(min_w, max_w)
        .min(area.width.saturating_sub(4));

    let max_h = ((area.height as f32) * max_h_pct) as u16;
    let height = content_height
        .min(max_h)
        .min(area.height.saturating_sub(4))
        .max(4);

    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;

    Rect::new(x, y, width, height)
}

/// Calculate a popup area anchored to a specific screen position.
///
/// Used for autocomplete-style popups that appear below the cursor.
/// `max_items` limits the visible rows; the actual height will not exceed
/// `anchor_y` (to avoid rendering above the terminal top).
pub fn anchored_popup(
    area: Rect,
    anchor_x: u16,
    anchor_y: u16,
    anchor_width: u16,
    item_count: u16,
    max_items: u16,
) -> Rect {
    let width = anchor_width
        .clamp(20, 60)
        .min(area.width.saturating_sub(anchor_x));
    let height = item_count.min(max_items).min(anchor_y).max(1);

    // Position the popup ABOVE the anchor line (opencode renders above the input)
    let popup_y = anchor_y.saturating_sub(height);

    Rect::new(area.x + anchor_x, area.y + popup_y, width, height)
}
/// Create an opencode-style block with a left-only border.
///
/// Matches the `SplitBorder` pattern used across opencode popups:
/// - Left border only, with custom border characters
/// - Themed border color (warning for alerts, accent for questions, etc.)
/// - Optional background fill
pub fn left_border_block(
    theme: Theme,
    border_color: Color,
    title: &str,
    bg: Option<Color>,
) -> Block<'static> {
    let mut block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border_color))
        .border_type(BorderType::Thick);

    if !title.is_empty() {
        block = block.title(Span::styled(
            title.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(bg_color) = bg {
        block = block.style(Style::default().bg(bg_color));
    }

    block
}

/// Create a full-bordered popup block (for command palette, etc.).
///
/// Uses the standard dialog styling: subtle border, panel background,
/// optional title.
pub fn dialog_block(theme: Theme, title: &str) -> Block<'static> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(theme.background_panel));

    if !title.is_empty() {
        block = block.title(Span::styled(
            title.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ));
    }

    block
}
/// Render a footer hint bar matching opencode's style.
///
/// Each hint is a `(key, action)` pair, e.g. `("enter", "confirm")`.
/// Rendered as: `enter confirm` `esc cancel` with the key in text color
/// and the action in muted color.
pub fn render_hint_bar(frame: &mut Frame, area: Rect, hints: &[(&str, &str)], theme: Theme) {
    if hints.is_empty() || area.height < 1 {
        return;
    }

    let mut spans = Vec::new();
    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme.text),
        ));
        spans.push(Span::styled(
            format!(" {action}"),
            Style::default().fg(theme.text_muted),
        ));
    }

    let hint_line = Line::from(spans);
    let widget = ratatui::widgets::Paragraph::new(hint_line);
    frame.render_widget(widget, area);
}
/// Render `Clear` on the given area to erase underlying content before
/// drawing an overlay popup.
pub fn clear_area(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popup_dimensions_centered() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = popup_dimensions(area, 0.5, 30, 60, 0.5, 20);

        assert_eq!(popup.width, 50);
        assert_eq!(popup.height, 20);
        assert_eq!(popup.x, 25);
        assert_eq!(popup.y, 15);
    }

    #[test]
    fn test_popup_dimensions_clamps_min_width() {
        let area = Rect::new(0, 0, 40, 30);
        let popup = popup_dimensions(area, 0.5, 30, 60, 0.5, 20);

        assert_eq!(popup.width, 30);
    }

    #[test]
    fn test_popup_dimensions_clamps_max_width() {
        let area = Rect::new(0, 0, 200, 50);
        let popup = popup_dimensions(area, 0.5, 30, 60, 0.5, 20);

        assert_eq!(popup.width, 60);
    }

    #[test]
    fn test_popup_dimensions_height_from_content() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = popup_dimensions(area, 0.5, 30, 60, 0.5, 8);
        assert_eq!(popup.height, 8);
    }

    #[test]
    fn test_anchored_popup_basic() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = anchored_popup(area, 10, 20, 40, 5, 10);

        assert_eq!(popup.width, 40);
        assert_eq!(popup.height, 5);
        assert_eq!(popup.x, 10);
        assert_eq!(popup.y, 15);
    }

    #[test]
    fn test_anchored_popup_caps_height() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = anchored_popup(area, 10, 20, 40, 15, 10);
        assert_eq!(popup.height, 10);
    }

    #[test]
    fn test_anchored_popup_respects_anchor_y() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = anchored_popup(area, 10, 3, 40, 10, 10);
        assert_eq!(popup.height, 3);
        assert_eq!(popup.y, 0);
    }

    #[test]
    fn test_anchored_popup_min_height() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = anchored_popup(area, 10, 20, 40, 0, 10);
        assert_eq!(popup.height, 1);
    }

    #[test]
    fn test_anchored_popup_width_clamped() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = anchored_popup(area, 10, 20, 10, 5, 10);
        assert_eq!(popup.width, 20);

        let popup = anchored_popup(area, 10, 20, 80, 5, 10);
        assert_eq!(popup.width, 60);
    }

    #[test]
    fn test_hint_bar_renders_empty_safely() {
        let area = Rect::new(0, 0, 100, 1);
        assert!(area.height >= 1);
    }
}
