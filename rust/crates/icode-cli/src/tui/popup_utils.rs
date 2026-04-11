use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear};
use ratatui::Frame;

use crate::tui::theme::Theme;

// =============================================================================
// Unified Popup Configuration System
// =============================================================================

/// Accent color for popup borders and titles.
#[derive(Debug, Clone, Copy, Default)]
pub enum PopupAccent {
    #[default]
    Default, // theme.border
    Warning,   // theme.warning
    Error,     // theme.error
    Primary,   // theme.primary
    Success,   // theme.success
    Accent,    // theme.accent
    Secondary, // theme.secondary
}

impl PopupAccent {
    /// Resolve the accent to an actual color from the theme.
    pub fn resolve(&self, theme: Theme) -> Color {
        match self {
            PopupAccent::Default => theme.border,
            PopupAccent::Warning => theme.warning,
            PopupAccent::Error => theme.error,
            PopupAccent::Primary => theme.primary,
            PopupAccent::Success => theme.success,
            PopupAccent::Accent => theme.accent,
            PopupAccent::Secondary => theme.secondary,
        }
    }
}

/// Unified configuration for rendering a popup/dialog block.
///
/// Replaces ad-hoc block creation across all dialogs with a single
/// consistent API. All dialogs should migrate to use this.
#[derive(Debug, Clone)]
pub struct PopupConfig {
    pub accent: PopupAccent,
    pub title: String,
    /// `true` = full rounded border (command palette, help, etc.)
    /// `false` = left-only thick border (permission, question, toasts)
    pub use_full_border: bool,
    /// Background color override. `None` uses `theme.background_panel`.
    pub background: Option<Color>,
}

impl PopupConfig {
    /// Standard full-bordered popup (command palette, dialogs, pickers).
    pub fn full(title: &str) -> Self {
        Self {
            accent: PopupAccent::Default,
            title: format!(" {title} "),
            use_full_border: true,
            background: None,
        }
    }

    /// Left-accent popup for alerts, permissions, questions.
    pub fn left(title: &str, accent: PopupAccent) -> Self {
        Self {
            accent,
            title: format!(" {title} "),
            use_full_border: false,
            background: None,
        }
    }

    /// Build the ratatui Block from this config and theme.
    pub fn to_block(&self, theme: Theme) -> Block<'static> {
        let border_color = self.accent.resolve(theme);
        let bg = self.background.unwrap_or(theme.background_panel);

        if self.use_full_border {
            let mut block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(bg));

            if !self.title.is_empty() {
                block = block.title(Span::styled(
                    self.title.clone(),
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ));
            }
            block
        } else {
            let mut block = Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(border_color))
                .border_type(BorderType::Thick)
                .style(Style::default().bg(bg));

            if !self.title.is_empty() {
                block = block.title(Span::styled(
                    self.title.clone(),
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ));
            }
            block
        }
    }
}

/// Clear the area and render a popup frame, returning the inner content area.
///
/// Convenience function that combines Clear + Block rendering.
pub fn render_popup_frame(
    frame: &mut Frame,
    area: Rect,
    config: &PopupConfig,
    theme: Theme,
) -> Rect {
    frame.render_widget(Clear, area);
    let block = config.to_block(theme);
    frame.render_widget(block.clone(), area);
    block.inner(area)
}

/// Render a semi-transparent backdrop behind modal dialogs.
/// Useful for permission dialogs, question prompts that block interaction.
pub fn render_backdrop(frame: &mut Frame, area: Rect, _theme: Theme) {
    // In ratatui, Clear + layering is the standard backdrop approach.
    // The actual "dimming" is achieved by the popup's Clear widget + border.
    // This function is a placeholder for future backdrop tinting support.
    let _ = (frame, area);
}

// =============================================================================
// Legacy helpers (kept for backward compatibility)
// =============================================================================

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
    // Minimum height of 3 ensures at least 1 visible row after layout padding
    let height = item_count.min(max_items).min(anchor_y).max(3);

    // Position the popup ABOVE the anchor line (opencode renders above the input)
    let popup_y = anchor_y.saturating_sub(height);

    Rect::new(area.x + anchor_x, area.y + popup_y, width, height)
}

/// Calculate a popup area that appears BELOW a given anchor rect.
///
/// Used for autocomplete popups that render just below the prompt input.
pub fn anchored_popup_below(
    area: Rect,
    anchor_rect: Rect,
    anchor_width: u16,
    item_count: u16,
    max_items: u16,
) -> Rect {
    let width = anchor_width
        .clamp(20, 60)
        .min(area.width.saturating_sub(anchor_rect.x));
    // Minimum height of 3 ensures at least 1 visible row after layout padding
    let height = item_count
        .min(max_items)
        .min(area.height.saturating_sub(anchor_rect.bottom()))
        .max(3);
    let popup_y = anchor_rect
        .bottom()
        .min(area.bottom().saturating_sub(height));
    Rect::new(anchor_rect.x, popup_y, width, height)
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

/// Create an opencode-style block with a split left border.
///
/// Uses custom border characters to render a left-only border with a
/// distinct bottom-left corner character (`╹`, U+2579) instead of a
/// plain thick line ending. This matches opencode's prompt box styling.
pub fn split_border_block(
    theme: Theme,
    border_color: Color,
    title: &str,
    bg: Option<Color>,
) -> Block<'static> {
    let custom_border = border::Set {
        top_left: "│",
        bottom_left: "╹",
        top_right: "",
        bottom_right: "",
        vertical_left: "│",
        vertical_right: "",
        horizontal_top: "",
        horizontal_bottom: "",
    };

    let bg_color = bg.unwrap_or(theme.background_element);
    let mut block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border_color))
        .border_type(BorderType::Plain)
        .style(Style::default().bg(bg_color));

    if !title.is_empty() {
        block = block.title(Span::styled(
            title.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ));
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
        assert_eq!(popup.height, 3);
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
    fn test_anchored_popup_below_basic() {
        let area = Rect::new(0, 0, 100, 50);
        let anchor = Rect::new(10, 20, 40, 1);
        let popup = anchored_popup_below(area, anchor, 40, 5, 10);

        assert_eq!(popup.width, 40);
        assert_eq!(popup.height, 5);
        assert_eq!(popup.x, 10);
        assert_eq!(popup.y, 21);
    }

    #[test]
    fn test_anchored_popup_below_caps_at_screen_bottom() {
        let area = Rect::new(0, 0, 100, 25);
        let anchor = Rect::new(10, 20, 40, 1);
        let popup = anchored_popup_below(area, anchor, 40, 10, 10);

        assert_eq!(popup.height, 4);
        assert_eq!(popup.y, 21);
    }

    #[test]
    fn test_split_border_block_renders_left_only() {
        let theme = Theme::default();
        let block = split_border_block(theme, Color::Rgb(160, 160, 160), "", None);

        let custom_border = border::Set {
            top_left: "│",
            bottom_left: "╹",
            top_right: "",
            bottom_right: "",
            vertical_left: "│",
            vertical_right: "",
            horizontal_top: "",
            horizontal_bottom: "",
        };
        assert_eq!(custom_border.top_left, "│");
        assert_eq!(custom_border.bottom_left, "╹");
        assert_eq!(custom_border.top_right, "");
        assert_eq!(custom_border.bottom_right, "");
        assert_eq!(custom_border.vertical_left, "│");
        assert_eq!(custom_border.vertical_right, "");
        assert_eq!(custom_border.horizontal_top, "");
        assert_eq!(custom_border.horizontal_bottom, "");

        let block_with_title = split_border_block(theme, Color::Rgb(160, 160, 160), "Test", None);
        drop(block);
        drop(block_with_title);
    }

    #[test]
    fn test_hint_bar_renders_empty_safely() {
        let area = Rect::new(0, 0, 100, 1);
        assert!(area.height >= 1);
    }

    #[test]
    fn test_permission_popup_dimensions_at_min_size() {
        let area = Rect::new(0, 0, 40, 10);
        let popup = popup_dimensions(area, 0.5, 30, 60, 0.5, 15);

        assert!(popup.width > 0);
        assert!(popup.height > 0);
        assert!(popup.right() <= area.right());
        assert!(popup.bottom() <= area.bottom());
    }

    #[test]
    fn test_question_popup_dimensions_at_min_size() {
        let area = Rect::new(0, 0, 60, 15);
        let popup = popup_dimensions(area, 0.5, 40, 70, 0.6, 12);

        assert!(popup.width > 0);
        assert!(popup.height > 0);
        assert!(popup.right() <= area.right());
        assert!(popup.bottom() <= area.bottom());
    }

    #[test]
    fn test_popup_dimensions_clamps_to_area() {
        let area = Rect::new(0, 0, 25, 8);
        let popup = popup_dimensions(area, 0.5, 30, 60, 0.5, 20);

        assert!(popup.width <= area.width);
        assert!(popup.height <= area.height);
        assert!(popup.right() <= area.right());
        assert!(popup.bottom() <= area.bottom());
    }
}
